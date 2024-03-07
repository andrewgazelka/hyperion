Given a machine with `64` cores:

The world is allocated into 64 `Partition`s where each partition is assigned to a certain region

```rust

struct Player {
    partition_id: usize
    inventory: Inventory
}

struct Entity {
    location: Location
    partion_id: usize
}

struct Packet {
    // ...
    group: Option<PacketGroup>
}

enum PacketGroup {
    PlayerLocal,
    RegionLocal,
    Global,
    CrossRegion {
        from: usize,
        to: usize
    },

}

impl Packet {
    fn group(&self, client: &ClientState, world: &World) -> PacketGroup {
        if self.requires_global_mut() {
            return PacketGroup::Global;
        }

        if let Some(boundary) = self.crosses_boundary(client, world) {
            return PacketGroup::CrossRegion(boundary)
        }

        if self.no_region_modification() {
            return PacketGroup::Player
        }

        // Confirm Teleportation 
        // Chat Message
        // Edit Book (modified player inv)
        // Keep Alive
        PacketGroup::Region
    }

    fn crosses_boundary(&self, client: &ClientState, world: &World) -> Option<CrossRegion> {
        match self.kind {
            SetPlayerPositionAndRotation | MoveVehicle (new_loc, ...) => world.get_opt_cross(client.loc, new_loc)
            _ => None
        }
    }

    /// do not care about region even
    fn client(&self) {
        // Confirm Teleportation 
        // Chat Message
        // Edit Book (modified player inv)
        // Keep Alive
    }

    fn regional(&self, client: &Client, world: &World) {
        // Click Container (assuming it does not cross regions)
        // Close Container
        // Interact (set to the partition of the entity)
    }

    fn requires_global_mut(&self) -> bool {
        // Confirm Teleportation 
        // 
    }
}

struct Partition {
    player_ids: Vec<usize>
    chunk_ids: Vec<usize>
    adjacent: Vec<usize>
}

struct World {
    partitions: [Partition; 64]
    local_packets: Recv<Packet>,
    global_packes: Recv<Packet>
}

struct Thread {
    partition: Partiton
}

impl Thread {
    // iterator of players not related to partition... should be equal split
    // between threads
    fn assigned_global_players(&mut self) -> impl Iterator<&mut Player>

    // iterator of entities not related to partition... should be equal split
    // between threads
    fn assigned_global_living_entities(&mut self) -> impl Iterator<&mut Entity>

    fn players_in_region(&mut self) -> impl Iterator<&mut Player>

    fn general(world: &WorldState, messages: &mut Messages) {
        for entity in world.assigned_global_entities() {
            if let Some(message) = entity.physics() {
                // new location of the entity,
                // the entity (say an arrow) hitting a player for instance
                messages.push(message);
            }
        }

        for player in world.assigned_global_players() {
            if let Some(message) = player.physics() {
                // new location of the entity,
                // the entity (say an arrow) hitting a player for instance
                messages.push(message);
            }
        }
    }

    fn apply_messages(world: &WorldState, input_messages: &Messages)  {
        
    }

    fn run_cycle() {


        for player in self.assigned_global_players() {
            for packet in player.packets() {
                let group = packet.assign_group();

                if group == PlayerLocal {
                    // process
                }
            }
        } 

        for entity in self.assigned_global_living_entities() {
            // we probably do not need to modify any blocks etc for basic entities
            entity.physics()
        }






    }
}


```

```rust
