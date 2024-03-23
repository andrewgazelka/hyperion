use evenio::{event::Receiver, fetch::Fetcher, rayon::prelude::*};

use crate::{Gametick, Zombie};

pub fn keep_alive(_: Receiver<Gametick>, mut zombie: Fetcher<&mut Zombie>) {
    zombie.par_iter_mut().for_each(|zombie| {});
}
