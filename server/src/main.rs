#![allow(clippy::many_single_char_names)]
// #![allow(unused)]

extern crate core;
mod chunk;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;
use std::time::{Duration, Instant};
use evenio::prelude::*;
use evenio::rayon::prelude::*;
use valence_protocol::decode::PacketFrame;
use valence_protocol::math::DVec3;

use crate::handshake::{Io, server};

mod handshake;
mod global;

mod packets;

// A zero-sized component, often called a "marker" or "tag".
#[derive(Component)]
struct Player {
    input: flume::Receiver<PacketFrame>,
    locale: Option<String>,
}

#[derive(Event)]
struct InitPlayer {
    player: EntityId,
    io: Io,
    pos: [f32; 3],
}

#[derive(Event)]
struct SendChat<'a> {
    x: &'a str
}

#[derive(Event)]
struct Gametick;

struct Game {
    world: World,
    global: Arc<global::Global>,
    incoming: flume::Receiver<Io>,
}

impl Game {
    fn tick(&mut self) {
        while let Ok(io) = self.incoming.try_recv() {
            let player = self.world.spawn();

            let event = InitPlayer {
                player,
                io,
                pos: [0.0, 0.0, 0.0],
            };

            self.world.send(event);
        }


    }
}

// The `Receiver<Tick>` parameter tells our handler to listen for the `Tick` event.
fn update_positions(_: Receiver<Gametick>, mut fetcher: Fetcher<(&mut Player, &mut Position)>, mut s: Sender<SendChat>) {
    fetcher.par_iter_mut().for_each(|(player| {
        while let Ok(packet) = player.input.try_recv() {
            match packet {
                PacketFrame::TeleportConfirmC2s(packet) => {
                    println!("Teleport confirm: {:?}", packet);
                }
                _ => {}
            }
        }
    }
}




fn tick() -> anyhow::Result<()> {

}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::Subscriber::builder().init();
    
    let global = global::Global {
        player_count: 0,
    };
    
    let global = Arc::new(global);

    let server = server(global)?;

    let mut last_tick = Instant::now();
    let tick_duration = Duration::from_millis(20);

    let mut world = World::new();

    world.add_handler(init_player_system);
    world.add_handler(update_positions);

    loop {
        // Perform your tick logic here
        println!("Tick!");

        //         server.try_recv()
        while let Ok(io) = server.try_recv() {
            let player = world.spawn();

            let event = InitPlayer {
                player,
                io,
                pos: [0.0, 0.0, 0.0],
            };

            world.send(event);
        }

        // Calculate the elapsed time since the last tick
        let elapsed = last_tick.elapsed();

        // If the elapsed time is greater than the desired tick duration,
        // skip the sleep to catch up
        if elapsed < tick_duration {
            let sleep_duration = tick_duration - elapsed;
            std::thread::sleep(sleep_duration);
        }

        // Update the last tick time
        last_tick = Instant::now();
    }

    // let entity = world.spawn();
    // 
    // world.send(InitPlayer {
    //     player: entity,
    //     pos: [24.0, 24.0, 24.0],
    // });

    // Ok(())
}

#[derive(Component)]
struct Health(i32);

#[derive(Component)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Component)]
struct FullEntityPose {
    position: DVec3,
    yaw: f32,
    pitch: f32,
}

#[derive(Component)]
struct Monster;

fn init_player_system(
    r: Receiver<InitPlayer>,
    mut s: Sender<(Insert<Health>, Insert<Position>, Insert<Player>)>,
) {
    // let InitPlayer {
    //     player: entity,
    //     pos: [x, y, z],
    // } = *r.event;
    // 
    // s.insert(entity, Health(20));
    // s.insert(entity, Position { x, y, z });
    // s.insert(entity, Player);
}
