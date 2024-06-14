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

pub use uuid;

mod blocks;
mod chunk;
pub mod singleton;
pub mod util;

use std::{
    alloc::Allocator,
    cell::RefCell,
    fmt::Debug,
    net::ToSocketAddrs,
    sync::{atomic::AtomicU32, Arc},
    time::Duration,
};

use anyhow::Context;
use bumpalo::Bump;
use derive_more::{Deref, DerefMut, From};
use flecs_ecs::core::World;
use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
use libdeflater::CompressionLvl;
use spin::Lazy;
use thread_local::ThreadLocal;
use tracing::{error, info, instrument, warn};
use valence_protocol::CompressionThreshold;
pub use valence_server;

use crate::{
    component::chunks::Blocks,
    global::Global,
    net::{proxy::init_proxy_comms, Compose, Compressors, IoBuf, MAX_PACKET_SIZE},
    singleton::{fd_lookup::StreamLookup, player_id_lookup::EntityIdLookup},
    system::{chunks::ChunkChanges, player_join_world::generate_biome_registry},
    tasks::Tasks,
};

pub mod component;
// pub mod event;
pub mod event;

pub mod global;
pub mod net;

// pub mod inventory;

mod packets;
mod system;

mod bits;

mod tasks;

// mod tracker;

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

        info!("current soft limit: {}", limits.rlim_cur);
        info!("current hard limit: {}", limits.rlim_max);
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

    info!("setting soft limit to: {}", limits.rlim_cur);

    if unsafe { setrlimit(RLIMIT_NOFILE, &limits) } != 0 {
        error!("Failed to set the file handle limits");
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}

/// The central [`Hyperion`] struct which owns and manages the entire server.
pub struct Hyperion;

pub fn register_components(world: &World) {
    world.component::<component::Pose>();
    world.component::<component::Player>();
    world.component::<component::InGameName>();
    world.component::<component::AiTargetable>();
    world.component::<component::ImmuneStatus>();
    world.component::<component::Uuid>();
    world.component::<component::KeepAlive>();
    world.component::<component::Health>();
    world.component::<component::Vitals>();
    world.component::<component::ChunkPosition>();
    world.component::<ChunkChanges>();
    world.component::<component::DisplaySkin>();
    world.component::<component::EntityReaction>();
}

impl Hyperion {
    pub fn init(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
        Self::init_with(address, |_| {})
    }

    pub fn init_with(
        address: impl ToSocketAddrs + Send + Sync + 'static,
        handlers: impl FnOnce(&World) + Send + Sync + 'static,
    ) -> anyhow::Result<()> {
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
        handlers: impl FnOnce(&World) + Send + Sync + 'static,
    ) -> anyhow::Result<()> {
        // 10k players * 2 file handles / player  = 20,000. We can probably get away with 16,384 file handles
        adjust_file_descriptor_limits(32_768).context("failed to set file limits")?;

        info!("starting hyperion");
        Lazy::force(&config::CONFIG);

        let shared = Arc::new(global::Shared {
            player_count: AtomicU32::new(0),
            compression_threshold: CompressionThreshold(256),
            compression_level: CompressionLvl::new(6)
                .map_err(|_| anyhow::anyhow!("failed to create compression level"))?,
        });

        let world = World::new();
        let world = Box::new(world);
        let world = Box::leak(world);

        register_components(world);

        handlers(world);

        let address = address
            .to_socket_addrs()?
            .next()
            .context("could not get first address")?;

        let tasks = Tasks::default();

        let (receive_state, egress_comm) = init_proxy_comms(&tasks, address);

        let global = Global::new(shared.clone());

        world.set(Compose::new(
            Compressors::new(shared.compression_level),
            Scratches::default(),
            global,
            IoBuf::default(),
        ));

        world.set(EntityIdLookup::default());

        world.set(egress_comm);
        world.set(tasks);

        system::chunks::generate_chunk_changes(world);
        system::chunks::send_updates(world);

        system::stats_message::stats_message(world);
        let biome_registry =
            generate_biome_registry().context("failed to generate biome registry")?;
        world.set(Blocks::new(&biome_registry)?);

        world.set(StreamLookup::default());

        system::ingress::player_connect_disconnect(world, receive_state.0.clone());
        system::ingress::ingress_to_ecs(world, receive_state.0);

        system::ingress::remove_player(world);

        system::ingress::recv_data(world);

        system::sync_entity_position::sync_entity_position(world);

        system::egress::egress(world);

        system::pkt_attack::send_pkt_attack_player(world);
        system::pkt_attack::pkt_attack_entity(world);

        world.set_threads(8);

        loop {
            let tick_each_ms = 50.0;

            let start = std::time::Instant::now();
            world.progress();
            let elapsed = start.elapsed();

            let ms_last_tick = elapsed.as_secs_f32() * 1000.0;

            if ms_last_tick < tick_each_ms {
                let remaining = tick_each_ms - ms_last_tick;
                let remaining = Duration::from_secs_f32(remaining / 1000.0);
                std::thread::sleep(remaining);
            }

            world.get::<&mut Compose>(|compose| {
                compose.global_mut().ms_last_tick = ms_last_tick;
            });
        }
    }
}

// todo: naming? this seems bad
#[derive(Debug)]
pub struct Scratch<A: Allocator = std::alloc::Global> {
    inner: Vec<u8, A>,
}

impl Scratch {
    #[must_use]
    pub fn new() -> Self {
        let inner = Vec::with_capacity(MAX_PACKET_SIZE);
        Self { inner }
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

/// Nice for getting a buffer that can be used for intermediate work
///
/// # Safety
/// - every single time [`ScratchBuffer::obtain`] is called, the buffer will be cleared before returning
/// - the buffer has capacity of at least `MAX_PACKET_SIZE`
pub unsafe trait ScratchBuffer: sealed::Sealed + Debug {
    type Allocator: Allocator;
    fn obtain(&mut self) -> &mut Vec<u8, Self::Allocator>;
}

mod sealed {
    pub trait Sealed {}
}

impl<A: Allocator + Debug> sealed::Sealed for Scratch<A> {}

unsafe impl<A: Allocator + Debug> ScratchBuffer for Scratch<A> {
    type Allocator = A;

    fn obtain(&mut self) -> &mut Vec<u8, Self::Allocator> {
        self.inner.clear();
        &mut self.inner
    }
}

pub type BumpScratch<'a> = Scratch<&'a Bump>;

impl<A: Allocator> From<A> for Scratch<A> {
    fn from(allocator: A) -> Self {
        Self {
            inner: Vec::with_capacity_in(MAX_PACKET_SIZE, allocator),
        }
    }
}

#[derive(Debug, Deref, DerefMut, Default)]
pub struct Scratches {
    inner: ThreadLocal<RefCell<Scratch>>,
}
