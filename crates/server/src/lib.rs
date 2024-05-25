//! Hyperion

#![feature(type_alias_impl_trait)]
#![feature(lint_reasons)]
#![feature(io_error_more)]
#![feature(trusted_len)]
#![feature(allocator_api)]
#![feature(read_buf)]
#![feature(core_io_borrowed_buf)]
#![feature(maybe_uninit_slice)]
#![feature(duration_millis_float)]
#![feature(new_uninit)]
#![feature(sync_unsafe_cell)]
#![feature(iter_array_chunks)]
#![feature(io_slice_advance)]
#![feature(assert_matches)]
#![expect(clippy::type_complexity, reason = "evenio uses a lot of complex types")]

pub use evenio;
pub use uuid;

mod blocks;
mod chunk;
pub mod singleton;
pub mod util;

use std::{
    collections::VecDeque,
    net::ToSocketAddrs,
    sync::{atomic::AtomicU32, Arc},
    time::{Duration, Instant},
};

use anyhow::Context;
use derive_more::From;
use evenio::prelude::*;
use humansize::{SizeFormatter, BINARY};
use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
use libdeflater::CompressionLvl;
use num_format::Locale;
use signal_hook::iterator::Signals;
use singleton::bounding_box;
use spin::Lazy;
use tracing::{error, info, instrument, warn};
use valence_protocol::CompressionThreshold;
pub use valence_server;

use crate::{
    components::{
        chunks::{Chunks, Tasks},
        Vitals,
    },
    event::{Egress, Gametick, Scratches, Stats},
    global::Global,
    net::{buffers::BufferAllocator, Broadcast, Compressors, Server, ServerDef, S2C_BUFFER_SIZE},
    singleton::{
        fd_lookup::FdLookup, player_aabb_lookup::PlayerBoundingBoxes,
        player_id_lookup::EntityIdLookup, player_uuid_lookup::PlayerUuidLookup,
    },
    system::{generate_biome_registry, generate_ingress_events, ItemPickupQuery},
};

pub mod components;
pub mod event;

pub mod global;
pub mod net;

pub mod inventory;

mod packets;
pub mod system;

mod bits;

mod tracker;

mod config;

#[must_use]
pub fn default<T: Default>() -> T {
    T::default()
}

#[derive(From, Debug)]
pub enum CowBytes<'a> {
    Owned(bytes::Bytes),
    Borrowed(&'a [u8]),
}

impl<'a> AsRef<[u8]> for CowBytes<'a> {
    fn as_ref(&self) -> &[u8] {
        match self {
            CowBytes::Owned(bytes) => bytes.as_ref(),
            CowBytes::Borrowed(bytes) => bytes,
        }
    }
}

/// on macOS, the soft limit for the number of open file descriptors is often 256. This is far too low
/// to test 10k players with.
/// This attempts to the specified `recommended_min` value.
#[allow(
    clippy::cognitive_complexity,
    reason = "I have no idea why the cognitive complexity is calcualted as being high"
)]
#[instrument(skip_all)]
pub fn adjust_file_descriptor_limits(recommended_min: u64) -> std::io::Result<()> {
    let mut limits = libc::rlimit {
        rlim_cur: 0, // Initialize soft limit to 0
        rlim_max: 0, // Initialize hard limit to 0
    };

    if unsafe { getrlimit(RLIMIT_NOFILE, &mut limits) } == 0 {
        // Create a stack-allocated buffer...
        let mut rlim_cur = num_format::Buffer::default();
        rlim_cur.write_formatted(&limits.rlim_cur, &Locale::en);

        info!("current soft limit: {rlim_cur}");

        let mut rlim_max = num_format::Buffer::default();
        rlim_max.write_formatted(&limits.rlim_max, &Locale::en);

        info!("current hard limit: {rlim_max}");
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
    let mut new_limit = num_format::Buffer::default();
    new_limit.write_formatted(&limits.rlim_cur, &Locale::en);

    info!("setting soft limit to: {new_limit}");

    if unsafe { setrlimit(RLIMIT_NOFILE, &limits) } != 0 {
        error!("Failed to set the file handle limits");
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

#[instrument(skip_all)]
fn set_memlock_limit(limit: u64) -> anyhow::Result<()> {
    let mut rlim_current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };

    let result = unsafe { getrlimit(libc::RLIMIT_MEMLOCK, &mut rlim_current) };

    if result != 0 {
        return Err(std::io::Error::last_os_error()).context("failed to get memlock limit");
    }

    if result == 0 {
        let rlim_cur = SizeFormatter::new(rlim_current.rlim_cur, BINARY);
        let rlim_max = SizeFormatter::new(rlim_current.rlim_max, BINARY);
        info!("current soft limit: {rlim_cur}");
        info!("current hard limit: {rlim_max}");
        //
    }

    if limit < rlim_current.rlim_cur {
        info!("current limit is already greater than requested limit");
        return Ok(());
    }

    let rlim = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };

    let result = unsafe { setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if result == 0 {
        let limit_fmt = SizeFormatter::new(limit, BINARY);
        info!("set limit to {limit_fmt}");

        let count = rayon_local::count();
        let expected = limit * count as u64;
        let expected_fmt = SizeFormatter::new(expected, BINARY);
        info!("expected to allocate {count}×{limit_fmt} = {expected_fmt} bytes of memory");

        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("failed to set memlock limit")
    }
}

/// The central [`Hyperion`] struct which owns and manages the entire server.
pub struct Hyperion {
    /// The shared state between the ECS framework and the I/O thread.
    shared: Arc<global::Shared>,
    /// The manager of the ECS framework.
    world: World,
    /// Data for what time the last ticks occurred.
    last_ticks: VecDeque<Instant>,
    /// The tick of the game. This is incremented every 50 ms.
    tick_on: u64,

    server: Server,
}

//

impl Hyperion {
    /// Get the [`World`] which is the core part of the ECS framework.
    pub const fn world(&self) -> &World {
        &self.world
    }

    /// Get all shared data that is shared between the ECS framework and the IO thread.
    pub const fn shared(&self) -> &Arc<global::Shared> {
        &self.shared
    }

    /// See [`Hyperion::world`].
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// # Panics
    /// This function will panic if the game is already shutdown.
    pub const fn shutdown(&self) {
        // TODO
    }

    pub fn init(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<Self> {
        Self::init_with(address, |_| {})
    }

    pub fn init_with(
        address: impl ToSocketAddrs + Send + Sync + 'static,
        handlers: impl FnOnce(&mut World) + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        // Denormals (numbers very close to 0) are flushed to zero because doing computations on them
        // is slow.
        rayon::ThreadPoolBuilder::new()
            .spawn_handler(|thread| {
                std::thread::spawn(|| {
                    no_denormals::no_denormals(|| {
                        thread.run();
                    });
                });
                Ok(())
            })
            .build_global()
            .context("failed to build thread pool")?;

        no_denormals::no_denormals(|| Self::init_with_helper(address, handlers))
    }

    /// Initialize the server.
    #[allow(clippy::too_many_lines, reason = "todo")]
    fn init_with_helper(
        address: impl ToSocketAddrs + Send + Sync + 'static,
        handlers: impl FnOnce(&mut World) + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        // 10k players * 2 file handles / player  = 20,000. We can probably get away with 16,384 file handles
        adjust_file_descriptor_limits(32_768).context("failed to set file limits")?;

        // we want at least S2C_BUFFER_SIZE memlock limit
        set_memlock_limit(S2C_BUFFER_SIZE as u64).context("failed to set memlock limit.")?;

        info!("starting hyperion");
        Lazy::force(&config::CONFIG);

        let current_threads = rayon::current_num_threads();
        let max_threads = rayon::max_num_threads();

        info!("rayon: current threads: {current_threads}, max threads: {max_threads}");

        let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
            .context("failed to create signal handler")?;

        std::thread::spawn({
            move || {
                for _ in signals.forever() {
                    warn!("Shutting down...");
                    SHUTDOWN.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });

        let shared = Arc::new(global::Shared {
            player_count: AtomicU32::new(0),
            compression_threshold: CompressionThreshold(256),
            compression_level: CompressionLvl::new(6)
                .map_err(|_| anyhow::anyhow!("failed to create compression level"))?,
        });

        let mut world = World::new();

        handlers(&mut world);

        let compressor_id = world.spawn();
        world.insert(compressor_id, Compressors::new(shared.compression_level));

        let address = address
            .to_socket_addrs()?
            .next()
            .context("could not get first address")?;

        let mut server_def = Server::new(address)?;

        let buffers_id = world.spawn();
        let mut buffers_elem = BufferAllocator::new(&mut server_def);

        let broadcast = world.spawn();
        world.insert(broadcast, Broadcast::new(&mut buffers_elem)?);

        world.insert(buffers_id, buffers_elem);

        world.add_handler(system::ingress::add_player);
        world.add_handler(system::ingress::remove_player);
        world.add_handler(system::ingress::recv_data);
        world.add_handler(system::ingress::sent_data);

        world.add_handler(system::chunks::generate_chunk_changes);
        world.add_handler(system::chunks::send_updates);

        world.add_handler(system::init_player);
        world.add_handler(system::despawn_player);
        world.add_handler(system::player_join_world);

        world.add_handler(system::send_player_info);
        world.add_handler(system::player_kick);
        world.add_handler(system::init_entity);
        world.add_handler(system::entity_move_logic);
        world.add_handler(system::entity_detect_collisions);
        world.add_handler(system::sync_entity_position);
        world.add_handler(system::recalculate_bounding_boxes);
        world.add_handler(system::update_time);
        world.add_handler(system::send_time);
        world.add_handler(system::update_health);
        world.add_handler(system::sync_players);
        world.add_handler(system::rebuild_player_location);
        //   world.add_handler(system::player_detect_mob_hits);

        world.add_handler(system::generic_collision::<ItemPickupQuery>);
        world.add_handler(system::pickups);

        world.add_handler(system::check_immunity);
        world.add_handler(system::pkt_attack_player);
        world.add_handler(system::pkt_attack_entity);
        world.add_handler(system::set_player_skin);
        world.add_handler(system::compass);

        world.add_handler(system::block_update);
        world.add_handler(system::chat_message);
        world.add_handler(system::disguise_player);
        world.add_handler(system::teleport);
        world.add_handler(system::shoved_reaction);
        world.add_handler(system::pose_update);

        world.add_handler(system::effect::display);
        world.add_handler(system::effect::speed);

        world.add_handler(system::pkt_hand_swing);

        world.add_handler(system::generate_egress_packets);

        world.add_handler(system::egress);

        world.add_handler(system::keep_alive);
        world.add_handler(system::stats_message);
        world.add_handler(system::kill_all);

        world.add_handler(system::get_inventory_actions);
        world.add_handler(system::update_main_hand);
        world.add_handler(system::update_equipment);
        world.add_handler(system::give_command);
        world.add_handler(system::drop);

        let global = world.spawn();
        world.insert(global, Global::new(shared.clone()));

        let scratches = world.spawn();
        world.insert(scratches, Scratches::default());

        let bounding_boxes = world.spawn();
        world.insert(bounding_boxes, bounding_box::EntityBoundingBoxes::default());

        let uuid_lookup = world.spawn();
        world.insert(uuid_lookup, PlayerUuidLookup::default());

        let chunks = world.spawn();
        let biome_registry =
            generate_biome_registry().context("failed to generate biome registry")?;
        world.insert(chunks, Chunks::new(&biome_registry)?);

        let tasks = world.spawn();
        world.insert(tasks, Tasks::default());

        let player_id_lookup = world.spawn();
        world.insert(player_id_lookup, EntityIdLookup::default());

        let player_location_lookup = world.spawn();
        world.insert(player_location_lookup, PlayerBoundingBoxes::default());

        let fd_lookup = world.spawn();
        world.insert(fd_lookup, FdLookup::default());

        let mut game = Self {
            shared,
            world,
            last_ticks: VecDeque::default(),
            tick_on: 0,
            server: server_def,
        };

        game.last_ticks.push_back(Instant::now());
        Ok(game)
    }

    /// The duration to wait between ticks.
    #[instrument(skip_all, level = "trace")]
    fn calculate_wait_duration(&self) -> Option<Duration> {
        let &first_tick = self.last_ticks.front()?;

        let count = self.last_ticks.len();

        #[expect(clippy::cast_precision_loss, reason = "count is limited to 100")]
        let time_for_20_tps = { first_tick + Duration::from_secs_f64(count as f64 / 20.0) };

        // aim for 20 ticks per second
        let now = Instant::now();

        if time_for_20_tps < now {
            let off_by = now - time_for_20_tps;
            let off_by = off_by.as_millis_f64();
            warn!("off by {off_by:.2}ms → skipping sleep");
            return None;
        }

        let duration = time_for_20_tps - now;
        let duration = duration.mul_f64(0.8);

        if duration.as_millis() > 47 {
            return Some(Duration::from_millis(47));
        }

        // this is a bit of a hack to be conservative when sleeping
        Some(duration)
    }

    /// Run the main game loop at 20 ticks per second.
    pub fn game_loop(&mut self) {
        while !SHUTDOWN.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(wait_duration) = self.tick() {
                spin_sleep::sleep(wait_duration);
            }
        }
    }

    /// Run one tick of the game loop.
    #[instrument(skip(self), fields(on = self.tick_on))]
    pub fn tick(&mut self) -> Option<Duration> {
        /// The length of history to keep in the moving average.
        const LAST_TICK_HISTORY_SIZE: usize = 100;

        let now = Instant::now();

        // let mut tps = None;
        if self.last_ticks.len() > LAST_TICK_HISTORY_SIZE {
            let last = self.last_ticks.back().unwrap();

            let ms = last.elapsed().as_nanos() as f64 / 1_000_000.0;
            if ms > 60.0 {
                warn!("took too long: {ms:.2}ms");
            }

            self.last_ticks.pop_front().unwrap();
        }

        self.last_ticks.push_back(now);

        generate_ingress_events(&mut self.world, &mut self.server);

        tracing::span!(tracing::Level::TRACE, "gametick").in_scope(|| {
            self.world.send(Gametick);
        });

        let server = &mut self.server;

        tracing::span!(tracing::Level::TRACE, "egress-event").in_scope(|| {
            self.world.send(Egress { server });
        });

        #[expect(
            clippy::cast_precision_loss,
            reason = "realistically, nanoseconds between last tick will not be greater than 2^52 \
                      (~52 days)"
        )]
        let ms = now.elapsed().as_nanos() as f64 / 1_000_000.0;
        self.update_tick_stats(ms);
        self.calculate_wait_duration()
    }

    #[instrument(skip_all, level = "trace")]
    fn update_tick_stats(&mut self, ms: f64) {
        self.world.send(Stats { ms_per_tick: ms });

        self.tick_on += 1;
    }
}

// todo: remove static and make this an `Arc` to prevent weird behavior with multiple `Game`s
/// A shutdown atomic which is used to shut down the [`Hyperion`] gracefully.
static SHUTDOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
