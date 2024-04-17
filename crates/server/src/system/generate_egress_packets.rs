use evenio::{
    event::Receiver,
    fetch::{Fetcher, Single},
};
use tracing::instrument;
use valence_protocol::{packets::play, VarInt, Velocity};

use crate::{components::EntityReaction, events::Gametick, global::Global, net::LocalEncoder};

fn vel_m_per_tick(input: glam::Vec3) -> Velocity {
    let input = input * 8000.0;
    let input = input.as_i16vec3();
    Velocity::from(input.to_array())
}

#[instrument(skip_all, level = "trace")]
pub fn generate_egress_packets(
    _: Receiver<Gametick>,
    global: Single<&Global>,
    mut connections: Fetcher<(&mut LocalEncoder, &mut EntityReaction)>,
) {
    connections.iter_mut().for_each(|(encoder, reaction)| {
        if reaction.velocity.x.abs() > 0.01 || reaction.velocity.z.abs() > 0.01 {
            let vel = reaction.velocity;
            // vel *= 10.0;
            let velocity = vel_m_per_tick(vel);

            encoder
                .append(
                    &play::EntityVelocityUpdateS2c {
                        entity_id: VarInt(0), // 0 is always self as the join packet we are giving 0
                        velocity,
                    },
                    &global,
                )
                .unwrap();
        }

        *reaction = EntityReaction::default();
    });
}
