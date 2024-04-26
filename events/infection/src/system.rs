#![allow(
    clippy::needless_pass_by_value,
    reason = "this is used in the event loop"
)]

use server::{
    evenio::{
        entity::EntityId,
        event::{Receiver, ReceiverMut, Sender},
    },
    event,
    valence_server::{
        entity::EntityKind,
        text::{Color, IntoText},
        BlockState, Text,
    },
};

// makes it easier to test with the same account
pub fn scramble_player_name(mut r: ReceiverMut<event::PlayerInit, ()>) {
    // 10 alphanumeric name using fastrand

    let mut name = String::new();
    for _ in 0..10 {
        name.push(fastrand::alphanumeric());
    }

    r.event.username = name.into_boxed_str();
}

pub fn deny_block_break(
    r: Receiver<event::BlockFinishBreak, EntityId>,
    mut s: Sender<(event::UpdateBlock, event::ChatMessage)>,
) {
    s.send(event::UpdateBlock {
        position: r.event.position,
        id: BlockState::STONE,
        sequence: r.event.sequence,
    });

    let message = Text::text("You cannot break this block").color(Color::RED);

    s.send(event::ChatMessage {
        target: r.event.by,
        message,
    });
}

pub fn disguise_player_command(
    r: Receiver<event::Command, EntityId>,
    mut s: Sender<(event::DisguisePlayer, event::ChatMessage)>,
) {
    let raw = &r.event.raw;

    // todo: how to do commands in non O(n) time?
    if raw != "disguise" {
        return;
    }

    let target = r.query;

    s.send(event::ChatMessage {
        target,
        message: Text::text("Disguising"),
    });

    s.send(event::DisguisePlayer {
        target,
        mob: EntityKind::ZOMBIE,
    });
}
