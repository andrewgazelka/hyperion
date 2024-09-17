
use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    simulation::blocks::MinecraftWorld,
    valence_protocol::{math::IVec3, BlockState},
};
use ndarray::Array3;
use tracing::trace_span;

use crate::command::add_to_tree;

#[derive(Component)]
pub struct AnimationModule;

impl Module for AnimationModule {
    #[allow(clippy::excessive_nesting)]
    fn module(world: &World) {
        add_to_tree(world);

        let animate = #[coroutine]
        static move || {
            // use yield; to wait a tick
            let mut frame = Array3::from_elem((27, 40, 27), BlockState::AIR);

            let blocks = [
                BlockState::LAPIS_BLOCK,
                BlockState::COPPER_BLOCK,
                BlockState::AMETHYST_BLOCK,
                BlockState::PRISMARINE,
                BlockState::PURPUR_BLOCK,
                BlockState::QUARTZ_BLOCK,
            ];

            loop {
                let height = 20;
                let width = 10;
                let thickness = 3;
                let center_x = frame.shape()[0] / 2;
                let center_z = frame.shape()[2] / 2;

                // Build up, replacing previous blocks
                for &current_block in blocks.iter() {
                    for progress in 0..=height {
                        for y in 0..progress {
                            for x in 0..width {
                                for z in 0..thickness {
                                    if x < thickness
                                        || x >= width - thickness
                                        || (y == height / 2 || y == height / 2 + 1)
                                    {
                                        frame[[
                                            center_x + x - width / 2,
                                            y,
                                            center_z + z - thickness / 2,
                                        ]] = current_block;
                                    }
                                }
                            }
                        }
                        yield frame.clone();
                    }

                    // Wait a bit before next block type
                    for _ in 0..5 {
                        yield frame.clone();
                    }
                }

                // Wait before starting over
                for _ in 0..(height * 10) {
                    yield frame.clone();
                }
            }
        };

        let animate = Box::pin(animate);
        let mut iter = core::iter::from_coroutine(animate);

        system!("regular_animation", world, &mut MinecraftWorld($))
            .multi_threaded()
            .each_iter(move |_it: TableIter<'_, false>, _, mc| {
                let span = trace_span!("regular_animation");
                let _enter = span.enter();

                let Some(frame) = iter.next() else {
                    return;
                };

                {
                    let span = trace_span!("paste_frame");
                    let _enter = span.enter();

                    let center = IVec3::new(-438, 100, -26);
                    mc.paste(center - IVec3::new(20, 0, 20), frame.view());
                }
            });
    }
}
