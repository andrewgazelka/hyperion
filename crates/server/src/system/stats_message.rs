use evenio::prelude::*;
use tracing::instrument;
use uuid::Uuid;
use valence_protocol::{
    packets::play::boss_bar_s2c::{BossBarAction, BossBarColor, BossBarDivision, BossBarFlags},
    text::IntoText,
};

use crate::{
    event::Stats,
    global::Global,
    net::{Compose, Io},
};

#[instrument(skip_all, level = "trace")]
pub fn stats_message(r: ReceiverMut<Stats>, compose: Compose, global: Single<&Global>) {
    let event = r.event;

    let ms_per_tick = event.ms_per_tick;

    let title = format!("{ms_per_tick:05.2} ms/tick");
    let title = title.into_cow_text();
    let health = (ms_per_tick / 50.0).min(1.0) as f32;

    let color = if health > 0.5 {
        BossBarColor::Red
    } else {
        BossBarColor::White
    };

    // boss bar
    let pkt = valence_protocol::packets::play::BossBarS2c {
        id: Uuid::from_u128(0),
        action: BossBarAction::Add {
            title,
            health,
            color,
            division: BossBarDivision::NoDivision,
            flags: BossBarFlags::default(),
        },
    };

    compose.broadcast(&pkt).send().unwrap();

    let player_count = global
        .shared
        .player_count
        .load(std::sync::atomic::Ordering::Relaxed);

    let title = format!("{player_count} player online");
    let title = title.into_cow_text();
    let health = (player_count as f32 / 10_000.0).min(1.0);

    let pkt = valence_protocol::packets::play::BossBarS2c {
        id: Uuid::from_u128(1),
        action: BossBarAction::Add {
            title,
            health,
            color: BossBarColor::White,
            division: BossBarDivision::NoDivision,
            flags: BossBarFlags::default(),
        },
    };

    compose.broadcast(&pkt).send().unwrap();
}
