use evenio::prelude::*;
use evenio::rayon::prelude::*;

use crate::{Player, TpsEvent};

pub fn call(
    r: Receiver<TpsEvent>,
    mut players: Fetcher<&mut Player>,
) {
    let ms_per_tick = r.event.ms_per_tick;
    
    // with 4 zeroes
    // lead 2 zeroes
    let message = format!("MSPT: {ms_per_tick:07.4}");
    
    players.par_iter_mut().for_each(|player| {
        // todo: handle error
        let _ = player.packets.writer.send_chat_message(&message);
    });
}
