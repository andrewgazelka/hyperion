use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::Compose,
    system_registry::SystemId,
    valence_protocol::{packets::play, text::IntoText},
};
use jemalloc_ctl::{epoch, stats};

#[derive(Component)]
pub struct StatsModule;

impl Module for StatsModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        let mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "Unknown".to_string());

        let mut last_frame_time_total = 0.0;

        system!("stats", world, &Compose($))
            .multi_threaded()
            .each_iter(move |it, _, compose| {
                // Update the epoch to get the most recent stats
                epoch::advance().unwrap();

                let world = it.world();
                let player_count = compose
                    .global()
                    .player_count
                    .load(std::sync::atomic::Ordering::Relaxed);

                let info = world.info();
                let current_frame_time_total = info.frame_time_total;

                let ms_per_tick = (current_frame_time_total - last_frame_time_total) * 1000.0;
                last_frame_time_total = current_frame_time_total;

                let allocated = stats::allocated::read().unwrap();
                let active = stats::active::read().unwrap();
                let resident = stats::resident::read().unwrap();

                let allocated_gib = allocated as f64 / 1_073_741_824.0;
                let active_gib = active as f64 / 1_073_741_824.0;
                let resident_gib = resident as f64 / 1_073_741_824.0;

                let title = format!(
                    "§6§l{:.2} ms/tick §r| §b{}§r\n§aAllocated: {:.2} GiB §r| §eActive: {:.2} GiB \
                     §r| §cResident: {:.2} GiB",
                    ms_per_tick, mode, allocated_gib, active_gib, resident_gib
                );

                let footer = format!("§d§l{} players online", player_count);

                let pkt = play::PlayerListHeaderS2c {
                    header: title.into_cow_text(),
                    footer: footer.into_cow_text(),
                };

                compose.broadcast(&pkt, SystemId(99)).send(&world).unwrap();
            });
    }
}
