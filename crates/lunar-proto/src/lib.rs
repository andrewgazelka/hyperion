#![allow(clippy::doc_markdown)]

use prost::{bytes::BufMut, Message};
use prost_types::Any;
include!(concat!(env!("OUT_DIR"), "/schema.rs"));

pub trait Sendable {
    fn send(&self, buf: &mut impl BufMut);
}

impl Sendable for OverrideServerRichPresenceMessage {
    fn send(&self, result_buf: &mut impl BufMut) {
        let mut buf = Vec::new();

        self.encode(&mut buf).unwrap();

        // az'ollode:reung

        Any {
            type_url: "type.googleapis.com/lunarclient.apollo.richpresence.v1.\
                       OverrideServerRichPresenceMessage"
                .to_string(),
            value: buf,
        }
        .encode(result_buf)
        .unwrap();
    }
}
