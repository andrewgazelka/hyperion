use flecs_ecs::prelude::*;
use tracing::{error, trace_span};

use crate::{
    net::Compose,
    simulation::{blocks::Blocks, Play},
};

#[derive(Component)]
pub struct StatsModule;

impl Module for StatsModule {
    fn module(world: &World) {
        let players = world.new_query::<&Play>();

        // let last_frame_time_total = 0.0;

        // let system_id = GLOBAL_STATS;

        system!("global_update", world, &mut Compose($))
            .kind::<flecs::pipeline::OnUpdate>() // ? OnUpdate
            .each_iter(move |_, _, compose| {
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
