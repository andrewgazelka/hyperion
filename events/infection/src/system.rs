use server::{
    evenio::{
        entity::EntityId,
        event::{Receiver, Sender},
    },
    event,
    valence_server::{
        text::{Color, IntoText},
        BlockState, Text,
    },
};

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
        to: r.event.by,
        message,
    });
}
