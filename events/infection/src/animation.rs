use flecs_ecs::{
    core::{QueryBuilderImpl, SystemAPI, TableIter, TermBuilderImpl, World},
    macros::{system, Component},
    prelude::Module,
};
use hyperion::{
    net::{Compose, NetworkStreamRef},
    simulation::{
        blocks::{chunk::LoadedChunk, frame::Frame, MinecraftWorld},
        InGameName, Uuid,
    },
    system_registry::SystemId,
    valence_protocol::{math::IVec3, BlockPos, BlockState},
};
use ndarray::Array3;
use tracing::{debug, trace_span};

use crate::{
    command::{add_to_tree, parse},
    component::team::Team,
};

#[derive(Component)]
pub struct AnimationModule;

impl Module for AnimationModule {
    fn module(world: &World) {
        add_to_tree(world);

        let mut tick = 0;

        system!("regular_animation", world, &mut MinecraftWorld($))
            .multi_threaded()
            .each_iter(move |it: TableIter<'_, false>, _, (mc)| {
                let span = trace_span!("regular_animation");
                let _enter = span.enter();
            
                let world = it.world();
            
                tick += 1;
                
                if tick < 100 {
                    return;
                }
            
                // Create a DNA double helix animation using Frame API
                let radius = 5.0;
                let height = 40;
                let speed = 0.5;
                let angle = tick as f32 * speed;
            
                // Create a frame for the DNA structure
            
                let mut frame = Array3::from_elem((27, 40, 27), BlockState::AIR);
            
                for y in 0..height {
                    let helix_angle = (y as f32).mul_add(0.3, angle);
                    let x1 = (radius * helix_angle.cos()) as i32 + 20;
                    let z1 = (radius * helix_angle.sin()) as i32 + 20;
                    let x2 = (radius * (helix_angle + std::f32::consts::PI).cos()) as i32 + 20;
                    let z2 = (radius * (helix_angle + std::f32::consts::PI).sin()) as i32 + 20;

                    {
                        let x1 = usize::try_from(x1).unwrap();
                        let z1 = usize::try_from(z1).unwrap();
                        let x2 = usize::try_from(x2).unwrap();
                        let z2 = usize::try_from(z2).unwrap();

                        // Create the two strands of the DNA
                        frame[(x1, y, z1)] = BlockState::DIAMOND_BLOCK;
                        frame[(x2, y, z2)] = BlockState::EMERALD_BLOCK;
                    }
            
                    // Create the "rungs" of the DNA ladder
                    if y % 2 == 0 {
                        for t in 0..=20 {
                            let xt = x1 + (x2 - x1) * t / 20;
                            let zt = z1 + (z2 - z1) * t / 20;
                            
                            let xt = usize::try_from(xt).unwrap();
                            let zt = usize::try_from(zt).unwrap();
            
                            frame[(xt, y, zt)] = BlockState::GOLD_BLOCK;
                        }
                    }
                }
            
                // Paste the frame into the world
                let center = IVec3::new(-438, 100, -26);
                let frame = Frame::from(frame);
                frame.paste(center - IVec3::new(20, 0, 20), mc);
            });
    }
}
