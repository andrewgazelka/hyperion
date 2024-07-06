use std::{
    cell::{Cell, SyncUnsafeCell},
    fmt::Debug,
    mem::MaybeUninit,
};

use bytes::Bytes;
use derive_more::{Deref, DerefMut};
use flecs_ecs::{core::World, macros::Component};
use glam::I16Vec2;
use tracing::trace;
use valence_generated::block::BlockState;
use valence_protocol::{packets::play, BlockPos};
use valence_server::layer::chunk::{Chunk, UnloadedChunk};

use crate::{
    component::blocks::{Block, MinecraftWorld},
    net::Compose,
    thread_local::ThreadLocal,
    SystemId,
};

pub const START_Y: i32 = -64;

// 384 / 16 = 24
// const CHUNK_HEIGHT: usize = 24;

// todo: bench packed vs non-packed cause we can pack xy into u8
// to get size_of::<Delta>() == 5
#[derive(Copy, Clone, Debug)]
pub struct Delta {
    x: u8,                   // 1
    z: u8,                   // 1
    y: u16,                  // 2
    block_state: BlockState, // 2
}

impl Delta {
    #[must_use]
    pub fn new(x: u8, y: u16, z: u8, block_state: BlockState) -> Self {
        debug_assert!(x <= 15);
        debug_assert!(z <= 15);
        debug_assert!(y <= 384);

        Self {
            x,
            z,
            y,
            block_state,
        }
    }
}

const _: () = assert!(size_of::<Delta>() == 6);

#[repr(packed)]
#[derive(Copy, Clone)]
pub struct OnChange {
    xz: u8, // 1
    y: u16, // 2
}

impl Debug for OnChange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnChange")
            .field("x", &self.x())
            .field("z", &self.z())
            .field("y", &self.y())
            .finish()
    }
}

impl OnChange {
    #[must_use]
    pub const fn new(x: u8, y: u16, z: u8) -> Self {
        Self {
            xz: x << 4 | (z & 0b1111),
            y,
        }
    }

    #[must_use]
    pub const fn x(&self) -> u8 {
        self.xz >> 4
    }

    #[must_use]
    pub const fn z(&self) -> u8 {
        self.xz & 0b1111
    }

    #[must_use]
    pub const fn y(&self) -> u16 {
        self.y
    }
}

const _: () = assert!(size_of::<OnChange>() == 3);

#[derive(Debug)]
pub struct ThreadHeaplessVec<T, const N: usize = 32> {
    inner: ThreadLocal<SyncUnsafeCell<heapless::Vec<T, N>>>,
}

#[derive(Debug, Deref, DerefMut)]
pub struct ThreadLocalVec<T> {
    inner: ThreadLocal<SyncUnsafeCell<Vec<T>>>,
}

pub struct ThreadLocalCustomVec<T> {
    lens: ThreadLocal<Cell<u16>>,
    inner: ThreadLocal<SyncUnsafeCell<Box<[MaybeUninit<T>]>>>,
}

impl<T> ThreadLocalCustomVec<T> {
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            lens: ThreadLocal::default(),
            inner: ThreadLocal::new_with(|_| SyncUnsafeCell::new(Box::new_uninit_slice(n))),
        }
    }

    pub fn push(&self, elem: T, world: &World) {
        let lens = self.lens.get(world);
        let idx = lens.get();
        lens.set(idx + 1);

        let inner = self.inner.get(world);
        let inner = unsafe { &mut *inner.get() };
        inner[idx as usize].write(elem);
    }

    pub fn is_empty(&mut self) -> bool {
        self.lens.iter_mut().all(|x| x.get() == 0)
    }
}

impl<T, const N: usize> Default for ThreadHeaplessVec<T, N> {
    fn default() -> Self {
        Self {
            inner: ThreadLocal::new_defaults(),
        }
    }
}

impl<T> Default for ThreadLocalVec<T> {
    fn default() -> Self {
        Self {
            inner: ThreadLocal::new_defaults(),
        }
    }
}

#[derive(Debug, Default, Deref, DerefMut, Component)]
pub struct PendingChanges(ThreadLocalVec<Delta>);

#[derive(Debug, Default, Deref, DerefMut, Component)]
pub struct NeighborNotify(ThreadLocalVec<OnChange>);

impl<T: Debug, const N: usize> ThreadHeaplessVec<T, N> {
    pub fn push(&self, element: T, world: &World) {
        let inner = self.inner.get(world);
        let inner = unsafe { &mut *inner.get() };
        assert!(inner.push(element).is_ok(), "ThreadList {inner:?} is full");
    }
}

impl<T> ThreadLocalVec<T> {
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            inner: ThreadLocal::new_with(|_| SyncUnsafeCell::new(Vec::with_capacity(n))),
        }
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.inner
            .iter_mut()
            .map(SyncUnsafeCell::get_mut)
            .flat_map(|x| x.drain(..))
    }

    pub fn is_empty(&mut self) -> bool {
        self.inner
            .iter_mut()
            .map(SyncUnsafeCell::get_mut)
            .all(|x| x.is_empty())
    }
}

impl<T: Debug> ThreadLocalVec<T> {
    pub fn push(&self, element: T, world: &World) {
        let inner = self.inner.get(world);
        let inner = unsafe { &mut *inner.get() };
        inner.push(element);
    }
}

struct Drain<'a, T, const N: usize> {
    inner: &'a mut heapless::Vec<T, N>,
    idx: usize,
}

impl<'a, T, const N: usize> Iterator for Drain<'a, T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.inner.len() {
            return None;
        }

        let item = self.inner.get(self.idx).unwrap();
        let item = unsafe { core::ptr::read(item) };

        self.idx += 1;

        Some(item)
    }
}

impl<'a, T, const N: usize> Drop for Drain<'a, T, N> {
    fn drop(&mut self) {
        unsafe { self.inner.set_len(0) };
    }
}

impl<'a, T, const N: usize> Drain<'a, T, N> {
    pub fn new(inner: &'a mut heapless::Vec<T, N>) -> Self {
        Self { inner, idx: 0 }
    }
}

impl<T> ThreadHeaplessVec<T> {
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.inner
            .iter_mut()
            .map(SyncUnsafeCell::get_mut)
            .flat_map(|inner| Drain::new(inner))
    }

    pub fn is_empty(&mut self) -> bool {
        self.inner
            .iter_mut()
            .map(SyncUnsafeCell::get_mut)
            .all(|x| x.is_empty())
    }
}

/// A chunk which has been loaded into memory.
#[derive(Debug, Component)]
pub struct LoadedChunk {
    /// The raw (usually compressed) bytes of the chunk that are sent to the client via the Minecraft protocol.
    pub base_packet_bytes: Bytes,

    /// The actual chunk data that is "uncompressed". It uses a palette to store the actual data. This is usually used
    /// for obtaining the actual data from the chunk such as getting the block state of a block at a given position.
    pub chunk: UnloadedChunk,

    pub position: I16Vec2,
}

impl LoadedChunk {
    pub const fn new(base_packet_bytes: Bytes, chunk: UnloadedChunk, position: I16Vec2) -> Self {
        Self {
            base_packet_bytes,
            chunk,
            position,
        }
    }

    pub fn bytes(&self) -> Bytes {
        self.base_packet_bytes.clone()
    }

    #[must_use]
    pub const fn chunk(&self) -> &UnloadedChunk {
        &self.chunk
    }

    pub fn chunk_mut(&mut self) -> &mut UnloadedChunk {
        &mut self.chunk
    }

    fn set_block_internal(&mut self, x: u8, y: u16, z: u8, state: BlockState) {
        self.chunk
            .set_block(u32::from(x), u32::from(y), u32::from(z), state);
    }

    fn get_block_internal(&self, x: u8, y: u16, z: u8) -> u16 {
        self.chunk
            .block_state(u32::from(x), u32::from(y), u32::from(z))
            .to_raw()
    }

    fn get_block(&self, x: u8, y: u16, z: u8) -> BlockState {
        BlockState::from_raw(self.get_block_internal(x, y, z)).unwrap()
    }

    pub fn process_neighbor_changes(
        &self,
        pending: &mut NeighborNotify,
        mc: &MinecraftWorld,
        world: &World,
    ) {
        let position = self.position;

        let start_x = i32::from(position.x) << 4;
        let start_z = i32::from(position.y) << 4;

        for change in pending.drain() {
            let x = change.x();
            let z = change.z();
            let y = change.y();

            let state = self.get_block(x, y, z);

            let x = i32::from(x) + start_x;
            let z = i32::from(z) + start_z;
            let y = i32::from(y) + START_Y;

            let block_pos = BlockPos::new(x, y, z);

            let block = Block::from(state);

            (block.on_neighbor_block_change)(mc, block_pos, state, world);
        }
    }

    pub fn interact(&self, x: u8, y: u16, z: u8, mc: &MinecraftWorld, world: &World) {
        let position = self.position;

        let state = self.get_block(x, y, z);

        let start_x = i32::from(position.x) << 4;
        let start_z = i32::from(position.y) << 4;

        let x = i32::from(x) + start_x;
        let z = i32::from(z) + start_z;
        let y = i32::from(y) + START_Y;

        let block_pos = BlockPos::new(x, y, z);

        let block = Block::from(state);

        (block.on_block_interact)(mc, block_pos, state, world);
    }

    pub fn process_pending_changes(
        &mut self,
        current_deltas: &mut PendingChanges,
        compose: &Compose,
        notify: &NeighborNotify,
        mc: &MinecraftWorld,
        system_id: SystemId,
        world: &World,
    ) {
        const MAX_Y: u16 = 384;

        let position = self.position;

        for Delta {
            x,
            y,
            z,
            block_state,
        } in current_deltas.drain()
        {
            self.set_block_internal(x, y, z, block_state);

            trace!("set block at {x} {y} {z} to {block_state}");

            let start_x = i32::from(position.x) << 4;
            let start_z = i32::from(position.y) << 4;

            let block_pos = BlockPos::new(
                start_x + i32::from(x),
                START_Y + i32::from(y),
                start_z + i32::from(z),
            );

            let pkt = play::BlockUpdateS2c {
                position: block_pos,
                block_id: block_state,
            };

            compose.broadcast(&pkt, system_id).send(world).unwrap();

            // notify neighbors
            if x == 0 {
                let chunk_position = position - I16Vec2::new(1, 0);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(15, y, z), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x - 1, y, z), world);
            }

            if x == 15 {
                let chunk_position = position + I16Vec2::new(1, 0);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(0, y, z), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x + 1, y, z), world);
            }

            if y != 0 {
                notify.push(OnChange::new(x, y - 1, z), world);
            }

            if y != MAX_Y {
                // todo: is this one off?
                notify.push(OnChange::new(x, y + 1, z), world);
            }

            if z == 0 {
                let chunk_position = position - I16Vec2::new(0, 1);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(x, y, 15), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x, y, z - 1), world);
            }

            if z == 15 {
                let chunk_position = position + I16Vec2::new(0, 1);
                if let Some(entity) = mc.get_loaded_chunk_entity(chunk_position, world) {
                    entity.get::<&NeighborNotify>(|notify| {
                        notify.push(OnChange::new(x, y, 0), world);
                    });
                };
            } else {
                notify.push(OnChange::new(x, y, z + 1), world);
            }
        }
    }
}
