use flecs_ecs::prelude::*;
use hyperion_proto::Flush;
use prost::Message;

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

#[derive(Component)]
pub struct EgressModule;

impl Module for EgressModule {
    fn module(world: &World) {
        // ByteMut::with_capacity(1024);
        // ByteMut [----------------------------------------------------------------------] ALLOC [A]
        // we write 30 bytes to the buffer
        // BytesMut [ 30 bytes here ] [ ------------------------------------------------- ] ALLOC [A]
        // .split(&mut self)
        // returned: BytesMut [ 30 bytes here ] ALLOC [A]
        //
        //                                             start ptr
        //                                               v
        // left in previous bytesMut [ UNUSED 30 bytes ] [ ----------------------------- ] ALLOC [A]

        static FLUSH: once_cell::sync::Lazy<bytes::Bytes> = once_cell::sync::Lazy::new(|| {
            let mut data = Vec::new();
            hyperion_proto::ServerToProxy::from(Flush {})
                .encode_length_delimited(&mut data)
                .unwrap();

            // We are turning it into a `Box` first because we want to make sure the allocation is as small as possible.
            // See `Vec::leak` for more information.
            let data = data.into_boxed_slice();
            let data = Box::leak(data);
            bytes::Bytes::from_static(data)
        });
        
        let pipeline = world
            .entity()
            .add::<flecs::pipeline::Phase>()
            .depends_on::<flecs::pipeline::OnStore>();

        world.import::<StatsModule>();
        world.import::<PlayerJoinModule>();
        world.import::<SyncChunksModule>();
        world.import::<SyncPositionModule>();


        system!(
            "egress",
            world,
            &mut Compose($),
            &mut EgressComm($),
        )
        .kind_id(pipeline)
        .each(|(compose, egress)| {
            let span = tracing::trace_span!("egress");
            let _enter = span.enter();
            let io = compose.io_buf_mut();
            for bytes in io.reset_and_split() {
                if bytes.is_empty() {
                    continue;
                }
                egress.send(bytes.freeze()).unwrap();
            }

            egress.send(FLUSH.clone()).unwrap();
        });
    }
}
