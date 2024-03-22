#![allow(clippy::many_single_char_names)]

extern crate core;
mod chunk;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    sync::atomic::AtomicU32,
    time::{Duration, Instant},
};

use evenio::{prelude::*, rayon::prelude::*};
use signal_hook::iterator::Signals;
use tracing::{info, warn};
use valence_protocol::math::DVec3;

use crate::handshake::{server, ClientConnection, Packets};

mod global;
mod handshake;

mod packets;
mod system;

// A zero-sized component, often called a "marker" or "tag".
#[derive(Component)]
struct Player {
    packets: Packets,
    name: Box<str>,
    last_keep_alive_sent: Instant,
    locale: Option<String>,
}

#[derive(Event)]
struct InitPlayer {
    entity: EntityId,
    io: Packets,
    name: Box<str>,
    pos: FullEntityPose,
}

#[derive(Event)]
struct PlayerJoinWorld {
    #[event(target)]
    target: EntityId,
}

#[derive(Event)]
struct KickPlayer {
    #[event(target)] // Works on tuple struct fields as well.
    target: EntityId,
    reason: String,
}

#[derive(Event)]
struct Gametick;

static GLOBAL: global::Global = global::Global {
    player_count: AtomicU32::new(0),
};

struct Game {
    world: World,
    incoming: flume::Receiver<ClientConnection>,
}

impl Game {
    fn tick(&mut self) {
        while let Ok(connection) = self.incoming.try_recv() {
            let ClientConnection { packets, name } = connection;

            let player = self.world.spawn();

            let event = InitPlayer {
                entity: player,
                io: packets,
                name,
                pos: FullEntityPose {
                    position: DVec3::new(0.0, 2.0, 0.0),
                    yaw: 0.0,
                    pitch: 0.0,
                },
            };

            self.world.send(event);
        }

        self.world.send(Gametick);
    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
fn process_packets(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(EntityId, &mut Player, &mut FullEntityPose)>,
    mut sender: Sender<KickPlayer>,
) {
    // todo: flume the best things to use here? also this really ust needs to be mpsc not mpmc
    let (tx, rx) = flume::unbounded();

    fetcher.par_iter_mut().for_each(|(id, player, position)| {
        // info!("Processing packets for player: {:?}", id);
        while let Ok(packet) = player.packets.reader.try_recv() {
            // info!("Received packet: {:?}", packet);
            if let Err(e) = packets::switch(packet, player, position) {
                let reason = format!("Invalid packet: {e}");
                let _ = tx.send(KickPlayer { target: id, reason });
            }
        }
    });

    for kick in rx.drain() {
        sender.send(kick);
    }
}

static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn main() {
    tracing_subscriber::fmt::init();

    info!("Starting mc-server");

    let mut signals =
        Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]).unwrap();

    let (shutdown_tx, shutdown_rx) = flume::bounded(1);

    std::thread::spawn(move || {
        for _ in signals.forever() {
            warn!("Shutting down...");
            SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
            shutdown_tx.send(()).unwrap();
        }
    });

    let server = server(shutdown_rx);

    let mut last_tick = Instant::now();
    let tick_duration = Duration::from_millis(20);

    let mut world = World::new();

    world.add_handler(system::init_player);
    world.add_handler(system::player_join_world);
    world.add_handler(system::player_kick);

    world.add_handler(system::keep_alive);
    world.add_handler(process_packets);

    let mut game = Game {
        world,
        incoming: server,
    };

    while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
        game.tick();

        // Calculate the elapsed time since the last tick
        let elapsed = last_tick.elapsed();

        // If the elapsed time is greater than the desired tick duration,
        // skip the sleep to catch up
        if elapsed < tick_duration {
            let sleep_duration = tick_duration - elapsed;
            // println!("Sleeping for {:?}", sleep_duration);
            std::thread::sleep(sleep_duration);
        }

        // Update the last tick time
        last_tick = Instant::now();
    }
}

#[derive(Component)]
struct FullEntityPose {
    position: DVec3,
    yaw: f32,
    pitch: f32,
}
