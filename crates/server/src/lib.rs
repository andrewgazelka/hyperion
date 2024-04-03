#![feature(thread_local)]
#![feature(lint_reasons)]
#![expect(clippy::type_complexity, reason = "evenio uses a lot of complex types")]

#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

extern crate core;
mod chunk;
mod singleton;

use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    sync::atomic::AtomicU32,
    time::{Duration, Instant},
};

use anyhow::Context;
use evenio::prelude::*;
use jemalloc_ctl::{epoch, stats};
use ndarray::s;
use signal_hook::iterator::Signals;
use spin::Lazy;
use tracing::{info, instrument, warn};
use valence_protocol::math::Vec3;

use crate::{
    bounding_box::BoundingBox,
    net::{server, ClientConnection, Packets, GLOBAL_PACKETS},
    singleton::{
        encoder::Encoder, player_location_lookup::PlayerLocationLookup,
        player_lookup::PlayerUuidLookup,
    },
};

mod global;
mod net;

mod packets;
mod system;

mod bits;

mod quad_tree;

pub mod bounding_box;
mod config;

// A zero-sized component, often called a "marker" or "tag".
#[derive(Component)]
struct Player {
    packets: Packets,
    name: Box<str>,
    last_keep_alive_sent: Instant,

    /// Set to true if a keep alive has been sent to the client and the client hasn't responded.
    unresponded_keep_alive: bool,

    /// The player's ping. This is likely higher than the player's real ping.
    ping: Duration,

    locale: Option<String>,
}

#[derive(Event)]
struct InitPlayer {
    entity: EntityId,
    io: Packets,
    name: Box<str>,
    uuid: uuid::Uuid,
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
pub struct RunningSpeed(f32);

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

#[derive(Event, Copy, Clone)]
struct StatsEvent {
    ms_per_tick_mean_1s: f64,
    ms_per_tick_mean_5s: f64,
    #[expect(dead_code, reason = "not used currently, will in future")]
    allocated: usize,
    resident: usize,
}

#[expect(clippy::cast_precision_loss, reason = "2^52 bytes is over 1000 TB")]
fn bytes_to_mb(bytes: usize) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}

#[derive(Event)]
struct Gametick;

#[derive(Event)]
struct BroadcastPackets;

static SHARED: global::Shared = global::Shared {
    player_count: AtomicU32::new(0),
};

pub struct Game {
    world: World,
    last_ticks: VecDeque<Instant>,
    last_ms_per_tick: VecDeque<f64>,
    tick_on: u64,
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
        Lazy::force(&config::CONFIG);

        // if linux
        #[cfg(target_os = "linux")]
        {
            info!("Running on linux");

            if let Err(e) = try_io_uring() {
                warn!("io_uring not supported: {e}");
            } else {
                info!("io_uring supported");
            }
        }

        let current_threads = rayon::current_num_threads();
        let max_threads = rayon::max_num_threads();

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

        let incoming = server(shutdown_rx)?;

        let mut world = World::new();

        world.add_handler(system::init_player);
        world.add_handler(system::player_join_world);
        world.add_handler(system::player_kick);
        world.add_handler(system::entity_spawn);
        world.add_handler(system::entity_move_logic);
        world.add_handler(system::entity_detect_collisions);
        world.add_handler(system::reset_bounding_boxes);
        world.add_handler(system::update_time);
        world.add_handler(system::rebuild_player_location);

        world.add_handler(system::broadcast_packets);
        world.add_handler(system::keep_alive);
        world.add_handler(process_packets);
        world.add_handler(system::tps_message);
        world.add_handler(system::kill_all);

        let global = world.spawn();
        world.insert(global, global::Global::default());

        let bounding_boxes = world.spawn();
        world.insert(bounding_boxes, bounding_box::EntityBoundingBoxes::default());

        let uuid_lookup = world.spawn();
        world.insert(uuid_lookup, PlayerUuidLookup::default());

        let player_location_lookup = world.spawn();
        world.insert(player_location_lookup, PlayerLocationLookup::default());

        let encoder = world.spawn();
        world.insert(encoder, Encoder::default());

        let mut game = Self {
            world,
            last_ticks: VecDeque::default(),
            last_ms_per_tick: VecDeque::default(),
            tick_on: 0,
            incoming,
        };

        game.last_ticks.push_back(Instant::now());

        Ok(game)
    }

    fn wait_duration(&self) -> Option<Duration> {
        let &first_tick = self.last_ticks.front()?;

        let count = self.last_ticks.len();

        #[expect(clippy::cast_precision_loss, reason = "count is limited to 100")]
        let time_for_20_tps = { first_tick + Duration::from_secs_f64(count as f64 / 20.0) };

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
        const LAST_TICK_HISTORY_SIZE: usize = 100;
        const MSPT_HISTORY_SIZE: usize = 100;

        let now = Instant::now();
        self.last_ticks.push_back(now);

        // let mut tps = None;
        if self.last_ticks.len() > LAST_TICK_HISTORY_SIZE {
            self.last_ticks.pop_front();
        }

        while let Ok(connection) = self.incoming.try_recv() {
            let ClientConnection {
                packets,
                name,
                uuid,
            } = connection;

            let player = self.world.spawn();

            let event = InitPlayer {
                entity: player,
                io: packets,
                name,
                uuid,
                pos: FullEntityPose {
                    position: Vec3::new(0.0, 2.0, 0.0),
                    bounding: BoundingBox::create(Vec3::new(0.0, 2.0, 0.0), 0.6, 1.8),
                    yaw: 0.0,
                    pitch: 0.0,
                },
            };

            self.world.send(event);
        }

        self.world.send(Gametick);
        self.world.send(BroadcastPackets);

        #[expect(
            clippy::cast_precision_loss,
            reason = "realistically, nanoseconds between last tick will not be greater than 2^52 \
                      (~52 days)"
        )]
        let ms = now.elapsed().as_nanos() as f64 / 1_000_000.0;
        self.last_ms_per_tick.push_back(ms);

        if self.last_ms_per_tick.len() > MSPT_HISTORY_SIZE {
            // efficient
            let arr = ndarray::Array::from_iter(self.last_ms_per_tick.iter().copied().rev());

            // last 1 second (20 ticks) 5 seconds (100 ticks) and 25 seconds (500 ticks)
            let mean_1_second = arr.slice(s![..20]).mean().unwrap();
            let mean_5_seconds = arr.slice(s![..100]).mean().unwrap();

            let allocated = stats::allocated::mib().unwrap();
            let resident = stats::resident::mib().unwrap();

            let e = epoch::mib().unwrap();

            // todo: profile; does this need to be done in a separate thread?
            // if self.tick_on % 100 == 0 {
            e.advance().unwrap();
            // }

            let allocated = allocated.read().unwrap();
            let resident = resident.read().unwrap();

            self.world.send(StatsEvent {
                ms_per_tick_mean_1s: mean_1_second,
                ms_per_tick_mean_5s: mean_5_seconds,
                allocated,
                resident,
            });

            self.last_ms_per_tick.pop_front();
        }

        self.tick_on += 1;

        // info!("Tick took: {:02.8}ms", ms);
    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
#[instrument(skip_all)]
fn process_packets(
    _: Receiver<Gametick>,
    mut fetcher: Fetcher<(EntityId, &mut Player, &mut FullEntityPose)>,
    lookup: Single<&PlayerUuidLookup>,
    mut sender: Sender<(KickPlayer, InitEntity, KillAllEntities)>,
) {
    // uuid to entity id map

    let packets: Vec<_> = core::mem::take(&mut *GLOBAL_PACKETS.lock());

    let lookup = lookup.0;

    for packet in packets {
        let id = packet.user;
        let Some(&user) = lookup.get(&id) else { return };

        let Ok((_, player, position)) = fetcher.get_mut(user) else {
            return;
        };

        let packet = packet.packet;

        if let Err(e) = packets::switch(packet, player, position, &mut sender) {
            let reason = format!("error: {e}");

            // todo: handle error
            let _ = player.packets.writer.send_chat_message(&reason);

            warn!("invalid packet: {reason}");
        }
    }
}

static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Component, Copy, Clone, Debug)]
pub struct FullEntityPose {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub bounding: BoundingBox,
}

impl FullEntityPose {
    pub fn move_by(&mut self, vec: Vec3) {
        self.position += vec;
        self.bounding = self.bounding.move_by(vec);
    }
    
    pub fn move_to(&mut self, pos: Vec3) {
        self.bounding = self.bounding.move_to(pos);
        self.position = pos;
    }
}

#[derive(Debug, Default)]
pub struct EntityReactionInner {
    velocity: Vec3,
}

#[derive(Component, Debug, Default)]
pub struct EntityReaction(UnsafeCell<EntityReactionInner>);

impl EntityReaction {
    fn get_mut(&mut self) -> &mut EntityReactionInner {
        self.0.get_mut()
    }
}

unsafe impl Send for EntityReaction {}

unsafe impl Sync for EntityReaction {}

#[cfg(target_os = "linux")]
fn try_io_uring() -> anyhow::Result<()> {
    use std::{fs, os::unix::io::AsRawFd};

    use io_uring::{opcode, types, IoUring};

    let mut ring = IoUring::new(8)?;

    let fd = fs::File::open("/dev/urandom")?;
    let mut buf = vec![0; 1024];

    let read_e = opcode::Read::new(
        types::Fd(fd.as_raw_fd()),
        buf.as_mut_ptr(),
        buf.len().try_into()?,
    )
    .build()
    .user_data(0x42);

    unsafe {
        ring.submission()
            .push(&read_e)
            .expect("submission queue is full");
    }

    ring.submit_and_wait(1)?;

    let cqe = ring.completion().next().expect("completion queue is empty");

    assert_eq!(cqe.user_data(), 0x42);
    assert!(cqe.result() >= 0, "read error: {}", cqe.result());

    Ok(())
}
