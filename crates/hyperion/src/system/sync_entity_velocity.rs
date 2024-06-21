use evenio::prelude::*;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;
use valence_protocol::{packets::play, VarInt, Velocity};

use crate::{
    components::{EntityPhysics, EntityPhysicsState},
    event::Gametick,
    net::Compose,
};

#[derive(Query, Debug)]
pub(crate) struct EntityQuery<'a> {
    id: EntityId,
    physics: &'a mut EntityPhysics,
}

#[instrument(skip_all, level = "trace")]
pub fn sync_entity_velocity(
    _: Receiver<Gametick>,
    mut entities: Fetcher<EntityQuery>,
    compose: Compose,
) {
    entities.par_iter_mut().for_each(|query| {
        let EntityQuery { id, physics } = query;

        let EntityPhysicsState::Moving { velocity } = physics.state else {
            return;
        };

        #[expect(clippy::cast_possible_wrap, reason = "wrapping is ok in this case")]
        let entity_id = VarInt(id.index().0 as i32);

        let pkt = play::EntityVelocityUpdateS2c {
            entity_id,
            velocity: Velocity(velocity.to_array().map(|a| (a * 8000.0) as i16)),
        };

        compose.broadcast(&pkt).send().unwrap();
    });
}
