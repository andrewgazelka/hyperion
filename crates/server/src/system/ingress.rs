use std::{alloc::Layout, any, ptr, ptr::NonNull};

use bumpalo::Bump;
use evenio::{
    entity::EntityId,
    event::{
        Despawn, EventMut, EventSet, GlobalEvent, GlobalEventIdx, Insert, ReceiverMut, Sender,
        Spawn, TargetedEvent, TargetedEventIdx,
    },
    fetch::{Fetcher, Single},
    world::World,
};
use parking_lot::Mutex;
use rayon_local::RayonLocal;
use serde_json::json;
use tracing::{debug, info, instrument, trace, warn};
use valence_protocol::{
    decode::PacketFrame,
    packets,
    packets::{handshaking::handshake_c2s::HandshakeNextState, login, login::LoginCompressionS2c},
    Packet, VarInt,
};

use crate::{
    event::{self},
    global::Global,
    singleton::fd_lookup::StreamLookup,
};

mod player_packet_buffer;

use crate::{
    components::{FullEntityPose, LoginState},
    net::{proxy::ReceiveStateInner, Compose, StreamId, MINECRAFT_VERSION, PROTOCOL_VERSION},
    packets::PacketSwitchQuery,
    singleton::player_id_lookup::EntityIdLookup,
    system::ingress::player_packet_buffer::DecodeBuffer,
};

pub type IngressEventSet = (
    event::PlayerInit,
    event::KickPlayer,
    event::InitEntity,
    event::SwingArm,
    event::AttackEntity,
    event::BlockStartBreak,
    event::BlockAbortBreak,
    event::BlockFinishBreak,
    event::Command,
    event::PoseUpdate,
    event::UpdateSelectedSlot,
    event::ClickEvent,
);

pub type ThreadLocalIngressSender<'a, 'b> = SenderLocal<'a, 'b, IngressEventSet>;
pub type IngressSender<'a> = Sender<'a, IngressEventSet>;

#[derive(GlobalEvent)]
pub struct AddPlayer {
    stream: u64,
}

#[derive(GlobalEvent)]
pub struct RemovePlayer {
    stream: u64,
}

// todo: do we really need three different lifetimes here?
#[derive(GlobalEvent)]
pub struct RecvDataBulk<'a> {
    events: &'a mut targeted_bulk::TargetedEvents<bytes::Bytes, u64>,
    bumps: &'a RayonLocal<&'a Bump>,
}

#[instrument(skip_all, level = "trace")]
#[allow(
    clippy::significant_drop_in_scrutinee,
    reason = "I think this is fine. However, let's double check in perf"
)]
pub fn generate_ingress_events(world: &mut World, shared: &Mutex<ReceiveStateInner>) {
    // todo: should we just lock once?

    let mut recv = shared.lock();

    for connect in recv.player_connect.drain(..) {
        world.send(AddPlayer {
            stream: connect.stream,
        });
    }

    for disconnect in recv.player_disconnect.drain(..) {
        world.send(RemovePlayer {
            stream: disconnect.stream,
        });
    }

    debug!("recv data bulk {}", recv.packets.len());

    // todo: call .reset() instead
    let bumps: RayonLocal<Bump> = RayonLocal::init_with_defaults();

    // todo: Is there a better name for `MapRef`?
    // Is there anything we can do to make this less complicated?
    // I feel like this is really complicated right now.
    let bumps_ref = bumps.map_ref(|bump| bump);

    world.send(RecvDataBulk {
        events: &mut recv.packets,
        bumps: &bumps_ref,
    });

    recv.packets.clear();
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn add_player(
    r: ReceiverMut<AddPlayer>,
    mut lookup: Single<&mut StreamLookup>,
    sender: Sender<(
        Spawn,
        Insert<LoginState>,
        Insert<DecodeBuffer>,
        Insert<StreamId>,
    )>,
) {
    info!("got a player with stream {}", r.event.stream);
    let event = r.event;

    let new_player = sender.spawn();
    sender.insert(new_player, LoginState::Handshake);
    sender.insert(new_player, DecodeBuffer::default());

    trace!("got a player with stream {}", event.stream);

    sender.insert(new_player, StreamId::new(event.stream));

    lookup.insert(event.stream, new_player);
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn remove_player(
    r: ReceiverMut<RemovePlayer>,
    mut lookup: Single<&mut StreamLookup>,
    sender: Sender<Despawn>,
) {
    let event = r.event;

    let stream = event.stream;

    let Some(id) = lookup.remove(&stream) else {
        warn!("tried to remove player with stream {stream:?} but it seemed to already be removed",);
        return;
    };

    sender.despawn(id);

    info!("removed a player with stream {stream}");
}

// todo: I'd love to see if we actually need the two lifetimes or not.
// We might be able to just use one for both of them, but I never really figure out when I need one versus two.
pub struct SenderLocal<'a, 'b, ES: EventSet> {
    state: &'b ES::Indices,
    bump: &'a Bump,
    pending_global: Vec<(NonNull<u8>, GlobalEventIdx)>,
    pending_targeted: Vec<(EntityId, NonNull<u8>, TargetedEventIdx)>,
}

impl<'a, 'b, ES: EventSet> SenderLocal<'a, 'b, ES> {
    pub fn new(state: &'b ES::Indices, bump: &'a Bump) -> Self {
        Self {
            state,
            bump,

            // todo: Potentially, this should be under bump allocator, but let's profile before trying to do that.
            pending_global: Vec::new(),
            pending_targeted: Vec::new(),
        }
    }

    // todo: what does track caller do?
    #[track_caller]
    pub fn send<E: GlobalEvent + 'static>(&mut self, event: E) {
        // The event type and event set are all compile time known, so the compiler
        // should be able to optimize this away.
        let event_idx = ES::find_index::<E>(self.state).unwrap_or_else(|| {
            panic!(
                "global event `{}` is not in the `EventSet` of this `Sender`",
                any::type_name::<E>()
            )
        });

        // let ptr = self.alloc_layout(Layout::new::<E>());
        let ptr = self.bump.alloc_layout(Layout::new::<E>());

        unsafe { ptr::write::<E>(ptr.as_ptr().cast(), event) };

        // This will be saved until the end of the system, where we will be sending the events in one thread synchronously.
        // unsafe { self.world.queue_global(ptr, GlobalEventIdx(event_idx)) };
        self.pending_global.push((ptr, GlobalEventIdx(event_idx)));
    }

    /// Add a [`TargetedEvent`] to the queue of events to send.
    ///
    /// The queue is flushed once all handlers for the current event have run.
    #[track_caller]
    pub fn send_to<E: TargetedEvent + 'static>(&mut self, target: EntityId, event: E) {
        // The event type and event set are all compile time known, so the compiler
        // should be able to optimize this away.
        let event_idx = ES::find_index::<E>(self.state).unwrap_or_else(|| {
            panic!(
                "targeted event `{}` is not in the `EventSet` of this `Sender`",
                any::type_name::<E>()
            )
        });

        let ptr = self.bump.alloc_layout(Layout::new::<E>());

        unsafe { ptr::write::<E>(ptr.as_ptr().cast(), event) };

        // unsafe {
        //     self.world
        //         .queue_targeted(target, ptr, TargetedEventIdx(event_idx))
        // };
        self.pending_targeted
            .push((target, ptr, TargetedEventIdx(event_idx)));
    }
}

#[instrument(skip_all, level = "trace")]
#[allow(clippy::too_many_arguments, reason = "todo")]
pub fn recv_data(
    r: ReceiverMut<RecvDataBulk>,
    stream_lookup: Single<&mut StreamLookup>,
    global: Single<&Global>,
    players: Fetcher<(
        &mut LoginState,
        &mut DecodeBuffer,
        &StreamId,
        Option<&mut FullEntityPose>,
    )>,
    id_lookup: Single<&EntityIdLookup>,
    compose: Compose,
    sender: IngressSender,
) {
    let recv_data = EventMut::take(r.event);

    let bumps = recv_data.bumps;

    let send_events: RayonLocal<SenderLocal<IngressEventSet>> =
        bumps.map_ref(|bump| SenderLocal::new(sender.state(), bump));

    recv_data.events.drain_par(|stream_id, data| {
        let stream_id = stream_id.inner();

        let Some(&entity_id) = stream_lookup.get(stream_id) else {
            warn!("recv data for stream id that does not exist: {stream_id}");
            return;
        };

        // We need to do this because it is technically unsafe to use `sendEvents`
        // since we can get multiple mutable references per thread.
        // However, as long as we do it once at the beginning of the scope and don't try to get it multiple times,
        // it should be fine.
        let send_events = send_events.get_local_raw();
        let send_events = unsafe { &mut *send_events.get() };

        // The reason we are using `get_unchecked`
        // is because there are mutable accesses which require exclusive access to players.
        // If we have two threads that are trying to access the same entity ID,
        // you have two mutable accesses and this is bad.
        // This is not supposed to happen with Rust's borrowing rules.
        // But we know that we only have one entity ID per thread because the mapping of stream IDs to entity IDs is one-to-one.
        // And the way that the targeted events work is that they only have certain data on certain threads,
        // so we would never have one ID on one thread and also on other threads.
        // todo: Is there a way to double-check that we are never aliasing incorrectly in debug?
        let (login_state, decoder, packets, mut pose) =
            unsafe { players.get_unchecked(entity_id) }.unwrap();

        decoder.queue_slice(data.as_ref());

        let scratch = compose.scratch();
        let mut scratch = scratch.borrow_mut();
        let scratch = &mut *scratch;

        while let Some(frame) = decoder.try_next_packet(scratch).unwrap() {
            match *login_state {
                LoginState::Handshake => process_handshake(login_state, &frame).unwrap(),
                LoginState::Status => {
                    process_status(login_state, &frame, *packets, &compose).unwrap();
                }
                LoginState::Terminate => {
                    // // todo: does this properly terminate the connection? I don't think so probably
                    // let Some(id) = fd_lookup.remove(&fd) else {
                    //     return;
                    // };
                    //
                    // sender.despawn(id);
                }
                LoginState::Login => {
                    process_login(
                        entity_id,
                        login_state,
                        &frame,
                        *packets,
                        decoder,
                        &global,
                        send_events,
                        &compose,
                    )
                    .unwrap();
                }
                LoginState::TransitioningPlay { .. } | LoginState::Play => {
                    if let LoginState::TransitioningPlay {
                        packets_to_transition,
                    } = login_state
                    {
                        if *packets_to_transition == 0 {
                            *login_state = LoginState::Play;
                        } else {
                            *packets_to_transition -= 1;
                        }
                    }

                    // We call this code when you're in play.
                    // Transitioning to play is just a way to make sure that the player is officially in play before we start sending them play packets.
                    // We have a certain duration that we wait before doing this.
                    if let Some(pose) = pose.as_mut() {
                        let mut query = PacketSwitchQuery {
                            id: entity_id,
                            pose,
                        };

                        crate::packets::switch(&frame, send_events, &id_lookup, &mut query)
                            .unwrap();
                    }
                }
            }
        }
    });

    // send the events

    for local in send_events {
        let world = sender.world();
        for (id, ptr, idx) in local.pending_targeted {
            unsafe { world.queue_targeted(id, ptr, idx) };
        }

        for (ptr, idx) in local.pending_global {
            unsafe { world.queue_global(ptr, idx) };
        }

        // todo: flush event queue
    }
}

fn process_handshake(login_state: &mut LoginState, packet: &PacketFrame) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Handshake);

    let handshake: packets::handshaking::HandshakeC2s = packet.decode()?;

    trace!("received handshake: {:?}", handshake);

    // todo: check version is correct

    match handshake.next_state {
        HandshakeNextState::Status => {
            *login_state = LoginState::Status;
        }
        HandshakeNextState::Login => {
            *login_state = LoginState::Login;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments, reason = "todo del")]
fn process_login(
    id: EntityId,
    login_state: &mut LoginState,
    packet: &PacketFrame,
    stream_id: StreamId,
    decoder: &mut DecodeBuffer,
    global: &Global,
    sender: &mut ThreadLocalIngressSender,
    compose: &Compose,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Login);

    let login::LoginHelloC2s { username, .. } = packet.decode()?;

    trace!("received LoginHello for {username}");

    let username = username.0;

    let pkt = LoginCompressionS2c {
        threshold: VarInt(global.shared.compression_threshold.0),
    };

    compose.unicast_no_compression(&pkt, stream_id).unwrap();

    decoder.set_compression(global.shared.compression_threshold);

    let username = Box::from(username);

    let elem = event::PlayerInit {
        username,
        pose: FullEntityPose::player(),
    };

    sender.send_to(id, elem);

    // todo: impl rest
    *login_state = LoginState::TransitioningPlay {
        packets_to_transition: 5,
    };

    Ok(())
}

fn process_status(
    login_state: &mut LoginState,
    packet: &PacketFrame,
    packets: StreamId,
    compose: &Compose,
) -> anyhow::Result<()> {
    debug_assert!(*login_state == LoginState::Status);

    match packet.id {
        packets::status::QueryRequestC2s::ID => {
            let query_request: packets::status::QueryRequestC2s = packet.decode()?;

            // https://wiki.vg/Server_List_Ping#Response
            let json = json!({
                "version": {
                    "name": MINECRAFT_VERSION,
                    "protocol": PROTOCOL_VERSION,
                },
                "players": {
                    "online": 1,
                    "max": 32,
                    "sample": [],
                },
                "description": "something"
            });

            let json = serde_json::to_string_pretty(&json)?;

            let send = packets::status::QueryResponseS2c { json: &json };

            trace!("sent query response: {query_request:?}");
            compose.unicast_no_compression(&send, packets).unwrap();
        }

        packets::status::QueryPingC2s::ID => {
            let query_ping: packets::status::QueryPingC2s = packet.decode()?;

            let payload = query_ping.payload;

            let send = packets::status::QueryPongS2c { payload };

            compose.unicast_no_compression(&send, packets).unwrap();

            trace!("sent query pong: {query_ping:?}");
            *login_state = LoginState::Terminate;
        }

        _ => panic!("unexpected packet id: {}", packet.id),
    }

    // todo: check version is correct

    Ok(())
}
