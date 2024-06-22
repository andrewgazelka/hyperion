use flecs_ecs::core::{QueryAPI, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World};
use uuid::Uuid;
use valence_protocol::{
    packets::play::boss_bar_s2c::{BossBarAction, BossBarColor, BossBarDivision, BossBarFlags},
    text::IntoText,
};

use crate::{component::Player, net::Compose};

#[rustfmt::skip]
pub fn stats_message(world: &World) {
    let mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "Unknown".to_string());


    let mut players = world.new_query::<&Player>();

    world
        .system_named::<&Compose>("stats_message")
        .term_at(0).singleton()
        .each(move |compose| {
            let ms_per_tick = compose.global().ms_last_tick;
            
            let title = format!("{ms_per_tick:05.2} ms/tick, {mode}");
            let title = title.into_cow_text();
            let health = (ms_per_tick / 50.0).min(1.0);
            
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
            
            // let player_count = compose.global().shared.player_count.load(std::sync::atomic::Ordering::Relaxed);
            let player_count = players.count();
            
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
            
        });
}
