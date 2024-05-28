use evenio::{event::ReceiverMut, fetch::Single};
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

    for bytes in io.split() {
        if bytes.is_empty() {
            continue;
        }
        egress.send(bytes.freeze()).unwrap();
    }
}
