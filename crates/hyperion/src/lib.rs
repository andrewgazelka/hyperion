//! Hyperion

#![feature(type_alias_impl_trait)]
#![feature(io_error_more)]
#![feature(trusted_len)]
#![feature(allocator_api)]
#![feature(read_buf)]
#![feature(core_io_borrowed_buf)]
#![feature(maybe_uninit_slice)]
#![feature(duration_millis_float)]
#![feature(sync_unsafe_cell)]
#![feature(iter_array_chunks)]
#![feature(assert_matches)]
#![feature(try_trait_v2)]
#![feature(let_chains)]
#![feature(ptr_metadata)]
#![allow(
    clippy::redundant_pub_crate,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::future_not_send
)]
#![feature(stmt_expr_attributes)]
#![feature(coroutines)]
use std::{alloc::Allocator, cell::RefCell, fmt::Debug, io::Write, net::ToSocketAddrs, sync::Arc};

use anyhow::{bail, Context};
use derive_more::{Deref, DerefMut};
use egress::EgressModule;
use flecs_ecs::prelude::*;
use ingress::IngressModule;
#[cfg(unix)]
use libc::{getrlimit, setrlimit, RLIMIT_NOFILE};
use libdeflater::CompressionLvl;
use once_cell::sync::Lazy;
use simulation::{
    blocks::MinecraftWorld, util::generate_biome_registry, Comms, SimModule, StreamLookup,
};
use storage::{Db, Events, GlobalEventHandlers, SkinHandler, ThreadLocal};
use tracing::info;
pub use uuid;
pub use valence_protocol;
use valence_protocol::{CompressionThreshold, Encode, Packet};

use crate::{
    net::{proxy::init_proxy_comms, Compose, Compressors, IoBuf, MAX_PACKET_SIZE},
    runtime::AsyncRuntime,
};

mod common;
pub use common::*;

pub mod egress;
pub mod ingress;
pub mod net;
pub mod simulation;
pub mod storage;

pub trait PacketBundle {
    fn encode_including_ids(self, w: impl Write) -> anyhow::Result<()>;
}

// impl<T: Packet + Encode> PacketBundle for T {
//     fn encode_including_ids(self, w: impl Write) -> anyhow::Result<()> {
//         self.encode_with_id(w)
//     }
// }

impl<'a, T: Packet + Encode> PacketBundle for &'a T {
    fn encode_including_ids(self, w: impl Write) -> anyhow::Result<()> {
        self.encode_with_id(w)
    }
}

/// on macOS, the soft limit for the number of open file descriptors is often 256. This is far too low
/// to test 10k players with.
/// This attempts to the specified `recommended_min` value.
#[allow(
    clippy::cognitive_complexity,
    reason = "I have no idea why the cognitive complexity is calcualted as being high"
)]
#[tracing::instrument(skip_all)]
#[cfg(unix)]
pub fn adjust_file_descriptor_limits(recommended_min: u64) -> std::io::Result<()> {
    use tracing::{error, warn};

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

impl Hyperion {
    /// Initializes the server.
    pub fn init(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
        Self::init_with(address, |_| {})
    }

    /// Initializes the server with a custom handler.
    pub fn init_with(
        address: impl ToSocketAddrs + Send + Sync + 'static,
        handlers: impl FnOnce(&World) + Send + Sync + 'static,
    ) -> anyhow::Result<()> {
        // Denormals (numbers very close to 0) are flushed to zero because doing computations on them
        // is slow.
        rayon::ThreadPoolBuilder::new()
            .num_threads(8)
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
        #[cfg(unix)]
        adjust_file_descriptor_limits(32_768).context("failed to set file limits")?;

        info!("starting hyperion");
        Lazy::force(&config::CONFIG);

        let shared = Arc::new(common::Shared {
            compression_threshold: CompressionThreshold(256),
            compression_level: CompressionLvl::new(2)
                .map_err(|_| anyhow::anyhow!("failed to create compression level"))?,
        });

        let world = World::new();
        let world = Box::new(world);
        let world = Box::leak(world);

        let mut app = world.app();

        app.enable_rest(0)
            .enable_stats(true)
            .set_threads(rayon::current_num_threads() as i32)
            .set_target_fps(20.0);

        world.set_threads(rayon::current_num_threads() as i32);

        let address = address
            .to_socket_addrs()?
            .next()
            .context("could not get first address")?;

        world.component::<Db>();
        world.component::<SkinHandler>();
        world.component::<storage::Events>();

        let runtime = AsyncRuntime::default();

        world.set(GlobalEventHandlers::default());

        info!("initializing database");
        let db = Db::new()?;
        let skins = SkinHandler::new(db.clone());
        info!("database initialized");

        world.set(db);
        world.set(skins);

        let (receive_state, egress_comm) = init_proxy_comms(&runtime, address);

        world.set(receive_state);

        let global = Global::new(shared.clone());

        world.set(Compose::new(
            Compressors::new(shared.compression_level),
            Scratches::default(),
            global,
            IoBuf::default(),
        ));

        world.set(Comms::default());

        let events = Events::initialize(world);
        world.set(events);

        world.set(egress_comm);

        let biome_registry =
            generate_biome_registry().context("failed to generate biome registry")?;

        let minecraft_world = MinecraftWorld::new(&biome_registry, runtime.clone())?;

        world.set(minecraft_world);
        world.set(runtime);

        world.set(StreamLookup::default());

        world.import::<SimModule>();
        world.import::<EgressModule>();
        world.import::<IngressModule>();

        handlers(world);

        app.run();

        bail!("app exited");

        // loop {
        //     let tick_each_ms = 50.0;
        //
        //     let start = std::time::Instant::now();
        //
        //     tracing::info_span!("tick").in_scope(|| {

        //         world.progress();
        //     });
        //
        //     let elapsed = start.elapsed();
        //
        //     let ms_last_tick = elapsed.as_secs_f32() * 1000.0;
        //
        //     if ms_last_tick < tick_each_ms {
        //         let remaining = tick_each_ms - ms_last_tick;
        //         let remaining = Duration::from_secs_f32(remaining / 1000.0);
        //         std::thread::sleep(remaining);
        //     }
        //
        //     world.get::<&mut Compose>(|compose| {
        //         compose.global_mut().ms_last_tick = ms_last_tick;
        //     });
        // }
    }
}

/// A scratch buffer for intermediate operations. This will return an empty [`Vec`] when calling [`Scratch::obtain`].
#[derive(Debug)]
pub struct Scratch<A: Allocator = std::alloc::Global> {
    inner: Vec<u8, A>,
}

impl Default for Scratch<std::alloc::Global> {
    fn default() -> Self {
        let inner = Vec::with_capacity(MAX_PACKET_SIZE);
        Self { inner }
    }
}

/// Nice for getting a buffer that can be used for intermediate work
///
/// # Safety
/// - every single time [`ScratchBuffer::obtain`] is called, the buffer will be cleared before returning
/// - the buffer has capacity of at least `MAX_PACKET_SIZE`
pub unsafe trait ScratchBuffer: sealed::Sealed + Debug {
    /// The type of the allocator the [`Vec`] uses.
    type Allocator: Allocator;
    /// Obtains a buffer that can be used for intermediate work.
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

impl<A: Allocator> From<A> for Scratch<A> {
    fn from(allocator: A) -> Self {
        Self {
            inner: Vec::with_capacity_in(MAX_PACKET_SIZE, allocator),
        }
    }
}

/// Thread local scratches
#[derive(Debug, Deref, DerefMut, Default)]
pub struct Scratches {
    inner: ThreadLocal<RefCell<Scratch>>,
}
