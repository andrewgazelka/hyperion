pub const REMOVE_PLAYER_FROM_VISIBILITY: SystemId = SystemId(1);
pub const GLOBAL_STATS: SystemId = SystemId(1);
pub const PLAYER_JOINS: SystemId = SystemId(2);
pub const GENERATE_CHUNK_CHANGES: SystemId = SystemId(3);
pub const SEND_FULL_LOADED_CHUNKS: SystemId = SystemId(4);
pub const LOCAL_STATS: SystemId = SystemId(5);
pub const RECV_DATA: SystemId = SystemId(0); // todo: change back to 6
pub const SYNC_ENTITY_POSITION: SystemId = SystemId(7);

#[derive(Copy, Clone, Debug)]
pub struct SystemId(pub u16);

impl SystemId {
    pub const fn id(self) -> u16 {
        self.0
    }
}
