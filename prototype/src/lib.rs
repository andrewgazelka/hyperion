#![feature(split_at_checked)]

use std::collections::HashMap;

use crate::messages::{Message, MessageSender};

// todo
type PlayerInventory = ();

type PartitionId = usize;
type PlayerId = usize;
type ChunkId = (i32, i32);
type MultiMap<K, V> = HashMap<K, Vec<V>>;
type Chunked<T> = Vec<T>;

type IdMap<T> = Vec<T>;

mod global;
mod messages;
mod utils;

struct Location {
    x: f64,
    y: f64,
    z: f64,
}

struct Player {
    partition_in: PartitionId,
    inventory: PlayerInventory,
    location: Location,
}

struct Entity {
    partition_in: PartitionId,
    location: Location,
}

struct Packet {
    // ...
    group: Option<PacketGroup>,
}

enum PacketGroup {
    PlayerLocal,
    RegionLocal,
    Global,
    CrossRegion { from: usize, to: usize },
}

impl Packet {
    // also take context of world
    fn group(&self) -> PacketGroup {
        todo!()
    }
}

struct World {
    partitions: Vec<Partition>,
    players: Vec<Player>,
    entities: Vec<Entity>,
}

struct Partition {
    players_in: Vec<PlayerId>,
    chunk_ids: Vec<ChunkId>,
    adjacent: Vec<usize>,
}

// struct World {
//     partitions: [Partition; 64]
//     local_packets: Recv<Packet>,
//     global_packes: Recv<Packet>
// }
//
// struct Thread {
//     partition: Partiton
// }

struct Thread {
    partition: Partition,
    messages: MessageSender,
}

impl Thread {
    fn process(&mut self, entities: &mut [Entity], world: &World) -> anyhow::Result<()> {
        for entity in entities {
            
        }
        Ok(())
    }
}

// impl Thread {
//    fn sar
//
//
//
//
//
// }

// impl Thread {
//     // iterator of players not related to partition... should be equal split
//     // between threads
//     fn assigned_global_players(&mut self) -> impl Iterator<&mut Player>
//
//     // iterator of entities not related to partition... should be equal split
//     // between threads
//     fn assigned_global_living_entities(&mut self) -> impl Iterator<&mut Entity>
//
//     fn players_in_region(&mut self) -> impl Iterator<&mut Player>
//
//     fn general(world: &WorldState, messages: &mut Messages) {
//         for entity in world.assigned_global_entities() {
//             if let Some(message) = entity.physics() {
//                 // new location of the entity,
//                 // the entity (say an arrow) hitting a player for instance
//                 messages.push(message);
//             }
//         }
//
//         for player in world.assigned_global_players() {
//             if let Some(message) = player.physics() {
//                 // new location of the entity,
//                 // the entity (say an arrow) hitting a player for instance
//                 messages.push(message);
//             }
//         }
//     }
//
//     fn apply_messages(world: &WorldState, input_messages: &Messages)  {
//
//     }
//
//     fn run_cycle() {
//
//
//         for player in self.assigned_global_players() {
//             for packet in player.packets() {
//                 let group = packet.assign_group();
//
//                 if group == PlayerLocal {
//                     // process
//                 }
//             }
//         }
//
//         for entity in self.assigned_global_living_entities() {
//             // we probably do not need to modify any blocks etc for basic entities
//             entity.physics()
//         }
//
//
//
//
//
//
//     }
// }
