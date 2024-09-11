use flecs_ecs::{
    core::{
        flecs::pipeline, QueryAPI, QueryBuilderImpl, SystemAPI, TermBuilderImpl, World,
        WorldProvider,
    },
    macros::system,
};
use tracing::trace_span;
use uuid::Uuid;
use valence_protocol::{
    packets::play::boss_bar_s2c::{BossBarAction, BossBarColor, BossBarDivision, BossBarFlags},
    text::IntoText,
};

use crate::{
    component::Play,
    net::{Compose, NetworkStreamRef},
    system::chunk_comm::ChunkSendQueue,
    tracing_ext::TracingExt,
    SystemRegistry,
};

pub fn stats(world: &World, registry: &mut SystemRegistry) {
    let mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "Unknown".to_string());

    let mut players = world.new_query::<&Play>();

    let mut last_frame_time_total = 0.0;

    let system_id = registry.register();

    system!(
        "global_stats",
        world,
        &mut Compose($),
    )
    .kind::<pipeline::OnUpdate>()
    .each_iter(move |iter, _, compose| {
        let world = iter.world();

        compose.global_mut().tick += 1;

        let span = tracing::trace_span!("stats_message");
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

        // let player_count = compose.global().shared.player_count.load(std::sync::atomic::Ordering::Relaxed);
        let player_count = players.count();
        let player_count = usize::try_from(player_count).expect("failed to convert player count");

        compose
            .global()
            .player_count
            .store(player_count, std::sync::atomic::Ordering::Relaxed);

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

    let system_id = registry.register();

    system!(
        "local_stats",
        world,
        &Compose($),
        &ChunkSendQueue,
        &NetworkStreamRef,
    )
    .multi_threaded()
    .kind::<pipeline::OnUpdate>()
    .tracing_each_entity(
        trace_span!("local_stats"),
        move |entity, (compose, chunk_send_queue, stream)| {
            const FULL_BAR_CHUNKS: usize = 4096;

            let world = entity.world();
            let chunks_to_send = chunk_send_queue.len();

            let title = format!("{chunks_to_send} chunks to send");
            let title = title.into_cow_text();

            let health = (chunks_to_send as f32 / FULL_BAR_CHUNKS as f32).min(1.0);

            let pkt = valence_protocol::packets::play::BossBarS2c {
                id: Uuid::from_u128(2),
                action: BossBarAction::Add {
                    title,
                    health,
                    color: BossBarColor::Red,
                    division: BossBarDivision::NoDivision,
                    flags: BossBarFlags::default(),
                },
            };

            compose.unicast(&pkt, *stream, system_id, &world).unwrap();
        },
    );
}
