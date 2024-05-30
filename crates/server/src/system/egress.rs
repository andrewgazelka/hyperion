use evenio::{event::ReceiverMut, fetch::Single};
use hyperion_proto::Flush;
use prost::Message;
use tracing::instrument;

use crate::{components::EgressComm, event::Egress, net::IoBuf};

#[instrument(skip_all, level = "trace")]
pub fn egress(_: ReceiverMut<Egress>, mut io: Single<&mut IoBuf>, egress: Single<&EgressComm>) {
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

    for bytes in io.split() {
        if bytes.is_empty() {
            continue;
        }
        egress.send(bytes.freeze()).unwrap();
    }

    egress.send(FLUSH.clone()).unwrap();
}
