use evenio::{event::Receiver, fetch::Fetcher};
use tracing::instrument;
use valence_protocol::{packets::play, VarInt, Velocity};

use crate::{
    components::EntityReaction,
    event::Gametick,
    net::{Compose, StreamId},
};

fn vel_m_per_tick(input: glam::Vec3) -> Velocity {
    let input = input * 8000.0;
    let input = input.as_i16vec3();
    Velocity::from(input.to_array())
}

#[instrument(skip_all, level = "trace")]
pub fn generate_egress_packets(
    _: Receiver<Gametick>,
    mut connections: Fetcher<(&mut StreamId, &mut EntityReaction)>,
    compose: Compose,
) {
    connections.iter_mut().for_each(|(packets, reaction)| {
        if reaction.velocity.x.abs() > 0.01 || reaction.velocity.z.abs() > 0.01 {
            let vel = reaction.velocity;
            let velocity = vel_m_per_tick(vel);

            let pkt = play::EntityVelocityUpdateS2c {
                entity_id: VarInt(0), // 0 is always self as the join packet we are giving 0
                velocity,
            };

            compose.unicast(&pkt, *packets).unwrap();
        }

        *reaction = EntityReaction::default();
    });
}
