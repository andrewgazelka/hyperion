use flecs_ecs::prelude::*;
use tracing::trace_span;
use valence_protocol::{packets::play, ByteAngle, RawBytes, VarInt};

use crate::{
    net::{Compose, NetworkStreamRef},
    simulation::{animation::ActiveAnimation, metadata::Metadata, Position},
    system_registry::SYNC_ENTITY_POSITION,
    util::TracingExt,
};

#[derive(Component)]
pub struct SyncPositionModule;

impl Module for SyncPositionModule {
    fn module(world: &World) {
        let system_id = SYNC_ENTITY_POSITION;

        system!("sync_position", world, &Compose($), &Position, &NetworkStreamRef, &mut Metadata, &mut ActiveAnimation)
            .multi_threaded()
            .kind::<flecs::pipeline::OnStore>()
            .tracing_each_entity(
                trace_span!("sync_position"),
                move |entity, (compose, pose, &io, metadata, animation)| {
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

                    if let Some(view) = metadata.get_and_clear() {
                        let pkt = play::EntityTrackerUpdateS2c {
                            entity_id,
                            tracked_values: RawBytes(&view),
                        };

                        compose
                            .broadcast(&pkt, system_id)
                            .exclude(io)
                            .send(&world)
                            .unwrap();
                    }

                    for pkt in animation.packets(entity_id) {
                        compose
                            .broadcast(&pkt, system_id)
                            .exclude(io)
                            .send(&world)
                            .unwrap();
                    }

                    animation.clear();
                },
            );
    }
}
