use ser::{
    types::{BitSet, Identifier, Nbt, Position, VarInt},
    Packet, Writable,
};

use crate::chunk::{BlockState, ChunkData, ChunkSectionBuilder};

// 0x29	Play	Client	Entity ID	Int	The player's Entity ID (EID).
// Is hardcore	Boolean
// Dimension Count	VarInt	Size of the following array.
// Dimension Names	Array of Identifier	Identifiers for all dimensions on the server.
// Max Players	VarInt	Was once used by the client to draw the player list, but now is ignored.
// View Distance	VarInt	Render distance (2-32).
// Simulation Distance	VarInt	The distance that the client will process specific things, such as
// entities. Reduced Debug Info	Boolean	If true, a Notchian client shows reduced information on the
// debug screen. For servers in development, this should almost always be false. Enable respawn
// screen	Boolean	Set to false when the doImmediateRespawn gamerule is true. Do limited crafting
// Boolean	Whether players can only craft recipes they have already unlocked. Currently unused by the
// client. Dimension Type	Identifier	The type of dimension in the minecraft:dimension_type registry,
// defined by the Registry Data packet. Dimension Name	Identifier	Name of the dimension being spawned
// into. Hashed seed	Long	First 8 bytes of the SHA-256 hash of the world's seed. Used client side for
// biome noise Game mode	Unsigned Byte	0: Survival, 1: Creative, 2: Adventure, 3: Spectator.
// Previous Game mode	Byte	-1: Undefined (null), 0: Survival, 1: Creative, 2: Adventure, 3: Spectator.
// The previous game mode. Vanilla client uses this for the debug (F3 + N & F3 + F4) game mode
// switch. (More information needed) Is Debug	Boolean	True if the world is a debug mode world; debug
// mode worlds cannot be modified and have predefined blocks. Is Flat	Boolean	True if the world is a
// superflat world; flat worlds have different void fog and a horizon at y=0 instead of y=63.
// Has death location	Boolean	If true, then the next two fields are present.
// Death dimension name	Optional Identifier	Name of the dimension the player died in.
// Death location	Optional Position	The location that the player died at.
// Portal cooldown	VarInt	The number of ticks until the player can use the portal again.
#[derive(Packet, Writable, Debug)]
#[packet(0x29)]
pub struct Login<'a> {
    pub entity_id: i32,
    pub is_hardcore: bool,
    pub dimension_names: Vec<Identifier<'a>>,
    pub max_players: VarInt,
    pub view_distance: VarInt,
    pub simulation_distance: VarInt,
    pub reduced_debug_info: bool,
    pub enable_respawn_screen: bool,
    pub do_limited_crafting: bool,
    pub dimension_type: Identifier<'a>,
    pub dimension_name: Identifier<'a>,
    pub hashed_seed: i64,
    pub game_mode: u8,
    pub previous_game_mode: i8,
    pub is_debug: bool,
    pub is_flat: bool,
    pub death_location: Option<DeathLocation<'a>>,
    pub portal_cooldown: VarInt,
}

#[derive(Writable, Debug)]
pub struct DeathLocation<'a> {
    pub dimension_name: Identifier<'a>,
    pub location: Position,
}

// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x25	Play	Client	Chunk X	Int	Chunk coordinate (block coordinate divided by 16, rounded down)
// Chunk Z	Int	Chunk coordinate (block coordinate divided by 16, rounded down)
// Heightmaps	NBT	See Chunk Format#Heightmaps structure
// Size	VarInt	Size of Data in bytes
// Data	Byte Array	See Chunk Format#Data structure
// Number of block entities	VarInt	Number of elements in the following array
// Block Entity	Packed XZ	Array	Unsigned Byte	The packed section coordinates are relative to the chunk
// they are in. Values 0-15 are valid. packed_xz = ((blockX & 15) << 4) | (blockZ & 15) // encode
// x = packed_xz >> 4, z = packed_xz & 15 // decode
// Y	Short	The height relative to the world
// Type	VarInt	The type of block entity
// Data	NBT	The block entity's data, without the X, Y, and Z values
// Sky Light Mask	BitSet	BitSet containing bits for each section in the world + 2. Each set bit
// indicates that the corresponding 16×16×16 chunk section has data in the Sky Light array below.
// The least significant bit is for blocks 16 blocks to 1 block below the min world height (one
// section below the world), while the most significant bit covers blocks 1 to 16 blocks above the
// max world height (one section above the world). Block Light Mask	BitSet	BitSet containing bits for
// each section in the world + 2. Each set bit indicates that the corresponding 16×16×16 chunk
// section has data in the Block Light array below. The order of bits is the same as in Sky Light
// Mask. Empty Sky Light Mask	BitSet	BitSet containing bits for each section in the world + 2. Each
// set bit indicates that the corresponding 16×16×16 chunk section has all zeros for its Sky Light
// data. The order of bits is the same as in Sky Light Mask. Empty Block Light Mask	BitSet	BitSet
// containing bits for each section in the world + 2. Each set bit indicates that the corresponding
// 16×16×16 chunk section has all zeros for its Block Light data. The order of bits is the same as
// in Sky Light Mask. Sky Light array count	VarInt	Number of entries in the following array; should
// match the number of bits set in Sky Light Mask Sky Light arrays	Length	Array	VarInt	Length of the
// following array in bytes (always 2048) Sky Light array	Byte Array (2048)	There is 1 array for each
// bit set to true in the sky light mask, starting with the lowest value. Half a byte per light
// value. Indexed ((y<<8) | (z<<4) | x) / 2 If there's a remainder, masked 0xF0 else 0x0F.
// Block Light array count	VarInt	Number of entries in the following array; should match the number of
// bits set in Block Light Mask Block Light arrays	Length	Array	VarInt	Length of the following array in
// bytes (always 2048) Block Light array	Byte Array (2048)	There is 1 array for each bit set to true
// in the block light mask, starting with the lowest value. Half a byte per light value. Indexed
// ((y<<8) | (z<<4) | x) / 2 If there's a remainder, masked 0xF0 else 0x0F.
#[derive(Packet, Writable)]
#[packet(0x25)]
pub struct ChunkDataAndLight {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub heightmaps: Nbt,

    /// See Chunk Format#Data structure
    pub data: Vec<u8>,

    // pub block_entities: Vec<BlockEntity>,
    pub number_of_block_entities: VarInt, // always 0

    pub sky_light_mask: BitSet,
    pub block_light_mask: BitSet,
    pub empty_sky_light_mask: BitSet,
    pub empty_block_light_mask: BitSet,
    pub sky_light_arrays: Vec<Vec<u8>>,
    pub block_light_arrays: Vec<Vec<u8>>,
}

impl ChunkDataAndLight {
    #[allow(clippy::missing_errors_doc)]
    pub fn new(chunk_x: i32, chunk_z: i32) -> anyhow::Result<Self> {
        let dirt = ChunkSectionBuilder::fill(BlockState::DIRT);

        let mut data = ChunkData::new();
        data.sections[0] = dirt.into();

        let mut bytes = Vec::new();
        data.write(&mut bytes)?;

        Ok(Self {
            chunk_x,
            chunk_z,
            heightmaps: Nbt::default(),
            data: bytes,
            number_of_block_entities: VarInt(0),
            sky_light_mask: BitSet::default(),
            block_light_mask: BitSet::default(),
            empty_sky_light_mask: BitSet::default(),
            empty_block_light_mask: BitSet::default(),
            sky_light_arrays: vec![],
            block_light_arrays: vec![],
        })
    }
}

// Chunk Biomes
// Packet ID	State	Bound To	Field Name	Field Type	Notes
// 0x0E	Play	Client
// Number of chunks	VarInt	Number of elements in the following array
// Chunk biome data	Chunk Z	Array	Int	Chunk coordinate (block coordinate divided by 16, rounded down)
// Chunk X	Int	Chunk coordinate (block coordinate divided by 16, rounded down)
// Size	VarInt	Size of Data in bytes
// Data	Byte Array	Chunk data structure, with sections containing only the Biomes field
#[derive(Packet, Writable, Debug)]
#[packet(0xe)]
pub struct ChunkBiomes {
    pub chunks: Vec<ChunkBiome>,
}

#[derive(Writable, Debug)]
pub struct ChunkBiome {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub data: Vec<u8>,
}

pub struct BlockEntity {
    /// The packed section coordinates are relative to the chunk they are in. Values 0-15 are
    /// valid.
    pub packed_xz: u8,
    /// The height relative to the world
    pub y: u16,
    /// The type of block entity
    pub r#type: VarInt,
    /// The block entity's data, without the X, Y, and Z values
    pub data: valence_nbt::Value,
}
