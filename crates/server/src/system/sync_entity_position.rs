use evenio::prelude::*;
use glam::{Vec2, Vec3};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;
use valence_protocol::{packets::play, ByteAngle, VarInt};

use crate::{
    components::{FullEntityPose, Uuid},
    event::Gametick,
    net::{Broadcast, Compose},
    singleton::broadcast::{PacketMetadata, PacketNecessity},
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    uuid: &'a Uuid,
    pose: &'a mut FullEntityPose,
    last_pose: &'a mut PositionSyncMetadata,
}

#[derive(Component, Copy, Clone, Debug, Default)]
pub struct PositionSyncMetadata {
    pub last_pose: Option<FullEntityPose>,
    pub rounding_error: Vec3,
    pub needs_resync: bool,
}

#[instrument(skip_all, level = "trace")]
pub fn sync_entity_position(
    _: Receiver<Gametick>,
    mut entities: Fetcher<EntityQuery>,
    broadcast: Single<&Broadcast>,
    compose: Compose,
) {
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery {
            id,
            uuid,
            pose,
            last_pose: sync_meta,
        } = query;
        let pos = pose.position;
        let pitch = ByteAngle::from_degrees(pose.pitch);
        let yaw = ByteAngle::from_degrees(pose.yaw);

        let movement = if let PositionSyncMetadata {
            last_pose: Some(last_pose),
            rounding_error,
            needs_resync,
        } = sync_meta
        {
            // Account for past rounding errors
            let last_pos = last_pose.position + *rounding_error;

            if *needs_resync
                || (pos.x - last_pos.x).abs() > 8.0
                || (pos.y - last_pos.y).abs() > 8.0
                || (pos.z - last_pos.z).abs() > 8.0
            {
                EntityMovement::Teleport { pos, pitch, yaw }
            } else {
                #[expect(clippy::float_cmp, reason = "Change detection")]
                let (position, rotation) = {
                    let position = last_pose.position != pose.position;
                    let rotation = last_pose.yaw != pose.yaw || last_pose.pitch != pose.pitch;

                    (position, rotation)
                };

                #[expect(
                    clippy::suboptimal_flops,
                    clippy::cast_lossless,
                    reason = "Calculate it the same way as Minecraft"
                )]
                let delta = {
                    // From wiki.vg
                    let delta_x = (pos.x * 32.0 - last_pos.x * 32.0) * 128.0;
                    let delta_y = (pos.y * 32.0 - last_pos.y * 32.0) * 128.0;
                    let delta_z = (pos.z * 32.0 - last_pos.z * 32.0) * 128.0;
                    let delta = [delta_x as i16, delta_y as i16, delta_z as i16];

                    // Prevent desync due to rounding error
                    *rounding_error = Vec3::new(
                        (delta_x / 128.0 - delta[0] as f32 / 128.0) / 32.0,
                        (delta_y / 128.0 - delta[1] as f32 / 128.0) / 32.0,
                        (delta_z / 128.0 - delta[2] as f32 / 128.0) / 32.0,
                    );

                    delta
                };

                match (position, rotation) {
                    (true, true) => EntityMovement::PositionAndRotation { delta, pitch, yaw },
                    (true, false) => EntityMovement::Position { delta },
                    (false, true) => EntityMovement::Rotation { pitch, yaw },
                    (false, false) => EntityMovement::None,
                }
            }
        } else {
            EntityMovement::Teleport { pos, pitch, yaw }
        };

        // FIXME: Relative move packets arent really dropable. Use some kind of an LOD system?
        // FIXME: Currenly passes normal entities to excluse as well players. Is this a problem?
        let metadata = PacketMetadata {
            necessity: PacketNecessity::Droppable {
                prioritize_location: Vec2::new(pose.position.x, pose.position.y),
            },
            exclude_player: Some(uuid.0),
        };

        movement.write_packets(id, &broadcast, metadata, &compose);

        if let EntityMovement::Teleport { .. } = movement {
            sync_meta.rounding_error = Vec3::ZERO;
            sync_meta.needs_resync = false;
        }

        sync_meta.last_pose = Some(*pose);
    });
}

pub enum EntityMovement {
    PositionAndRotation {
        delta: [i16; 3],
        pitch: ByteAngle,
        yaw: ByteAngle,
    },
    Position {
        delta: [i16; 3],
    },
    Rotation {
        pitch: ByteAngle,
        yaw: ByteAngle,
    },
    Teleport {
        pos: Vec3,
        pitch: ByteAngle,
        yaw: ByteAngle,
    },
    None,
}

impl EntityMovement {
    fn write_packets(
        &self,
        id: EntityId,
        broadcast: &Broadcast,
        _metadata: PacketMetadata,
        compose: &Compose,
    ) {
        #[expect(
            clippy::cast_possible_wrap,
            reason = "wrapping is okay in this scenario"
        )]
        let entity_id = VarInt(id.index().0 as i32);

        // TODO: calculate on_ground
        // TODO: remove unwrap
        match *self {
            Self::PositionAndRotation { delta, pitch, yaw } => {
                let pos = play::RotateAndMoveRelativeS2c {
                    entity_id,
                    delta,
                    pitch,
                    yaw,
                    on_ground: false,
                };

                let look = play::EntitySetHeadYawS2c {
                    entity_id,
                    head_yaw: yaw,
                };

                broadcast.append(&pos, compose).unwrap();
                broadcast.append(&look, compose).unwrap();
            }
            Self::Position { delta } => {
                let pos = play::MoveRelativeS2c {
                    entity_id,
                    delta,
                    on_ground: false,
                };

                broadcast.append(&pos, compose).unwrap();
            }
            Self::Rotation { pitch, yaw } => {
                let pos = play::RotateS2c {
                    entity_id,
                    pitch,
                    yaw,
                    on_ground: false,
                };

                let look = play::EntitySetHeadYawS2c {
                    entity_id,
                    head_yaw: yaw,
                };

                broadcast.append(&pos, compose).unwrap();
                broadcast.append(&look, compose).unwrap();
            }
            Self::Teleport { pos, pitch, yaw } => {
                let pos = play::EntityPositionS2c {
                    entity_id,
                    position: pos.as_dvec3(),
                    yaw,
                    pitch,
                    on_ground: false,
                };

                let look = play::EntitySetHeadYawS2c {
                    entity_id,
                    head_yaw: yaw,
                };

                broadcast.append(&pos, compose).unwrap();
                broadcast.append(&look, compose).unwrap();
            }
            Self::None => {}
        }
    }
}
