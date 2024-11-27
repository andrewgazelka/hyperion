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
#![feature(stmt_expr_attributes)]
#![feature(coroutines)]
#![feature(array_try_map)]
#![feature(split_array)]
#![feature(never_type)]
#![feature(duration_constructors)]
#![feature(array_chunks)]
#![feature(portable_simd)]

pub const NUM_THREADS: usize = 8;
pub const CHUNK_HEIGHT_SPAN: u32 = 384; // 512; // usually 384

use std::{
    alloc::Allocator,
    cell::RefCell,
    fmt::Debug,
    io::Write,
    net::ToSocketAddrs,
    sync::{Arc, atomic::AtomicBool},
};

use anyhow::{Context, bail};
use derive_more::{Deref, DerefMut};
use egress::EgressModule;
use flecs_ecs::prelude::*;
pub use glam;
use glam::IVec2;
use ingress::IngressModule;
#[cfg(unix)]
use libc::{RLIMIT_NOFILE, getrlimit, setrlimit};
use libdeflater::CompressionLvl;
use simulation::{Comms, SimModule, StreamLookup, blocks::Blocks};
use storage::{Events, GlobalEventHandlers, LocalDb, SkinHandler, ThreadLocal};
use tracing::{info, info_span, warn};
use util::mojang::MojangClient;
pub use uuid;
// todo: slowly move more and more things to arbitrary module
// and then eventually do not re-export valence_protocol
pub use valence_protocol;
use valence_protocol::{CompressionThreshold, Encode, Packet};
pub use valence_protocol::{
    ItemKind, ItemStack, Particle,
    block::{BlockKind, BlockState},
};

use crate::{
    net::{Compose, Compressors, IoBuf, MAX_PACKET_SIZE, proxy::init_proxy_comms},
    runtime::AsyncRuntime,
    simulation::{Pitch, Yaw},
};

mod common;
pub use common::*;
use hyperion_crafting::CraftingRegistry;
pub use valence_ident;

use crate::{
    ingress::PendingRemove,
    net::{NetworkStreamRef, PacketDecoder, proxy::ReceiveState},
    runtime::Tasks,
    simulation::{
        EgressComm, EntitySize, IgnMap, PacketState, Player,
        metadata::{EntityFlags, Pose},
    },
    util::mojang::ApiProvider,
};

pub mod egress;
pub mod ingress;
pub mod net;
pub mod simulation;
pub mod storage;

/// Tracks previous values
#[derive(Component, Deref, DerefMut, Clone, Copy)]
pub struct Prev<T: ComponentId>(pub T);

pub trait PacketBundle {
    fn encode_including_ids(self, w: impl Write) -> anyhow::Result<()>;
}

impl<T: Packet + Encode> PacketBundle for &T {
    fn encode_including_ids(self, w: impl Write) -> anyhow::Result<()> {
        self.encode_with_id(w)
    }
}

/// on macOS, the soft limit for the number of open file descriptors is often 256. This is far too low
/// to test 10k players with.
/// This attempts to the specified `recommended_min` value.
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

#[derive(Component)]
struct Shutdown {
    value: Arc<AtomicBool>,
}

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
            .num_threads(NUM_THREADS)
            .spawn_handler(|thread| {
                std::thread::Builder::new()
                    .stack_size(1024 * 1024)
                    .spawn(move || {
                        no_denormals::no_denormals(|| {
                            thread.run();
                        });
                    })
                    .expect("Failed to spawn thread");
                Ok(())
            })
            .build_global()
            .context("failed to build thread pool")?;

        no_denormals::no_denormals(|| Self::init_with_helper(address, handlers))
    }

    /// Initialize the server.
    fn init_with_helper(
        address: impl ToSocketAddrs + Send + Sync + 'static,
        handlers: impl FnOnce(&World) + Send + Sync + 'static,
    ) -> anyhow::Result<()> {
        // 10k players * 2 file handles / player  = 20,000. We can probably get away with 16,384 file handles
        #[cfg(unix)]
        adjust_file_descriptor_limits(32_768).context("failed to set file limits")?;

        let shared = Arc::new(Shared {
            compression_threshold: CompressionThreshold(256),
            compression_level: CompressionLvl::new(2)
                .map_err(|_| anyhow::anyhow!("failed to create compression level"))?,
        });

        let world = World::new();

        let world = Box::new(world);
        let world: &World = Box::leak(world);

        let mut app = world.app();

        app.enable_rest(0)
            .enable_stats(true)
            .set_threads(i32::try_from(rayon::current_num_threads())?)
            .set_target_fps(20.0);

        world.set_threads(i32::try_from(rayon::current_num_threads())?);

        let address = address
            .to_socket_addrs()?
            .next()
            .context("could not get first address")?;

        world.component::<Shutdown>();
        let shutdown = Arc::new(AtomicBool::new(false));

        world.set(Shutdown {
            value: shutdown.clone(),
        });

        world.component::<Pose>();
        world.component::<Prev<Pose>>();

        world.component::<Prev<EntityFlags>>();

        world.component::<EntityFlags>();
        // todo: sadly this requires u32
        // .bit("on_fire", *EntityFlags::ON_FIRE)
        // .bit("crouching", *EntityFlags::CROUCHING)
        // .bit("sprinting", *EntityFlags::SPRINTING)
        // .bit("swimming", *EntityFlags::SWIMMING)
        // .bit("invisible", *EntityFlags::INVISIBLE)
        // .bit("glowing", *EntityFlags::GLOWING)
        // .bit("flying_with_elytra", *EntityFlags::FLYING_WITH_ELYTRA);

        component!(world, IVec2 { x: i32, y: i32 });
        world.component::<PendingRemove>();

        world.component::<Yaw>();
        component!(world, Yaw).opaque_func(meta_ser_stringify_type_display::<Yaw>);

        world.component::<Pitch>();
        component!(world, Pitch).opaque_func(meta_ser_stringify_type_display::<Pitch>);

        world.component::<PacketDecoder>();

        world.component::<PacketState>();

        world.component::<NetworkStreamRef>();
        world.component::<ReceiveState>();
        world.component::<Compose>();
        world.component::<CraftingRegistry>();

        world.component::<LocalDb>();
        world.component::<SkinHandler>();
        world.component::<MojangClient>();
        world.component::<Events>();
        world.component::<Comms>();
        world.component::<EgressComm>();
        world.component::<Blocks>();

        world.component::<AsyncRuntime>();
        world.component::<Tasks>();

        system!("run_tasks", world, &mut Tasks($))
            .with::<flecs::pipeline::OnUpdate>()
            .each_iter(|it, _, tasks| {
                let world = it.world();
                let span = info_span!("run_tasks");
                let _enter = span.enter();
                while let Ok(Some(task)) = tasks.tasks.try_recv() {
                    task(&world);
                }
            });

        world.component::<StreamLookup>();
        world.component::<EntitySize>();
        world.component::<IgnMap>();

        world.component::<config::Config>();

        info!("starting hyperion");
        let config = config::Config::load("run/config.toml")?;
        world.set(config);

        let (task_tx, task_rx) = kanal::bounded(32);
        let runtime = AsyncRuntime::new(task_tx);

        #[cfg(unix)]
        #[allow(clippy::redundant_pub_crate)]
        runtime.spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            let mut sigquit =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit()).unwrap();

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    warn!("SIGINT/ctrl-c received, shutting down");
                    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ = sigterm.recv() => {
                    warn!("SIGTERM received, shutting down");
                    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                _ = sigquit.recv() => {
                    warn!("SIGQUIT received, shutting down");
                    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        });

        let tasks = Tasks { tasks: task_rx };
        world.set(tasks);

        world.component::<GlobalEventHandlers>();
        world.set(GlobalEventHandlers::default());

        info!("initializing database");
        let db = LocalDb::new()?;
        let skins = SkinHandler::new(&db)?;
        info!("database initialized");

        world.set(db);
        world.set(skins);

        world.set(MojangClient::new(&runtime, ApiProvider::MAT_DOES_DEV));

        let (receive_state, egress_comm) = init_proxy_comms(&runtime, address);

        world.set(receive_state);

        let global = Global::new(shared.clone());

        world.set(Compose::new(
            Compressors::new(shared.compression_level),
            Scratches::default(),
            global,
            IoBuf::default(),
        ));

        world.set(CraftingRegistry::default());

        world.set(Comms::default());

        let events = Events::initialize(world);
        world.set(events);

        world.set(egress_comm);

        world.set(runtime);
        world.set(StreamLookup::default());

        world.import::<SimModule>();
        world.import::<EgressModule>();
        world.import::<IngressModule>();

        world
            .component::<Player>()
            .add_trait::<(flecs::With, EntitySize)>()
            .add_trait::<(flecs::With, Yaw)>()
            .add_trait::<(flecs::With, Pitch)>();

        world.set(IgnMap::default());

        handlers(world);

        app.run();

        bail!("app exited");
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
