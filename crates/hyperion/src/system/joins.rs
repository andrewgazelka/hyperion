use flecs_ecs::core::{
    flecs::pipeline::OnUpdate, Query, QueryBuilderImpl, QueryTuple, SystemAPI, TermBuilderImpl,
    World, WorldRef,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use crate::{
    component,
    component::{blocks::Blocks, Comms, InGameName, Pose, Uuid},
    net::{Compose, NetworkStreamRef},
    runtime::AsyncRuntime,
    system::player_join_world::player_join_world,
    util::player_skin::PlayerSkin,
};

struct SendableRef<'a>(WorldRef<'a>);

unsafe impl<'a> Send for SendableRef<'a> {}
unsafe impl<'a> Sync for SendableRef<'a> {}

struct SendableQuery<T>(Query<T>)
where
    T: QueryTuple;

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T: QueryTuple + Send> Send for SendableQuery<T> {}
unsafe impl<T: QueryTuple> Sync for SendableQuery<T> {}

pub fn joins(world: &'static World) {
    let query = world.new_query::<(&Uuid, &InGameName, &Pose, &PlayerSkin)>();

    let query = SendableQuery(query);

    let stages = (0..world.get_stage_count())
        .map(|stage| world.stage(stage))
        .map(SendableRef)
        .collect::<Vec<_>>();

    world
        .system::<(&AsyncRuntime, &Comms, &Blocks, &Compose)>()
        .multi_threaded() // makes it read only I think
        .kind::<OnUpdate>()
        .term_at(0)
        .singleton()
        .term_at(1)
        .singleton()
        .term_at(2)
        .singleton()
        .term_at(3)
        .singleton()
        .each(move |(tasks, comms, blocks, compose)| {
            let span = tracing::info_span!("joins");
            let _enter = span.enter();

            let mut skins = Vec::new();

            while let Ok(Some((entity, skin))) = comms.skins_rx.try_recv() {
                skins.push((entity, skin));
            }

            skins.into_par_iter().for_each(|(entity, skin)| {
                // if we are not in rayon context that means we are in a single-threaded context and 0 will work
                let idx = rayon::current_thread_index().unwrap_or(0);

                let world = &stages[idx];
                let world = world.0;

                if !world.is_alive(entity) {
                    return;
                }
                //
                let entity = world.entity_from_id(entity);
                //
                entity.add::<component::Play>();

                entity.get::<(&Uuid, &InGameName, &Pose, &NetworkStreamRef)>(
                    |(uuid, name, pose, stream_id)| {
                        let query = &query;
                        let query = &query.0;

                        player_join_world(
                            &entity, tasks, blocks, compose, uuid.0, name, stream_id, pose, &world,
                            &skin, query,
                        );
                    },
                );
                // entity.set(skin);
            });
        });
}
