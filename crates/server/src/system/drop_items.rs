use std::{borrow::Cow, ops::Add};

use bvh_region::aabb::Aabb;
use evenio::{
    entity::EntityId,
    event::{Despawn, Insert, Receiver, ReceiverMut, Sender, Spawn},
    fetch::{self, Fetcher, Single},
    query::{Query, With},
};
use glam::{dvec3, DVec3, Vec3};
use itertools::Itertools;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::{instrument, warn};
use valence_protocol::{
    packets::play::{
        EntitiesDestroyS2c, EntitySpawnS2c, EntityTrackerUpdateS2c, ItemPickupAnimationS2c,
    },
    ByteAngle, Encode, RawBytes, VarInt, Velocity,
};
use valence_server::{
    ecs::system::In,
    entity::{item::ItemEntityBundle, EntityKind},
    ItemKind, ItemStack,
};

use super::{inventory_systems::send_inventory_update, sync_entity_position::PositionSyncMetadata};
use crate::{
    components::{Display, DroppedItemComponent, FullEntityPose, Player, Uuid},
    event::{DropItem, DropType, GenericBulkCollitionEvent},
    inventory::{self, PlayerInventory},
    net::{Broadcast, Compose, Packets},
    singleton::broadcast,
};

#[derive(Query)]
pub struct DropQuery<'a> {
    // id: EntityId,
    position: &'a mut FullEntityPose,
    inventory: &'a mut PlayerInventory,
    packet: &'a mut Packets,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn drop(
    r: Receiver<DropItem, DropQuery>,
    compose: Compose,
    s: Sender<(
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Uuid>,
        Insert<DroppedItemComponent>,
        Insert<Display>,
        Spawn,
    )>,
    broadcast: Single<&Broadcast>,
) {
    // let event = r.event;
    let query = r.query;

    let id = s.spawn();

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    s.insert(id, FullEntityPose {
        position: query.position.position.add(Vec3::new(2.0, 0.0, 2.0)),
        pitch: query.position.pitch,
        yaw: query.position.yaw,
        bounding: Aabb::create(
            query.position.position.add(Vec3::new(2.0, 0.0, 2.0)),
            0.25,
            0.25,
        ),
    });

    s.insert(id, uuid);
    s.insert(id, PositionSyncMetadata::default());

    let mut stack = query.inventory.get_main_hand().unwrap().clone();

    if let DropType::Single = r.event.drop_type {
        // remove one item from the stack
        stack = stack.with_count(1);

        // remove one item from the inventory
        query.inventory.get_main_hand_mut().unwrap().count -= 1;
    } else {
        // remove all items from the stack
        let item_ref = query.inventory.get_main_hand_mut().unwrap();
        *item_ref = ItemStack::EMPTY;
    }

    s.insert(id, DroppedItemComponent {
        item: stack.clone(),
    });
    s.insert(id, Display(EntityKind::ITEM));
    s.insert(id, uuid);

    let packet2 = EntitySpawnS2c {
        entity_id: VarInt::from(id.index().0 as i32),
        object_uuid: uuid.0,
        kind: EntityKind::ITEM.get().into(),
        position: query
            .position
            .position
            .as_dvec3()
            .add(DVec3::new(2.0, 0.0, 2.0)),
        pitch: ByteAngle::from_degrees(query.position.pitch),
        yaw: ByteAngle::from_degrees(query.position.yaw),
        head_yaw: ByteAngle::from_degrees(0f32),
        data: VarInt::from(0),
        velocity: Velocity([1; 3]),
    };

    // we probably need a vector here because we dont know how big the buffer will be with nbt tags
    let mut buffer = Vec::<u8>::new();
    // index 8
    buffer.push(8u8);
    VarInt::from(7).encode(&mut buffer).unwrap();
    stack.encode(&mut buffer).unwrap();
    // terminator
    buffer.push(0xffu8);

    let packet3 = EntityTrackerUpdateS2c {
        entity_id: VarInt::from(id.index().0 as i32),
        tracked_values: RawBytes::from(buffer.as_slice()),
    };

    broadcast.append(&packet2, &compose).unwrap();
    broadcast.append(&packet3, &compose).unwrap();
}

#[derive(Query, Clone)]
pub struct ItemPickupQuery {
    _item: With<&'static DroppedItemComponent>,
}

#[derive(Query)]
pub struct PlayerPickupQuery<'a> {
    inventory: &'a mut PlayerInventory,
    packets: &'a mut Packets,
    _player: With<&'static Player>,
}

#[instrument(skip_all, level = "trace")]
pub fn pickups(
    mut r: ReceiverMut<GenericBulkCollitionEvent>,
    mut fetcher_player: Fetcher<PlayerPickupQuery>,
    fetcher_item: Fetcher<&DroppedItemComponent>,
    despawner: Sender<Despawn>,
    broadcast: Single<&Broadcast>,
    compose: Compose,
) {
    let mut despawned = Vec::new();
    r.event
        .events
        .get_all_mut()
        .iter()
        .flatten()
        .for_each(
            |event| match fetcher_player.get_mut(event.other_entity_id) {
                Ok(e) => {
                    let item = fetcher_item.get(event.enitiy_id).unwrap().item.clone();
                    let _item = e.inventory.set_first_available(item);

                    let pickup_packet = ItemPickupAnimationS2c {
                        collected_entity_id: VarInt::from(event.enitiy_id.index().0 as i32),
                        collector_entity_id: VarInt::from(event.other_entity_id.index().0 as i32),
                        pickup_item_count: VarInt::from(1),
                    };

                    despawner.despawn(event.enitiy_id);

                    despawned.push(VarInt::from(event.enitiy_id.index().0 as i32));

                    broadcast.append(&pickup_packet, &compose).unwrap();

                    send_inventory_update(e.inventory, e.packets, &compose);
                }
                // ignore if entity is not found or not a player
                Err(_) => (),
            },
        );

    let despawn_packet = EntitiesDestroyS2c {
        entity_ids: Cow::from(despawned),
    };
    broadcast.append(&despawn_packet, &compose).unwrap();
}
