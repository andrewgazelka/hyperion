// You can run this example from the root of the mio repo:
// cargo run --example tcp_server --features="os-poll net"
use std::{
    hash::BuildHasherDefault,
    io::{self, IoSlice, Read, Write},
    net::{SocketAddr, ToSocketAddrs},
    time::Duration,
};

use anyhow::{bail, Context};
use bytes::BytesMut;
use fxhash::FxHashMap;
use libc::iovec;
use mio::{
    event::Event,
    net::{TcpListener, TcpStream},
    Events, Interest, Poll, Registry, Token,
};
use tracing::{info, instrument, warn};

use crate::{
    net::{Fd, ServerDef, ServerEvent, WriteItem, MAX_PACKET_SIZE},
    CowBytes,
};

// Setup some tokens to allow us to identify which event is for which socket.
const SERVER: Token = Token(0);

const EVENT_CAPACITY: usize = 128;

struct ConnectionInfo {
    pub to_write: Vec<IoSlice<'static>>,
    pub connection: TcpStream,
}

pub struct GenericServer {
    poll: Poll,
    events: Events,
    server: TcpListener,
    ids: Ids,
    connections: FxHashMap<usize, ConnectionInfo>,
}

struct Ids {
    token_on: usize,
}

impl Ids {
    fn generate_unique_token(&mut self) -> Token {
        let next = self.token_on;
        self.token_on += 1;
        Token(next)
    }
}

impl ServerDef for GenericServer {
    fn new(address: SocketAddr) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        // todo: will this use kqueue on macOS? I hope it will 🥲
        let poll = Poll::new()?;
        // Create storage for events.
        let events = Events::with_capacity(EVENT_CAPACITY);

        let address = address
            .to_socket_addrs()?
            .next()
            .context("could not get first address")?;

        info!("using generic I/O server and listening on {address}");
        let mut server = TcpListener::bind(address)?;

        // Register the server with poll we can receive events for it.
        poll.registry()
            .register(&mut server, SERVER, Interest::READABLE)?;

        // Map of `Token` -> `TcpStream`.
        // todo: is there a more idiomatic way to do this?
        let connections = FxHashMap::with_hasher(BuildHasherDefault::default());

        Ok(Self {
            poll,
            connections,
            events,
            ids: Ids { token_on: 1 },
            server,
        })
    }

    #[instrument(skip_all, level = "trace")]
    fn drain<'a>(&'a mut self, mut f: impl FnMut(ServerEvent<'a>)) -> anyhow::Result<()> {
        // // todo: this is a bit of a hack, is there a better number? probs dont want people sending more than this
        let mut received_data = BytesMut::with_capacity(MAX_PACKET_SIZE * 2);

        // process the current tick
        if let Err(err) = self
            .poll
            .poll(&mut self.events, Some(Duration::from_nanos(10)))
        {
            if interrupted(&err) {
                return Ok(());
            }

            bail!("failed to poll: {err}");
        }

        for event in &self.events {
            match event.token() {
                SERVER => loop {
                    // Received an event for the TCP server socket, which
                    // indicates we can accept a connection.
                    let (mut connection, _) = match self.server.accept() {
                        Ok((connection, address)) => (connection, address),
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            // If we get a `WouldBlock` error we know our
                            // listener has no more incoming connections queued,
                            // so we can return to polling and wait for some
                            // more.
                            break;
                        }
                        Err(e) => {
                            // If it was any other kind of error, something went
                            // wrong and we terminate with an error.
                            bail!("failed to accept connection: {e}");
                        }
                    };

                    let token = self.ids.generate_unique_token();
                    self.poll.registry().register(
                        &mut connection,
                        token,
                        Interest::READABLE.add(Interest::WRITABLE),
                    )?;

                    self.connections.insert(token.0, ConnectionInfo {
                        to_write: Vec::new(),
                        connection,
                    });

                    f(ServerEvent::AddPlayer { fd: Fd(token.0) });
                },
                token => {
                    // Maybe received an event for a TCP connection.
                    let done = if let Some(connection) = self.connections.get_mut(&token.0) {
                        received_data.clear();
                        let result = handle_connection_event(
                            self.poll.registry(),
                            connection,
                            event,
                            &mut received_data,
                            token,
                            &mut f,
                        );

                        result.unwrap_or_else(|err| {
                            warn!("failed to handle connection event: {err}");
                            true
                        })
                    } else {
                        // Sporadic events happen, we can safely ignore them.
                        false
                    };
                    if done {
                        if let Some(mut connection) = self.connections.remove(&token.0) {
                            self.poll
                                .registry()
                                .deregister(&mut connection.connection)?;
                        }
                        f(ServerEvent::RemovePlayer { fd: Fd(token.0) });
                    }
                }
            }
        }
        Ok(())
    }

    // todo: make unsafe
    unsafe fn register_buffers(&mut self, _buffers: &[iovec]) {
        // nop
    }

    fn write(&mut self, write: WriteItem) {
        let WriteItem { info, fd, .. } = write;

        let Some(to_write) = self.connections.get_mut(&fd.0) else {
            warn!("no connection for fd {fd:?}");
            return;
        };

        let io_slice = IoSlice::new(unsafe { info.as_static_slice() });
        to_write.to_write.push(io_slice);
    }

    fn submit_events(&mut self) {
        // todo
    }
}

/// Returns `true` if the connection is done.
fn handle_connection_event<'a>(
    registry: &Registry,
    info: &mut ConnectionInfo,
    event: &Event,
    received_data: &mut BytesMut,
    token: Token,
    f: &mut impl FnMut(ServerEvent<'a>),
) -> anyhow::Result<bool> {
    if event.is_writable() {
        let empty = info.to_write.is_empty() || info.to_write[0].len() == 0;

        if empty {
            registry.reregister(
                &mut info.connection,
                event.token(),
                Interest::READABLE.add(Interest::WRITABLE),
            )?;

            return Ok(false);
        }

        let connection = &mut info.connection;

        // We can (maybe) write to the connection.
        match connection.write_vectored(&info.to_write) {
            Ok(n) => {
                let mut slice = info.to_write.as_mut_slice();
                IoSlice::advance_slices(&mut slice, n);
                let new_len = slice.len();
                let removed = info.to_write.len() - new_len;

                info.to_write.drain(..removed);

                for _ in 0..removed {
                    f(ServerEvent::SentData { fd: Fd(token.0) });
                }

                registry.reregister(
                    &mut info.connection,
                    event.token(),
                    Interest::READABLE.add(Interest::WRITABLE),
                )?;
            }
            // Would block "errors" are the OS's way of saying that the
            // connection is not actually ready to perform this I/O operation.
            Err(ref err) if would_block(err) => {}
            // Got interrupted (how rude!), we'll try again.
            Err(ref err) if interrupted(err) => {
                return handle_connection_event(registry, info, event, received_data, token, f)
            }
            // Other errors we'll consider fatal.
            Err(err) => bail!("failed to write to connection: {err}"),
        }
    }

    if event.is_readable() {
        let mut connection_closed = false;
        let mut bytes_read = 0;
        // We can (maybe) read from the connection.

        // todo: remove setting 0's, just use MaybeUninit
        received_data.resize(1024, 0);

        loop {
            match info.connection.read(&mut received_data[bytes_read..]) {
                Ok(0) => {
                    // Reading 0 bytes means the other side has closed the
                    // connection or is done writing, then so are we.
                    connection_closed = true;
                    break;
                }
                Ok(n) => {
                    bytes_read += n;
                    if bytes_read == received_data.len() {
                        received_data.resize(received_data.len() + 1024, 0);
                    }
                }
                // Would block "errors" are the OS's way of saying that the
                // connection is not actually ready to perform this I/O operation.
                Err(ref err) if would_block(err) => break,
                Err(ref err) if interrupted(err) => continue,
                // Other errors we'll consider fatal.
                Err(err) => bail!("failed to read from connection: {err}"),
            }
        }

        if bytes_read != 0 {
            let received_data = received_data.split_to(bytes_read);

            f(ServerEvent::RecvData {
                fd: Fd(token.0),
                data: CowBytes::Owned(received_data.freeze()),
            });
        }

        if connection_closed {
            return Ok(true);
        }
    }

    Ok(false)
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}
