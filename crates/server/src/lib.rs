//! Hyperion

#![feature(lint_reasons)]
#![expect(clippy::type_complexity, reason = "evenio uses a lot of complex types")]

mod chunk;
mod singleton;

use std::{
    collections::VecDeque,
    fmt::Debug,
    net::ToSocketAddrs,
    sync::{atomic::AtomicU32, Arc},
    time::{Duration, Instant},
};

use anyhow::Context;
use bvh::aabb::Aabb;
use evenio::prelude::*;
use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
use ndarray::s;
use signal_hook::iterator::Signals;
use singleton::bounding_box;
use spin::Lazy;
use tracing::{debug, error, info, instrument, trace, warn};
use valence_protocol::{math::Vec3, CompressionThreshold, Hand};

use crate::{
    global::Global,
    net::{init_io_thread, ClientConnection, Connection, Encoder},
    singleton::{
        broadcast::BroadcastBuf, player_aabb_lookup::PlayerBoundingBoxes,
        player_id_lookup::PlayerIdLookup, player_uuid_lookup::PlayerUuidLookup,
    },
    tracker::Tracker,
};

mod global;
mod net;

mod packets;
mod system;

mod bits;

mod pose;
mod tracker;

mod config;

/// History size for sliding average.
const MSPT_HISTORY_SIZE: usize = 100;

/// The absorption effect
#[derive(Clone, PartialEq, Debug)]
#[repr(packed)]
struct Absorption {
    /// This effect goes away on the tick with the value `end_tick`,
    end_tick: u64,
    /// The amount of health that is allocated to the absorption effect
    bonus_health: f32,
}

#[derive(Clone, PartialEq, Debug)]
struct Regeneration {
    /// This effect goes away on the tick with the value `end_tick`.
    end_tick: u64,
}

#[derive(Clone, PartialEq, Debug)]
enum PlayerState {
    /// If the player is alive
    Alive {
        /// Measured in half hearts
        health: f32,

        /// The absorption effect
        absorption: Absorption,
        /// The regeneration effect
        regeneration: Regeneration,
    },
    /// If the player is dead
    Dead {
        /// The tick the player will be respawned
        respawn_tick: u64,
    },
}

impl Default for Tracker<PlayerState> {
    fn default() -> Self {
        let mut value = Self::new(PlayerState::Alive {
            health: 0.0,
            absorption: Absorption {
                end_tick: 0,
                bonus_health: 0.0,
            },
            regeneration: Regeneration { end_tick: 0 },
        });
        // This update is needed so that a s2c packet updating the client's health is sent. If
        // this isn't sent, the client won't show the first time it takes damage as actual damage
        // because it believes that the server is simply updating its health to the amount of
        // health the player had when it left.
        value.update(|state| {
            let PlayerState::Alive { health, .. } = state else {
                unreachable!()
            };
            *health = 20.0;
        });
        value
    }
}

/// A component that represents a Player. In the future, this should be broken up into multiple components.
///
/// Why should it be broken up? The more things are broken up, the more we can take advantage of Rust borrowing rules.
#[derive(Component)]
pub struct Player {
    /// The name of the player i.e., `Emerald_Explorer`.
    name: Box<str>,

    /// The last time the player was sent a keep alive packet. TODO: this should be using a tick.
    last_keep_alive_sent: Instant,

    /// Set to true if a keep alive has been sent to the client and the client hasn't responded.
    unresponded_keep_alive: bool,

    /// The player's ping. This is likely higher than the player's real ping.
    ping: Duration,

    /// The locale of the player. This could in the future be used to determine the language of the player's chat messages.
    locale: Option<String>,

    /// The state of the player.
    state: Tracker<PlayerState>,

    /// The time until the player is immune to being hurt in ticks.
    immune_until: u64,
}

impl Player {
    /// Heal the player by a given amount.
    fn heal(&mut self, amount: f32) {
        assert!(amount.is_finite());
        assert!(amount > 0.0);

        self.state.update(|state| {
            let PlayerState::Alive { health, .. } = state else {
                return;
            };
            *health = (*health + amount).min(20.0);
        });
    }

    /// If the player is immune to being hurt, this returns false.
    const fn is_invincible(&self, global: &Global) -> bool {
        let tick = global.tick.unsigned_abs();

        if tick < self.immune_until {
            return true;
        }

        false
    }

    /// Hurt the player by a given amount.
    fn hurt(&mut self, global: &Global, mut amount: f32) {
        debug_assert!(amount.is_finite());
        debug_assert!(amount > 0.0);

        if self.is_invincible(global) {
            return;
        }

        let tick = global.tick.unsigned_abs();

        let max_hurt_resistant_time = global.max_hurt_resistant_time;

        self.immune_until = tick + u64::from(max_hurt_resistant_time) / 2;

        self.state.update(|state| {
            let PlayerState::Alive {
                health, absorption, ..
            } = state
            else {
                return;
            };

            if tick < absorption.end_tick {
                if amount > absorption.bonus_health {
                    amount -= absorption.bonus_health;
                    absorption.bonus_health = 0.0;
                } else {
                    absorption.bonus_health -= amount;
                    return;
                }
            }
            *health -= amount;

            if *health <= 0.0 {
                *state = PlayerState::Dead {
                    respawn_tick: tick + 100,
                };
            }
        });
    }
}

#[allow(clippy::missing_docs_in_private_items, reason = "self-explanatory")]
#[derive(Event)]
struct InitPlayer {
    entity: EntityId,
    encoder: Encoder,
    connection: Connection,
    name: Box<str>,
    uuid: uuid::Uuid,
    pos: FullEntityPose,
}

/// A UUID component. Generally speaking, this tends to be tied to entities with a [`Player`] component.
#[derive(Component, Copy, Clone, Debug)]
struct Uuid(uuid::Uuid);

/// Initialize a Minecraft entity (like a zombie) with a given pose.
#[derive(Event)]
pub struct InitEntity {
    /// The pose of the entity.
    pub pose: FullEntityPose,
}

/// If the entity can be targeted by non-player entities.
#[derive(Component)]
pub struct Targetable;

/// Sent whenever a player joins the server.
#[derive(Event)]
struct PlayerJoinWorld {
    /// The [`EntityId`] of the player.
    #[event(target)]
    target: EntityId,
}

/// Any living minecraft entity that is NOT a player.
///
/// Example: zombie, skeleton, etc.
#[derive(Component, Debug)]
pub struct MinecraftEntity;

/// The running multiplier of the entity. This defaults to 1.0.
#[derive(Component, Debug, Copy, Clone)]
pub struct RunningSpeed(f32);

impl Default for RunningSpeed {
    fn default() -> Self {
        Self(0.1)
    }
}

/// An event that is sent whenever a player is kicked from the server.
#[derive(Event)]
struct KickPlayer {
    /// The [`EntityId`] of the player.
    #[event(target)] // Works on tuple struct fields as well.
    target: EntityId,
    /// The reason the player was kicked.
    reason: String,
}

/// An event that is sent whenever a player swings an arm.
#[derive(Event)]
struct SwingArm {
    /// The [`EntityId`] of the player.
    #[event(target)]
    target: EntityId,
    /// The hand the player is swinging.
    hand: Hand,
}

#[derive(Event)]
struct AttackEntity {
    /// The [`EntityId`] of the player.
    #[event(target)]
    target: EntityId,
    /// The location of the player that is hitting.
    from_pos: Vec3,
}

/// An event to kill all minecraft entities (like zombies, skeletons, etc). This will be sent to the equivalent of
/// `/killall` in the game.
#[derive(Event)]
struct KillAllEntities;

/// An event when server stats are updated.
#[derive(Event, Copy, Clone)]
struct StatsEvent {
    /// The number of milliseconds per tick in the last second.
    ms_per_tick_mean_1s: f64,
    /// The number of milliseconds per tick in the last 5 seconds.
    ms_per_tick_mean_5s: f64,
}

#[derive(Event)]
struct Gametick;

/// An event that is sent when it is time to send packets to clients.
#[derive(Event)]
struct Egress;

/// on macOS, the soft limit for the number of open file descriptors is often 256. This is far too low
/// to test 10k players with.
/// This attempts to the specified `recommended_min` value.
pub fn adjust_file_limits(recommended_min: u64) -> std::io::Result<()> {
    let mut limits = libc::rlimit {
        rlim_cur: 0, // Initialize soft limit to 0
        rlim_max: 0, // Initialize hard limit to 0
    };

    if unsafe { getrlimit(RLIMIT_NOFILE, &mut limits) } == 0 {
        info!("Current file handle soft limit: {}", limits.rlim_cur);
        info!("Current file handle hard limit: {}", limits.rlim_max);
    } else {
        error!("Failed to get the current file handle limits");
        return Err(std::io::Error::last_os_error());
    };

    if limits.rlim_max < recommended_min {
        warn!(
            "Could only set file handle limit to {}. Recommended minimum is {}",
            limits.rlim_cur, recommended_min
        );
    }

    limits.rlim_cur = limits.rlim_max;
    info!("Setting soft limit to: {}", limits.rlim_cur);

    if unsafe { setrlimit(RLIMIT_NOFILE, &limits) } != 0 {
        error!("Failed to set the file handle limits");
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

/// The central [`Game`] struct which owns and manages the entire server.
pub struct Game {
    /// The shared state between the ECS framework and the I/O thread.
    shared: Arc<global::Shared>,
    /// The manager of the ECS framework.
    world: World,
    /// Data for what time the last ticks occurred.
    last_ticks: VecDeque<Instant>,
    /// Data for how many milliseconds previous ticks took.
    last_ms_per_tick: VecDeque<f64>,
    /// The tick of the game. This is incremented every 50 ms.
    tick_on: u64,
    /// The event that is sent when it is time to receive packets from clients.
    incoming: flume::Receiver<ClientConnection>,
    /// The event that is sent when the server is shutting down. This allows shutting down the I/O thread.
    shutdown_tx: flume::Sender<()>,
}

impl Game {
    /// Get the [`World`] which is the core part of the ECS framework.
    pub const fn world(&self) -> &World {
        &self.world
    }

    /// Get all shared data that is shared between the ECS framework and the IO thread.
    pub const fn shared(&self) -> &Arc<global::Shared> {
        &self.shared
    }

    /// See [`Game::world`].
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// # Panics
    /// This function will panic if the game is already shutdown.
    pub fn shutdown(&self) {
        self.shutdown_tx.send(()).unwrap();
    }

    /// Initialize the server.
    pub fn init(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<Self> {
        info!("Starting hyperion");
        Lazy::force(&config::CONFIG);

        let current_threads = rayon::current_num_threads();
        let max_threads = rayon::max_num_threads();

        info!("rayon: current threads: {current_threads}, max threads: {max_threads}");

        let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
            .context("failed to create signal handler")?;

        let (shutdown_tx, shutdown_rx) = flume::bounded(1);

        std::thread::spawn({
            let shutdown_tx = shutdown_tx.clone();
            move || {
                for _ in signals.forever() {
                    warn!("Shutting down...");
                    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                    let _ = shutdown_tx.send(());
                }
            }
        });

        let shared = Arc::new(global::Shared {
            player_count: AtomicU32::new(0),
            compression_level: CompressionThreshold(64),
        });

        let incoming = init_io_thread(shutdown_rx, address, shared.clone())?;

        let mut world = World::new();

        world.add_handler(system::ingress);
        world.add_handler(system::init_player);
        world.add_handler(system::player_join_world);
        world.add_handler(system::player_kick);
        world.add_handler(system::entity_spawn);
        world.add_handler(system::entity_move_logic);
        world.add_handler(system::entity_detect_collisions);
        world.add_handler(system::sync_entity_position);
        world.add_handler(system::reset_bounding_boxes);
        world.add_handler(system::update_time);
        world.add_handler(system::update_health);
        world.add_handler(system::sync_players);
        world.add_handler(system::rebuild_player_location);
        world.add_handler(system::player_detect_mob_hits);
        world.add_handler(system::clean_up_io);

        world.add_handler(system::pkt_attack);
        world.add_handler(system::pkt_hand_swing);

        world.add_handler(system::generate_egress_packets);
        world.add_handler(system::egress_broadcast);
        world.add_handler(system::egress_local);
        world.add_handler(system::keep_alive);
        world.add_handler(system::stats_message);
        world.add_handler(system::kill_all);

        let global = world.spawn();
        world.insert(global, Global {
            tick: 0,
            max_hurt_resistant_time: 20, // actually kinda like 10 vanilla mc is weird
            shared: shared.clone(),
        });

        let bounding_boxes = world.spawn();
        world.insert(bounding_boxes, bounding_box::EntityBoundingBoxes::default());

        let uuid_lookup = world.spawn();
        world.insert(uuid_lookup, PlayerUuidLookup::default());

        let player_id_lookup = world.spawn();
        world.insert(player_id_lookup, PlayerIdLookup::default());

        let player_location_lookup = world.spawn();
        world.insert(player_location_lookup, PlayerBoundingBoxes::default());

        let encoder = world.spawn();
        world.insert(encoder, BroadcastBuf::new(shared.compression_level));

        let mut game = Self {
            shared,
            world,
            last_ticks: VecDeque::default(),
            last_ms_per_tick: VecDeque::default(),
            tick_on: 0,
            incoming,
            shutdown_tx,
        };

        game.last_ticks.push_back(Instant::now());

        Ok(game)
    }

    /// The duration to wait between ticks.
    fn wait_duration(&self) -> Option<Duration> {
        let &first_tick = self.last_ticks.front()?;

        let count = self.last_ticks.len();

        #[expect(clippy::cast_precision_loss, reason = "count is limited to 100")]
        let time_for_20_tps = { first_tick + Duration::from_secs_f64(count as f64 / 20.0) };

        // aim for 20 ticks per second
        let now = Instant::now();

        if time_for_20_tps < now {
            warn!("tick took full 50ms; skipping sleep");
            return None;
        }

        let duration = time_for_20_tps - now;
        let duration = duration.mul_f64(0.8);

        if duration.as_millis() > 47 {
            trace!("duration is long");
            return Some(Duration::from_millis(47));
        }

        // this is a bit of a hack to be conservative when sleeping
        Some(duration)
    }

    /// Run the main game loop at 20 ticks per second.
    pub fn game_loop(&mut self) {
        while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
            self.tick();

            if let Some(wait_duration) = self.wait_duration() {
                spin_sleep::sleep(wait_duration);
            }
        }
    }

    /// Run one tick of the game loop.
    #[instrument(skip(self), fields(tick_on = self.tick_on))]
    pub fn tick(&mut self) {
        /// The length of history to keep in the moving average.
        const LAST_TICK_HISTORY_SIZE: usize = 100;

        let now = Instant::now();

        // let mut tps = None;
        if self.last_ticks.len() > LAST_TICK_HISTORY_SIZE {
            let last = self.last_ticks.back().unwrap();

            let ms = last.elapsed().as_nanos() as f64 / 1_000_000.0;
            if ms > 60.0 {
                warn!("tick took too long: {ms}ms");
            }

            self.last_ticks.pop_front().unwrap();
        }

        self.last_ticks.push_back(now);

        while let Ok(connection) = self.incoming.try_recv() {
            let ClientConnection {
                encoder,
                name,
                uuid,
                tx,
            } = connection;

            let player = self.world.spawn();

            let connection = Connection::new(tx);

            let dx = fastrand::f32().mul_add(10.0, -5.0);
            let dz = fastrand::f32().mul_add(10.0, -5.0);

            let event = InitPlayer {
                entity: player,
                encoder,
                connection,
                name,
                uuid,
                pos: FullEntityPose {
                    position: Vec3::new(dx, 30.0, dz),
                    bounding: Aabb::create(Vec3::new(0.0, 2.0, 0.0), 0.6, 1.8),
                    yaw: 0.0,
                    pitch: 0.0,
                },
            };

            self.world.send(event);
        }

        self.world.send(Gametick);
        self.world.send(Egress);

        #[expect(
            clippy::cast_precision_loss,
            reason = "realistically, nanoseconds between last tick will not be greater than 2^52 \
                      (~52 days)"
        )]
        let ms = now.elapsed().as_nanos() as f64 / 1_000_000.0;
        self.update_tick_stats(ms);
        // info!("Tick took: {:02.8}ms", ms);
    }

    #[instrument(skip(self))]
    fn update_tick_stats(&mut self, ms: f64) {
        self.last_ms_per_tick.push_back(ms);

        if self.last_ms_per_tick.len() > MSPT_HISTORY_SIZE {
            // efficient
            let arr = ndarray::Array::from_iter(self.last_ms_per_tick.iter().copied().rev());

            // last 1 second (20 ticks) 5 seconds (100 ticks) and 25 seconds (500 ticks)
            let mean_1_second = arr.slice(s![..20]).mean().unwrap();
            let mean_5_seconds = arr.slice(s![..100]).mean().unwrap();

            debug!("ms / tick: {mean_1_second:.2}ms");

            self.world.send(StatsEvent {
                ms_per_tick_mean_1s: mean_1_second,
                ms_per_tick_mean_5s: mean_5_seconds,
            });

            self.last_ms_per_tick.pop_front();
        }

        self.tick_on += 1;
    }
}

// todo: remove static and make this an `Arc` to prevent weird behavior with multiple `Game`s
/// A shutdown atomic which is used to shut down the [`Game`] gracefully.
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The full pose of an entity. This is used for both [`Player`] and [`MinecraftEntity`].
#[derive(Component, Copy, Clone, Debug)]
pub struct FullEntityPose {
    /// The (x, y, z) position of the entity.
    /// Note we are using [`Vec3`] instead of [`glam::DVec3`] because *cache locality* is important.
    /// However, the Notchian server uses double precision floating point numbers for the position.
    pub position: Vec3,

    /// The yaw of the entity. (todo: probably need a separate component for head yaw, perhaps separate this out)
    pub yaw: f32,

    /// The pitch of the entity.
    pub pitch: f32,

    /// The bounding box of the entity.
    pub bounding: Aabb,
}

impl FullEntityPose {
    /// Move the pose by the given vector.
    pub fn move_by(&mut self, vec: Vec3) {
        self.position += vec;
        self.bounding = self.bounding.move_by(vec);
    }

    /// Teleport the pose to the given position.
    pub fn move_to(&mut self, pos: Vec3) {
        self.bounding = self.bounding.move_to(pos);
        self.position = pos;
    }
}

/// The reaction of an entity, in particular to collisions as calculated in `entity_detect_collisions`.
///
/// Why is this useful?
///
/// - We want to be able to detect collisions in parallel.
/// - Since we are accessing bounding boxes in parallel,
/// we need to be able to make sure the bounding boxes are immutable (unless we have something like a
/// [`Arc`] or [`std::sync::RwLock`], but this is not efficient).
/// - Therefore, we have an [`EntityReaction`] component which is used to store the reaction of an entity to collisions.
/// - Later we can apply the reaction to the entity's [`FullEntityPose`] to move the entity.
#[derive(Component, Default, Debug)]
pub struct EntityReaction {
    /// The velocity of the entity.
    velocity: Vec3,
}
