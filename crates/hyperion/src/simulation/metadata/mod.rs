use std::fmt::Debug;

use derive_more::Deref;
use flecs_ecs::{
    addons::Meta,
    core::{
        ComponentId, Entity, EntityView, IdOperations, QueryBuilderImpl, SystemAPI,
        TermBuilderImpl, World, WorldProvider, flecs,
    },
    macros::{Component, observer},
};
use heck::ToSnakeCase;
use tracing::info_span;
use valence_protocol::{Encode, VarInt};

use crate::{
    Prev,
    simulation::metadata::entity::{EntityFlags, Pose},
};

pub mod block_display;
pub mod display;
pub mod entity;
pub mod living_entity;
pub mod player;

#[derive(Component)]
pub struct MetadataPrefabs {
    pub entity_base: Entity,

    pub display_base: Entity,
    pub block_display_base: Entity,

    pub living_entity_base: Entity,
    pub player_base: Entity,
}

fn component_and_prev<T>(world: &World) -> fn(&mut EntityView<'_>)
where
    T: ComponentId + Copy + PartialEq + Metadata + Default + flecs_ecs::core::DataComponent + Debug,
    <T as ComponentId>::UnderlyingType: Meta<<T as ComponentId>::UnderlyingType>,
{
    world.component::<T>().meta();

    let type_name = core::any::type_name::<T>();

    // convert to snake_case
    let type_name = type_name.to_snake_case();

    let system_name = format!("exchange_{type_name}").leak();

    observer!(world, flecs::OnSet, &T, [filter] &mut MetadataChanges,).each_entity(
        |entity, (value, changes)| {
            println!("setting {value:?}");
            changes.encode(*value);
        },
    );

    // world
    //     .system_named::<(
    //         &mut (Prev, T),       //            (0)
    //         &mut T,               //                  (1)
    //         &mut MetadataChanges, //     (2)
    //     )>(system_name)
    //     .multi_threaded()
    //     .kind::<flecs::pipeline::OnStore>()
    //     .run(move |mut table| {
    //         let span = info_span!("exchange", name = system_name);
    //         let _enter = span.enter();
    //
    //         while table.next() {
    //             unsafe {
    //                 let mut prev = table.field_unchecked::<T>(0);
    //                 let prev = prev.get_mut(..).unwrap();
    //
    //                 let current = table.field_unchecked::<T>(1);
    //                 let current = current.get(..).unwrap();
    //
    //                 let mut metadata_changes = table.field_unchecked::<MetadataChanges>(2);
    //                 let metadata_changes = metadata_changes.get_mut(..).unwrap();
    //
    //                 // todo(perf): big optimization treating as raw bytes and SIMD
    //                 // or code that can easily be compiled to SIMD
    //                 // also can do copy_from_slice in one pass but want SIMD-optimized
    //                 // first
    //                 // todo(learn): reborrowing in-depth
    //                 for (idx, (prev, current)) in itertools::zip_eq(&mut *prev, current).enumerate()
    //                 {
    //                     if prev != current {
    //                         let metadata_changes = metadata_changes.get_unchecked_mut(idx);
    //                         metadata_changes.encode(*current);
    //                     }
    //                 }
    //
    //                 prev.copy_from_slice(current);
    //             }
    //         }
    //     });

    let register = |view: &mut EntityView<'_>| {
        view.set(T::default());
    };

    register
}

trait EntityViewExt {
    fn component_and_prev<T>(self) -> Self
    where
        T: ComponentId
            + Copy
            + PartialEq
            + Metadata
            + Default
            + flecs_ecs::core::DataComponent
            + Debug,
        <T as ComponentId>::UnderlyingType: Meta<<T as ComponentId>::UnderlyingType>;
}

impl EntityViewExt for EntityView<'_> {
    fn component_and_prev<T>(mut self) -> Self
    where
        T: ComponentId
            + Copy
            + PartialEq
            + Metadata
            + Default
            + flecs_ecs::core::DataComponent
            + Debug,
        <T as ComponentId>::UnderlyingType: Meta<<T as ComponentId>::UnderlyingType>,
    {
        let world = self.world();
        // todo: how this possible exclusive mut
        component_and_prev::<T>(&world)(&mut self);
        self
    }
}

#[must_use]
pub fn register_prefabs(world: &World) -> MetadataPrefabs {
    world.component::<MetadataChanges>();

    let entity_base = entity::register_prefab(world, None)
        .add::<MetadataChanges>()
        .component_and_prev::<EntityFlags>()
        .component_and_prev::<Pose>()
        .id();

    let display_base = display::register_prefab(world, Some(entity_base)).id();
    let block_display_base = block_display::register_prefab(world, Some(display_base)).id();

    let living_entity_base = living_entity::register_prefab(world, Some(entity_base)).id();
    let player_base = player::register_prefab(world, Some(living_entity_base))
        // .add::<Player>()
        .id();

    MetadataPrefabs {
        entity_base,
        display_base,
        block_display_base,
        living_entity_base,
        player_base,
    }
}

use crate::simulation::metadata::r#type::MetadataType;

#[derive(Debug, Default, Component, Clone)]
// index (u8), type (varint), value (varies)
/// <https://wiki.vg/Entity_metadata>
///
/// Tracks updates within a gametick for the metadata
pub struct MetadataChanges(Vec<u8>);

unsafe impl Send for MetadataChanges {}

// technically not Sync but I mean do we really care? todo: Indra
unsafe impl Sync for MetadataChanges {}

mod status;

mod r#type;

pub trait Metadata {
    const INDEX: u8;
    type Type: MetadataType + Encode;
    fn to_type(self) -> Self::Type;
}

#[macro_export]
macro_rules! define_metadata_component {
    ($index:literal, $name:ident -> $type:ty) => {
        #[derive(
            Component,
            Copy,
            Clone,
            PartialEq,
            derive_more::Deref,
            derive_more::DerefMut,
            derive_more::Constructor,
            Debug
        )]
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[meta]
        pub struct $name {
            value: $type,
        }

        #[allow(warnings)]
        impl PartialOrd for $name
        where
            $type: PartialOrd,
        {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                self.value.partial_cmp(&other.value)
            }
        }

        impl Metadata for $name {
            type Type = $type;

            const INDEX: u8 = $index;

            fn to_type(self) -> Self::Type {
                self.value
            }
        }
    };
}

#[macro_export]
macro_rules! register_component_ids {
    ($world:expr, $entity:ident, $($name:ident),* $(,)?) => {
        {
            $(
                let reg = $crate::simulation::metadata::component_and_prev::<$name>($world);
                reg(&mut $entity);
            )*

            $entity
        }
    };
}

#[macro_export]
macro_rules! define_and_register_components {
    {
        $(
            $index:literal, $name:ident -> $type:ty
        ),* $(,)?
    } => {
        // Define all components
        $(
            $crate::define_metadata_component!($index, $name -> $type);
        )*

        // Create the registration function
        #[must_use]
        pub fn register_prefab(world: &World, entity_base: Option<Entity>) -> EntityView<'_> {
            // todo: add name
            let mut entity = world.prefab();

            if let Some(entity_base) = entity_base {
                entity = entity.is_a_id(entity_base);
            }

            $crate::register_component_ids!(
                world,
                entity,
                $($name),*
            )
        }
    };
}

// // Air supply component
// #[derive(Component, Default)]
// pub struct AirSupply {
//     pub ticks: i32, // VarInt in original, using i32 for Rust
// }
//
// impl Metadata for AirSupply {
//     type Type = VarInt;
//
//     const INDEX: u8 = 1;
//
//     fn to_type(self) -> Self::Type {
//         VarInt(self.ticks)
//     }
// }

impl MetadataChanges {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn encode<M: Metadata>(&mut self, metadata: M) {
        let value_index = M::INDEX;
        self.0.push(value_index);

        let type_index = VarInt(<M as Metadata>::Type::INDEX);
        type_index.encode(&mut self.0).unwrap();

        let r#type = metadata.to_type();
        r#type.encode(&mut self.0).unwrap();
    }

    pub fn get_and_clear(&mut self) -> Option<MetadataView<'_>> {
        if self.is_empty() {
            return None;
        }
        // denote end of metadata
        self.0.push(0xff);

        Some(MetadataView(self))
    }
}

#[derive(Debug)]
pub struct MetadataView<'a>(&'a mut MetadataChanges);

impl Deref for MetadataView<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0.0[..]
    }
}

impl Drop for MetadataView<'_> {
    fn drop(&mut self) {
        self.0.0.clear();
    }
}
