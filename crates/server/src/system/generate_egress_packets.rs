use evenio::{event::Receiver, fetch::Fetcher};
use rayon::prelude::*;
use tracing::instrument;
use valence_protocol::{packets::play, VarInt, Velocity};

use crate::{net::Encoder, EntityReaction, Gametick};

fn vel_m_per_tick(input: glam::Vec3) -> Velocity {
    let input = input * 8000.0;
    let input = input.as_i16vec3();
    Velocity::from(input.to_array())
}

#[instrument(skip_all, level = "trace")]
pub fn generate_egress_packets(
    _: Receiver<Gametick>,
    mut connections: Fetcher<(&mut Encoder, &mut EntityReaction)>,
) {
    connections.par_iter_mut().for_each(|(encoder, reaction)| {
        if reaction.velocity.x.abs() > 0.01 || reaction.velocity.z.abs() > 0.01 {
            let vel = reaction.velocity;
            // vel *= 10.0;
            let velocity = vel_m_per_tick(vel);

            encoder
                .encode(&play::EntityVelocityUpdateS2c {
                    entity_id: VarInt(0), // 0 is always self as the join packet we are giving 0
                    velocity,
                })
                .unwrap();
        }

        *reaction = EntityReaction::default();
    });
}
