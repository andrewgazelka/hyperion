use flecs_ecs::prelude::*;
use tracing::{error, trace_span};
use uuid::Uuid;
use valence_protocol::{
    packets::play::boss_bar_s2c::{BossBarAction, BossBarColor, BossBarDivision, BossBarFlags},
    text::IntoText,
};

use crate::{
    net::Compose,
    simulation::{blocks::Blocks, Play},
    system_registry::GLOBAL_STATS,
};

#[derive(Component)]
pub struct StatsModule;

impl Module for StatsModule {
    fn module(world: &World) {

        let mut players = world.new_query::<&Play>();

        // let last_frame_time_total = 0.0;

        // let system_id = GLOBAL_STATS;

        system!("global_update", world, &mut Compose($))
            .kind::<flecs::pipeline::OnUpdate>() // ? OnUpdate
            .each_iter(move |iter, _, compose| {
                let world = iter.world();

                let global = compose.global_mut();

                global.tick += 1;

                // let player_count = compose.global().shared.player_count.load(std::sync::atomic::Ordering::Relaxed);
                let player_count = players.count();

                let Ok(player_count) = usize::try_from(player_count) else {
                    // should never be a negative number. this is just in case.
                    error!("failed to convert player count to usize. Was {player_count}");
                    return;
                };

                *global.player_count.get_mut() = player_count;

                // todo: move back but into infection crate
                // let span = trace_span!("stats_message");
                // let _enter = span.enter();
                // let info = world.info();
                // 
                // let current_frame_time_total = info.frame_time_total;
                // let ms_per_tick = (current_frame_time_total - last_frame_time_total) * 1000.0;
                // last_frame_time_total = current_frame_time_total;
                // 
                // let title = format!("{ms_per_tick:05.2} ms/tick, {mode}");
                // let title = title.into_cow_text();
                // let health = (ms_per_tick / 50.0).min(1.0);
                // 
                // let color = if health > 0.5 {
                //     BossBarColor::Red
                // } else {
                //     BossBarColor::White
                // };
                // 
                // // boss bar
                // let pkt = valence_protocol::packets::play::BossBarS2c {
                //     id: Uuid::from_u128(0),
                //     action: BossBarAction::Add {
                //         title,
                //         health,
                //         color,
                //         division: BossBarDivision::NoDivision,
                //         flags: BossBarFlags::default(),
                //     },
                // };
                // 
                // compose.broadcast(&pkt, system_id).send(&world).unwrap();
                // 
                // let title = format!("{player_count} player online");
                // let title = title.into_cow_text();
                // let health = (player_count as f32 / 10_000.0).min(1.0);
                // 
                // let pkt = valence_protocol::packets::play::BossBarS2c {
                //     id: Uuid::from_u128(1),
                //     action: BossBarAction::Add {
                //         title,
                //         health,
                //         color: BossBarColor::White,
                //         division: BossBarDivision::NoDivision,
                //         flags: BossBarFlags::default(),
                //     },
                // };
                // 
                // compose.broadcast(&pkt, system_id).send(&world).unwrap();
            });

        system!(
            "load_pending",
            world,
            &mut Blocks($),
        )
        .kind::<flecs::pipeline::OnUpdate>()
        .each_iter(|_iter, _, blocks| {
            let span = trace_span!("load_pending");
            let _enter = span.enter();
            blocks.load_pending();
        });
    }
}
