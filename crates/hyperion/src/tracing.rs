use flecs_ecs::core::{private, Builder, ComponentId, QueryTuple, SystemAPI};

// pub trait TracingExt<'a, P, T>: SystemAPI<'a, P, T>
// where
//     T: QueryTuple,
//     P: ComponentId,
// {
//     fn trace_each_entity<F>(&mut self, f: F)
//     where
//         F: FnMut(Builder<'a, P, T>),
//     {
//         self.iter_mut().for_each(|iter| {
//             let span = tracing::trace_span!("trace_each_entity");
//             let _enter = span.enter();
//             iter.iter_stage(self.world()).for_each(|entity| {
//                 let entity = iter.entity(entity);
//                 f(entity);
//             });
//         });
//     }
// }
