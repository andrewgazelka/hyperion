use flecs_ecs::prelude::*;
use tracing::trace_span;
use uuid::Uuid;
use valence_protocol::{
    packets::play::boss_bar_s2c::{BossBarAction, BossBarColor, BossBarDivision, BossBarFlags},
    text::IntoText,
};

use crate::{global::GLOBAL_STATS, net::Compose, simulation::Play};

#[derive(Component)]
pub struct StatsModule;

impl Module for StatsModule {
    fn module(world: &World) {
        let mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "Unknown".to_string());

        let mut players = world.new_query::<&Play>();

        let mut last_frame_time_total = 0.0;

        let system_id = GLOBAL_STATS;

        system!("global_stats", world, &mut Compose($))
            .kind::<flecs::pipeline::OnUpdate>() // ? OnUpdate
            .each_iter(move |iter, _, compose| {
                let world = iter.world();

                let global = compose.global_mut();

                global.tick += 1;

                // let player_count = compose.global().shared.player_count.load(std::sync::atomic::Ordering::Relaxed);
                let player_count = players.count();
                let player_count =
                    usize::try_from(player_count).expect("failed to convert player count");

                *global.player_count.get_mut() = player_count;

                let span = trace_span!("stats_message");
                let _enter = span.enter();
                let info = world.info();

                let current_frame_time_total = info.frame_time_total;
                let ms_per_tick = (current_frame_time_total - last_frame_time_total) * 1000.0;
                last_frame_time_total = current_frame_time_total;

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

                compose.broadcast(&pkt, system_id).send(&world).unwrap();

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

                compose.broadcast(&pkt, system_id).send(&world).unwrap();
            });
    }
}
