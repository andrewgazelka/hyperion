use antithesis::random::AntithesisRng;
use rand::Rng;
use tokio::io::AsyncWriteExt;
use valence_protocol::{
    Bounded, Encode, PROTOCOL_VERSION, VarInt, packets,
    packets::handshaking::handshake_c2s::HandshakeNextState,
};

use crate::{Bot, util::random_either};

