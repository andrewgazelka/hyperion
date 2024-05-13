use evenio::{
    event::{Receiver, ReceiverMut, Sender},
    fetch::Single,
};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use tracing::instrument;

use crate::{
    event,
    event::{AttackType, BulkShoved},
};

#[instrument(skip_all, level = "trace")]
pub fn shoved_reaction(mut r: ReceiverMut<BulkShoved>, mut s: Sender<event::AttackEntity>) {
    let result = r
        .event
        .0
        .get_all_mut()
        .par_iter_mut()
        .flatten()
        .map(|event| {
            event::AttackEntity {
                target: event.target,
                from_pos: event.from_location,
                from: event.from,
                // todo: determine damage
                damage: 3.0,
                source: AttackType::Shove,
            }
        })
        .collect_vec_list();

    for event in result.into_iter().flatten() {
        s.send(event);
    }
}
