use crate::{utils::split_into_mut, Entity, IdMap, Thread, World};

struct Global {
    threads: Vec<Thread>,
    entities: IdMap<Entity>,
    world: World,
}

impl Global {
    
    fn parallel_tasks(&mut self) {
        rayon::scope(|s| {
            let threads = &mut self.threads;
            let entities = split_into_mut(threads.len(), &mut self.entities);
            let world = &self.world;

            for (thread, entities) in threads.iter_mut().zip(entities) {
                s.spawn(move |_| {
                    // unwrap not ideal
                    #[allow(clippy::unwrap_used)]
                    thread.process(entities, world).unwrap();
                });
            }
        });
    }
    
    fn run_cycle(&mut self) {
        self.parallel_tasks();
    }
}
