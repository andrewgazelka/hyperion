#![allow(clippy::unnecessary_wraps)]
#![allow(unused)]

use std::process::id;

use evenio::{
    entity::EntityId,
    event::Receiver,
    fetch::{Fetcher, Single},
    query::{Not, Query},
    rayon::prelude::*,
};
use sha2::digest::generic_array::arr;
use tracing::info;
use valence_protocol::{math::DVec2, ByteAngle, VarInt};

use crate::{
    bounding_box::{BoundingBox, CollisionContext, EntityBoundingBoxes},
    EntityReaction, FullEntityPose, Gametick, MinecraftEntity, Player, RunningSpeed,
};

#[derive(Query, Debug)]
pub struct EntityQuery<'a> {
    id: EntityId,
    entity: &'a MinecraftEntity,

    running_speed: Option<&'a RunningSpeed>,
    pose: &'a mut FullEntityPose,
}

pub fn call(
    _: Receiver<Gametick>,
    entity_bounding_boxes: Single<&EntityBoundingBoxes>,
    mut poses_fetcher: Fetcher<(EntityId, &FullEntityPose, &EntityReaction)>,
) {
    use valence_protocol::packets::play;

    let entity_bounding_boxes = entity_bounding_boxes.0;

    poses_fetcher.par_iter().for_each(|(id, pose, reaction)| {
        let context = CollisionContext {
            bounding: pose.bounding,
            id,
        };

        let collisions = entity_bounding_boxes.get_collisions(context, &poses_fetcher);

        for (id, other_pose) in collisions {
            // safety: this is safe because we are doing this to one entity at a time so there
            // is never a case where we are borrowing the same entity twice
            unsafe { pose.apply_entity_collision(&other_pose, reaction) }
        }
    });
}
