use evenio::event::{Receiver, Sender};
use tracing::instrument;

use crate::{
    event,
    event::{AttackType, Shoved},
};

#[instrument(skip_all, level = "trace")]
pub fn shoved_reaction(r: Receiver<Shoved, ()>, mut s: Sender<event::AttackEntity>) {
    s.send(event::AttackEntity {
        target: r.event.target,
        from_pos: r.event.from_location,
        from: r.event.from,
        // todo: determine damage
        damage: 3.0,
        source: AttackType::Shove,
    });
}
