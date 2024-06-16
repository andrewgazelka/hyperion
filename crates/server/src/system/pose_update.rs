use flecs_ecs::core::{IdOperations, QueryBuilderImpl, ReactorAPI, TermBuilderImpl, World};
use tracing::instrument;
use valence_protocol::{packets::play, Encode, RawBytes, VarInt};

use crate::{
    event,
    net::{Compose, NetworkStreamRef},
};

#[instrument(skip_all)]
pub fn pose_update(world: &World) {
    world
        .observer_named::<event::PostureUpdate, (&Compose, &NetworkStreamRef)>("pose_update")
        .term_at(0)
        .singleton()
        .each_iter(|iter, idx, (compose, stream)| {
            let entity = iter.entity(idx);

            // Server to Client (S2C):
            // Entity Metadata packet (0x52).

            let entity_id = entity.id().0 as i32;

            // https://wiki.vg/Entity_metadata#Entity_Metadata_Format

            // Index	Unsigned Byte
            // Type	VarInt Enum	 (Only if Index is not 0xff; the type of the index, see the table below)
            // Value	Varies	Only if Index is not 0xff: the value of the metadata field, see the table below

            // for entity index=6 is pose
            // pose had id of 20

            // 6
            // 20
            // varint

            let mut bytes = Vec::new();
            bytes.push(6_u8);
            VarInt(20).encode(&mut bytes).unwrap();

            let pose = iter.param();
            VarInt(pose.state as i32).encode(&mut bytes).unwrap();

            // end with 0xff
            bytes.push(0xff);

            let tracker = play::EntityTrackerUpdateS2c {
                entity_id: entity_id.into(),
                tracked_values: RawBytes(&bytes),
            };

            compose.broadcast(&tracker).exclude(stream).send().unwrap();
        });
}
