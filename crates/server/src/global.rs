use std::sync::atomic::AtomicU32;

pub struct Global {
    // realistically, we will never have more than 2^32 = 4,294,967,296 players
    pub player_count: AtomicU32,
}
