use std::io::BufReader;

use ser::{PacketData, Readable};

fn main() -> anyhow::Result<()> {
    // start socket 25565
    let listener = std::net::TcpListener::bind("0.0.0.0:25565")?;

    // accept incoming connections
    for stream in listener.incoming() {
        let stream = stream?;
        let mut buf = BufReader::new(stream);

        let handshake: PacketData<protocol_765::client::Handshake> = PacketData::read(&mut buf)?;

        println!("{handshake:?}");
    }

    Ok(())
}
