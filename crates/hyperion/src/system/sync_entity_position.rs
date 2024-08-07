use flecs_ecs::{
    core::{
        flecs::pipeline::OnUpdate, IdOperations, QueryBuilderImpl, TermBuilderImpl, World,
        WorldProvider,
    },
    macros::system,
};
use tracing::trace_span;
use valence_protocol::{packets::play, ByteAngle, VarInt};

use crate::{
    component::Pose,
    net::{Compose, NetworkStreamRef},
    tracing_ext::TracingExt,
    SystemRegistry,
};

pub fn sync_entity_position(world: &World, registry: &mut SystemRegistry) {
    let system_id = registry.register();
    system!("sync_entity_position", world, &Compose($), &Pose, &NetworkStreamRef)
        .multi_threaded()
        .kind::<OnUpdate>()
        .tracing_each_entity(
            trace_span!("sync_entity_position"),
            move |entity, (compose, pose, &io)| {
                let entity_id = VarInt(entity.id().0 as i32);

                let world = entity.world();

                let pkt = play::EntityPositionS2c {
                    entity_id,
                    position: pose.position.as_dvec3(),
                    yaw: ByteAngle::from_degrees(pose.yaw),
                    pitch: ByteAngle::from_degrees(pose.pitch),
                    on_ground: false,
                };

                compose
                    .broadcast(&pkt, system_id)
                    .exclude(io)
                    .send(&world)
                    .unwrap();

                let pkt = play::EntitySetHeadYawS2c {
                    entity_id,
                    head_yaw: ByteAngle::from_degrees(pose.yaw),
                };

                compose
                    .broadcast(&pkt, system_id)
                    .exclude(io)
                    .send(&world)
                    .unwrap();
            },
        );
}
