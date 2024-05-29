use std::cell::RefCell;

use evenio::event::{EventMut, ReceiverMut, Sender};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tracing::instrument;

use crate::{
    event,
    event::{AttackType, BulkShoved},
};

#[instrument(skip_all, level = "trace")]
pub fn shoved_reaction(r: ReceiverMut<BulkShoved>, s: Sender<event::AttackEntity>) {
    let event = EventMut::take(r.event);

    let result = event
        .0
        .into_inner()
        .into_vec()
        .into_par_iter()
        .map(RefCell::into_inner)
        .flatten()
        .map(|event| {
            (event.target, event::AttackEntity {
                from_pos: event.from_location,
                from: event.from,
                damage: 3.0,
                source: AttackType::Shove,
            })
        })
        .collect_vec_list();

    for (target, event) in result.into_iter().flatten() {
        s.send_to(target, event);
    }
}
