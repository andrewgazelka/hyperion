use std::{iter, mem};

use bvh_region::aabb::Aabb;
use evenio::prelude::*;
use glam::Vec3;
use tracing::{error, instrument};
use valence_protocol::{
    item::ItemStack, packets::play, ByteAngle, Hand, ItemKind, VarInt, Velocity,
};
use valence_server::entity::EntityKind;

use crate::{
    components::{Arrow, EntityPhysics, EntityPhysicsState, FullEntityPose, Uuid},
    event::{ReleaseItem, UpdateInventory},
    inventory::PlayerInventory,
    net::Compose,
    system::sync_entity_position::PositionSyncMetadata,
};

#[derive(Query)]
pub struct ReleaseItemQuery<'a> {
    id: EntityId,
    pose: &'a FullEntityPose,
    inventory: &'a mut PlayerInventory,
}

#[instrument(skip_all, level = "trace")]
pub fn release_item(
    r: Receiver<ReleaseItem, ReleaseItemQuery>,
    compose: Compose,
    s: Sender<(
        Insert<Arrow>,
        Insert<EntityPhysics>,
        Insert<FullEntityPose>,
        Insert<PositionSyncMetadata>,
        Insert<Uuid>,
        Spawn,
        UpdateInventory,
    )>,
) {
    let query = r.query;
    let inventory = query.inventory;

    let Some(interaction) = mem::take(&mut inventory.interaction) else {
        error!("client attempted to release item without using one first");
        return;
    };

    // Check that the player is holding a bow
    if inventory.get_hand(interaction.hand).item != ItemKind::Bow {
        return;
    }

    // The tick when the bow is released doesn't count, so one tick is subtracted from the
    // duration.
    let duration = interaction.start.elapsed().unwrap().as_secs_f32() - 0.05;

    if duration < 0.140_175 {
        // The arrow was not shot. Note that the bow draw and release packets are not tied to a specific
        // tick when sent over the network, so the client may believe that the bow was drawn for
        // enough time to shoot an arrow and remove an arrow from the inventory on the client side
        // while the server believes that the bow wasn't drawn for enough time. To avoid desync, an
        // update inventory packet is sent.
        s.send_to(query.id, UpdateInventory);
        return;
    }

    // Look for an arrow and remove it. The client doesn't need an update inventory packet because
    // the client removes the arrow locally.
    let opposite_hand = match interaction.hand {
        Hand::Main => Hand::Off,
        Hand::Off => Hand::Main,
    };

    let mut found_arrow = false;
    for slot_index in iter::once(inventory.get_hand_slot(opposite_hand))
        .chain(36..45)
        .chain(0..36)
    {
        let slot = &mut inventory.items.slots[slot_index];
        if slot.item == ItemKind::Arrow {
            if slot.count > 1 {
                slot.count -= 1;
            } else {
                *slot = ItemStack::EMPTY;
            }
            found_arrow = true;
            break;
        }
    }

    if !found_arrow {
        error!("client attempted to use bow without having an arrow");
        return;
    }

    let initial_speed = duration.mul_add(2.0, duration * duration).min(3.0);

    let id = s.spawn();

    #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
    let entity_id = VarInt(id.index().0 as i32);

    let uuid = Uuid::from(uuid::Uuid::new_v4());

    let (pitch_sin, pitch_cos) = query.pose.pitch.to_radians().sin_cos();
    let (yaw_sin, yaw_cos) = query.pose.yaw.to_radians().sin_cos();
    let velocity = Vec3::new(-pitch_cos * yaw_sin, -pitch_sin, pitch_cos * yaw_cos) * initial_speed;
    let encoded_velocity = Velocity(velocity.to_array().map(|a| (a * 8000.0) as i16));

    let position = query.pose.position + Vec3::new(0.0, 1.52, 0.0);

    s.insert(id, Arrow);
    s.insert(id, EntityPhysics {
        state: EntityPhysicsState::Moving { velocity },
        gravity: 0.05,
        drag: 0.01,
    });
    s.insert(id, FullEntityPose {
        position,
        yaw: query.pose.yaw,
        pitch: query.pose.pitch,
        bounding: Aabb::create(position, 0.5, 0.5), // TODO: use correct values
    });
    s.insert(id, PositionSyncMetadata::default());
    s.insert(id, uuid);

    let pkt = play::EntitySpawnS2c {
        entity_id,
        object_uuid: *uuid,
        kind: VarInt(EntityKind::ARROW.get()),
        position: position.as_dvec3(),
        pitch: ByteAngle::from_degrees(query.pose.pitch),
        yaw: ByteAngle::from_degrees(query.pose.yaw),
        head_yaw: ByteAngle::from_degrees(0.0),
        data: VarInt::default(),
        velocity: encoded_velocity,
    };

    compose.broadcast(&pkt).send().unwrap();

    // At least one velocity packet is needed for the arrow to not immediately fall to the ground,
    // so one velocity packet needs to be sent manually instead of relying on
    // sync_entity_velocity.rs in case the arrow hits a wall on the same tick.
    let pkt = play::EntityVelocityUpdateS2c {
        entity_id,
        velocity: encoded_velocity,
    };

    compose.broadcast(&pkt).send().unwrap();
}
