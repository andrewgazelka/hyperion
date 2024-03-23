#![allow(clippy::many_single_char_names)]

extern crate core;
mod chunk;

#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::{
    sync::atomic::AtomicU32,
    time::{Duration, Instant},
};
use std::collections::VecDeque;

use evenio::prelude::*;
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

#[derive(Component, Copy, Clone)]
struct Uuid(uuid::Uuid);

#[derive(Event)]
struct InitEntity {
    pose: FullEntityPose,
}

#[derive(Event)]
struct PlayerJoinWorld {
    #[event(target)]
    target: EntityId,
}

#[derive(Component, Debug)]
pub struct MinecraftEntity;

#[derive(Component, Debug, Copy, Clone)]
pub struct RunningSpeed(f64);

impl Default for RunningSpeed {
    fn default() -> Self {
        Self(0.1)
    }
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
    last_ticks: VecDeque<Instant>,
    incoming: flume::Receiver<ClientConnection>,
}

impl Game {
    fn wait_duration(&self) -> Option<Duration> {
        let &first_tick = self.last_ticks.front()?;

        let count = self.last_ticks.len();

        let time_for_20_tps = first_tick + Duration::from_secs_f64(count as f64 / 20.0);

        // aim for 20 ticks per second
        let now = Instant::now();

        if time_for_20_tps < now {
            return None;
        }

        let duration = time_for_20_tps - now;

        // this is a bit of a hack to be conservative when sleeping
        Some(duration.mul_f64(0.8))
    }

    fn tick(&mut self) {
        const HISTORY_SIZE: usize = 100;

        let now = Instant::now();
        self.last_ticks.push_back(now);

        if self.last_ticks.len() > HISTORY_SIZE {
            let front = self.last_ticks.pop_front().unwrap();
            let ticks_per_second = 100.0 / (now - front).as_secs_f64();

            info!("Ticks per second: {:?}", ticks_per_second);
        }


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
    mut sender: Sender<(KickPlayer, InitEntity)>,
) {
    // todo: flume the best things to use here? also this really ust needs to be mpsc not mpmc
    // let (tx, rx) = flume::unbounded();

    fetcher.iter_mut().for_each(|(_id, player, position)| {
        // info!("Processing packets for player: {:?}", id);
        while let Ok(packet) = player.packets.reader.try_recv() {
            // info!("Received packet: {:?}", packet);
            if let Err(e) = packets::switch(packet, player, position, &mut sender) {
                let reason = format!("error: {e}");

                player.packets.writer.send_chat_message(&reason).unwrap();

                warn!("invalid packet: {reason}");
                // let _ = tx.send(KickPlayer { target: id, reason });
            }
        }
    });
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

    let mut world = World::new();

    world.add_handler(system::init_player);
    world.add_handler(system::player_join_world);
    world.add_handler(system::player_kick);
    world.add_handler(system::entity_spawn);
    world.add_handler(system::entity_move_logic);

    world.add_handler(system::keep_alive);
    world.add_handler(process_packets);

    let mut game = Game {
        world,
        last_ticks: VecDeque::default(),
        incoming: server,
    };

    game.last_ticks.push_back(Instant::now());

    while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
        game.tick();

        if let Some(wait_duration) = game.wait_duration() {
            std::thread::sleep(wait_duration);
        }
    }
}

#[derive(Component, Copy, Clone, Debug)]
pub struct FullEntityPose {
    pub position: DVec3,
    pub yaw: f32,
    pub pitch: f32,
}
