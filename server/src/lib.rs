#![allow(unused)]
#![allow(clippy::many_single_char_names)]

extern crate core;
mod chunk;

use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    sync::atomic::AtomicU32,
    time::{Duration, Instant},
};

use anyhow::Context;
use evenio::prelude::*;
use signal_hook::iterator::Signals;
use tracing::{info, instrument, warn};
use valence_protocol::math::DVec3;

use crate::{
    bounding_box::BoundingBox,
    io::{server, ClientConnection, Packets},
};

mod global;
mod io;

mod packets;
mod system;

mod bits;

mod quad_tree;

pub mod bounding_box;

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
pub struct InitEntity {
    pub pose: FullEntityPose,
}

/// If the entity can be targeted by non-player entities.
#[derive(Component)]
pub struct Targetable;

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
struct KillAllEntities;

#[derive(Event)]
struct TpsEvent {
    ms_per_tick: f64,
}

#[derive(Event)]
struct Gametick;

static GLOBAL: global::Global = global::Global {
    player_count: AtomicU32::new(0),
};

pub struct Game {
    world: World,
    last_ticks: VecDeque<Instant>,
    last_ms_per_tick: VecDeque<f64>,

    incoming: flume::Receiver<ClientConnection>,
}

impl Game {
    pub const fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    #[instrument]
    pub fn init() -> anyhow::Result<Self> {
        info!("Starting mc-server");

        let current_threads = evenio::rayon::current_num_threads();
        let max_threads = evenio::rayon::max_num_threads();

        info!("rayon\tcurrent threads: {current_threads}, max threads: {max_threads}");

        let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
            .context("failed to create signal handler")?;

        let (shutdown_tx, shutdown_rx) = flume::bounded(1);

        std::thread::spawn(move || {
            for _ in signals.forever() {
                warn!("Shutting down...");
                SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = shutdown_tx.send(());
            }
        });

        let server = server(shutdown_rx)?;

        let mut world = World::new();

        world.add_handler(system::init_player);
        world.add_handler(system::player_join_world);
        world.add_handler(system::player_kick);
        world.add_handler(system::entity_spawn);
        world.add_handler(system::entity_move_logic);
        world.add_handler(system::entity_detect_collisions);
        world.add_handler(system::reset_bounding_boxes);

        world.add_handler(system::keep_alive);
        world.add_handler(process_packets);
        world.add_handler(system::tps_message);
        world.add_handler(system::kill_all);

        let bounding_boxes = world.spawn();
        world.insert(bounding_boxes, bounding_box::EntityBoundingBoxes::default());

        let mut game = Self {
            world,
            last_ticks: VecDeque::default(),
            last_ms_per_tick: VecDeque::default(),
            incoming: server,
        };

        game.last_ticks.push_back(Instant::now());

        Ok(game)
    }

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

    #[instrument(skip_all)]
    pub fn game_loop(&mut self) {
        while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
            self.tick();

            if let Some(wait_duration) = self.wait_duration() {
                std::thread::sleep(wait_duration);
            }
        }
    }

    #[instrument(skip_all)]
    pub fn tick(&mut self) {
        const HISTORY_SIZE: usize = 100;

        let now = Instant::now();
        self.last_ticks.push_back(now);

        // let mut tps = None;
        if self.last_ticks.len() > HISTORY_SIZE {
            self.last_ticks.pop_front();
            // let ticks_per_second = 100.0 / (now - front).as_secs_f64();
            // tps = Some(ticks_per_second);
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
                    bounding: BoundingBox::create(DVec3::new(0.0, 2.0, 0.0), 0.6, 1.8),
                    yaw: 0.0,
                    pitch: 0.0,
                },
            };

            self.world.send(event);
        }

        self.world.send(Gametick);

        let ms = now.elapsed().as_nanos() as f64 / 1_000_000.0;
        self.last_ms_per_tick.push_back(ms);

        if self.last_ms_per_tick.len() > HISTORY_SIZE {
            self.last_ms_per_tick.pop_front();

            let ms_per_tick =
                self.last_ms_per_tick.iter().sum::<f64>() / self.last_ms_per_tick.len() as f64;

            self.world.send(TpsEvent { ms_per_tick });
        }

        // info!("Tick took: {:02.8}ms", ms);
    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all)]
fn process_packets(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(EntityId, &mut Player, &mut FullEntityPose)>,
    mut sender: Sender<(KickPlayer, InitEntity, KillAllEntities)>,
) {
    // todo: flume the best things to use here? also this really ust needs to be mpsc not mpmc
    // let (tx, rx) = flume::unbounded();

    fetcher.iter_mut().for_each(|(_id, player, position)| {
        // info!("Processing packets for player: {:?}", id);
        while let Ok(packet) = player.packets.reader.try_recv() {
            // info!("Received packet: {:?}", packet);
            if let Err(e) = packets::switch(packet, player, position, &mut sender) {
                let reason = format!("error: {e}");

                // todo: handle error
                let _ = player.packets.writer.send_chat_message(&reason);

                warn!("invalid packet: {reason}");
                // let _ = tx.send(KickPlayer { target: id, reason });
            }
        }
    });
}

static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Component, Copy, Clone, Debug)]
pub struct FullEntityPose {
    pub position: DVec3,
    pub yaw: f32,
    pub pitch: f32,
    pub bounding: BoundingBox,
}

impl FullEntityPose {
    fn move_by(&mut self, vec: DVec3) {
        self.position += vec;
        self.bounding = self.bounding.move_by(vec);
    }
}

#[derive(Debug, Default)]
pub struct EntityReactionInner {
    velocity: DVec3,
}

#[derive(Component, Debug, Default)]
pub struct EntityReaction(UnsafeCell<EntityReactionInner>);

impl EntityReaction {
    #[allow(dead_code)]
    fn get_mut(&mut self) -> &mut EntityReactionInner {
        self.0.get_mut()
    }
}

#[allow(clippy::undocumented_unsafe_blocks)]
unsafe impl Send for EntityReaction {}

#[allow(clippy::undocumented_unsafe_blocks)]
unsafe impl Sync for EntityReaction {}
