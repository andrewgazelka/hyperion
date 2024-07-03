use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::system,
};
use hyperion_proto::Flush;
use prost::Message;
use tracing::instrument;

use crate::{component::EgressComm, net::Compose, CustomPipeline};

#[instrument(skip_all, level = "trace")]
pub fn egress(world: &World, pipeline: &CustomPipeline) {
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

    system!(
        "egress",
        world,
        &mut Compose($),
        &mut EgressComm($),
    )
    .kind_id(pipeline.egress)
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
