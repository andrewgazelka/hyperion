use bytes::BytesMut;
use flecs_ecs::prelude::*;
use hyperion_proto::{Flush, UpdatePlayerChunkPositions};
use prost::Message;
use tracing::{error, trace_span};
use valence_protocol::{packets::play, VarInt};

use crate::{net::Compose, simulation::EgressComm};

pub mod metadata;
pub mod player_join;
mod stats;
pub mod sync_chunks;
mod sync_position;

use player_join::PlayerJoinModule;
use stats::StatsModule;
use sync_chunks::SyncChunksModule;
use sync_position::SyncPositionModule;

use crate::{
    net::NetworkStreamRef,
    simulation::{blocks::Blocks, ChunkPosition},
    system_registry::SystemId,
};

#[derive(Component)]
pub struct EgressModule;

impl Module for EgressModule {
    fn module(world: &World) {
        let flush = {
            let mut data = Vec::new();

            #[expect(
                clippy::unwrap_used,
                reason = "this is only called once on startup; it should be fine. we mostly care \
                          about crashing during server execution"
            )]
            hyperion_proto::ServerToProxy::from(Flush {})
                .encode_length_delimited(&mut data)
                .unwrap();

            // We are turning it into a `Box` first because we want to make sure the allocation is as small as possible.
            // See `Vec::leak` for more information.
            let data = data.into_boxed_slice();
            let data = Box::leak(data);
            bytes::Bytes::from_static(data)
        };

        let pipeline = world
            .entity()
            .add::<flecs::pipeline::Phase>()
            .depends_on::<flecs::pipeline::OnStore>();

        world.import::<StatsModule>();
        world.import::<PlayerJoinModule>();
        world.import::<SyncChunksModule>();
        world.import::<SyncPositionModule>();

        system!(
            "broadcast_chunk_deltas",
            world,
            &Compose($),
            &mut Blocks($),
        )
        .multi_threaded()
        .kind::<flecs::pipeline::OnUpdate>()
        .each_iter(move |it: TableIter<'_, false>, _, (compose, mc)| {
            let span = trace_span!("broadcast_chunk_deltas");
            let _enter = span.enter();

            let world = it.world();

            mc.for_each_to_update_mut(|chunk| {
                for packet in chunk.delta_drain_packets() {
                    if let Err(e) = compose.broadcast(packet, SystemId(99)).send(&world) {
                        error!("failed to send chunk delta packet: {e}");
                        return;
                    }
                }
            });
            mc.clear_should_update();

            for to_confirm in mc.to_confirm.drain(..) {
                let entity = world.entity_from_id(to_confirm.entity);

                let pkt = play::PlayerActionResponseS2c {
                    sequence: VarInt(to_confirm.sequence),
                };

                entity.get::<&NetworkStreamRef>(|stream| {
                    if let Err(e) = compose.unicast(&pkt, *stream, SystemId(99), &world) {
                        error!("failed to send player action response: {e}");
                    }
                });
            }
        });

        let player_location_query = world.new_query::<(&NetworkStreamRef, &ChunkPosition)>();

        system!(
            "egress",
            world,
            &mut Compose($),
            &mut EgressComm($),
        )
        .kind_id(pipeline)
        .each(move |(compose, egress)| {
            let span = trace_span!("egress");
            let _enter = span.enter();

            {
                let span = trace_span!("chunk_positions");
                let _enter = span.enter();

                let mut stream = Vec::new();
                let mut positions = Vec::new();

                player_location_query.each(|(io, pos)| {
                    stream.push(io.inner());

                    let position = hyperion_proto::ChunkPosition {
                        x: pos.0.x,
                        z: pos.0.y,
                    };

                    positions.push(position);
                });

                let packet = UpdatePlayerChunkPositions { stream, positions };

                let mut buffer = BytesMut::new();
                let to_send = hyperion_proto::ServerToProxy::from(packet);
                to_send.encode_length_delimited(&mut buffer).unwrap();

                if let Err(e) = egress.send(buffer.freeze()) {
                    error!("failed to send egress: {e}");
                }
            }

            let io = compose.io_buf_mut();
            for bytes in io.reset_and_split() {
                if bytes.is_empty() {
                    continue;
                }
                if let Err(e) = egress.send(bytes.freeze()) {
                    error!("failed to send egress: {e}");
                }
            }

            if let Err(e) = egress.send(flush.clone()) {
                error!("failed to send flush: {e}");
            }
        });

        system!(
            "clear_bump",
            world,
            &mut Compose($),
        )
        .kind_id(pipeline)
        .each(move |compose| {
            let span = tracing::trace_span!("clear_bump");
            let _enter = span.enter();

            for bump in &mut compose.bump {
                bump.reset();
            }
        });
    }
}
