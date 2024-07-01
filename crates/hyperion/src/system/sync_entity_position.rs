// use evenio::prelude::*;
// use glam::{Vec2, Vec3};
// use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
// use tracing::instrument;
// use valence_protocol::{packets::play, ByteAngle, VarInt};
//
// use crate::{
//     components::{FullEntityPose, Uuid},
//     event::Gametick,
//     net::Compose,
//     singleton::broadcast::{PacketMetadata, PacketNecessity},
// };

// #[derive(Query, Debug)]
// pub(crate) struct EntityQuery<'a> {
//     id: EntityId,
//     uuid: &'a Uuid,
//
//     pose: &'a mut FullEntityPose,
//     last_pose: &'a mut PositionSyncMetadata,
// }
//

// #[derive(Component, Copy, Clone, Debug, Default)]
// pub struct PositionSyncMetadata {
//     pub last_pose: Option<Pose>,
//     pub rounding_error: Vec3,
//     pub needs_resync: bool,
// }

use flecs_ecs::{
    core::{IdOperations, IntoWorld, QueryBuilderImpl, TermBuilderImpl, World},
    macros::system,
};
use tracing::trace_span;
use valence_protocol::{packets::play, ByteAngle, VarInt};

use crate::{
    component::Pose,
    net::{Compose, NetworkStreamRef},
    tracing_ext::TracingExt,
};

pub fn sync_entity_position(world: &World) {
    system!("sync_entity_position", world, &Compose($), &Pose, &NetworkStreamRef)
        .multi_threaded()
        .tracing_each_entity(
            trace_span!("sync_entity_position"),
            |entity, (compose, pose, io)| {
                let entity_id = VarInt(entity.id().0 as i32);

                let world = entity.world();

                let pkt = play::EntityPositionS2c {
                    entity_id,
                    position: pose.position.as_dvec3(),
                    yaw: ByteAngle::from_degrees(pose.yaw),
                    pitch: ByteAngle::from_degrees(pose.pitch),
                    on_ground: false,
                };

                compose.broadcast(&pkt).exclude(io).send(&world).unwrap();
            },
        );
}
