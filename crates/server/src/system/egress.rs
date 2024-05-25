use evenio::{event::ReceiverMut, fetch::Single};
use tracing::instrument;

use crate::{
    components::{EgressComm, Singleton},
    event::Egress,
    net::Io,
};

#[instrument(skip_all, level = "trace")]
pub fn egress(_: ReceiverMut<Egress>, mut io: Single<&mut Io>, egress: Singleton<EgressComm>) {
    for bytes in io.split() {
        egress.send(bytes.freeze()).unwrap();
    }
}
