//! All systems in the ECS framework.

#![allow(clippy::missing_docs_in_private_items, reason = "self-explanatory")]

// pub mod block_update;
// pub mod chat_message;
pub mod chunk_comm;
// pub mod compass;
// pub mod despawn_player;
// pub mod disguise_player;
// pub mod effect;
pub mod egress;
// pub mod entity_detect_collisions;
// pub mod entity_move_logic;
// pub mod entity_physics;
// pub mod generate_egress_packets;
pub mod ingress;

pub mod plugin;

// pub mod init_entity;
// pub mod init_player;
// pub mod inventory_systems;
// pub mod keep_alive;
// pub mod kill_all;
// pub mod pkt_attack;
// pub mod pkt_hand_swing;
// pub mod player_detect_mob_hits;
pub mod event_handler;
pub mod player_join_world;
pub mod stats;
pub mod sync_entity_position;
// pub mod player_kick;
// pub mod pose_update;
// pub mod rebuild_player_location;
// pub mod recalculate_bounding_boxes;
// pub mod release_item;
// pub mod set_player_skin;
// pub mod shoved_reaction;
// pub mod speed;
// pub mod stats_message;
// pub mod sync_entity_position;
// pub mod sync_entity_velocity;
// pub mod sync_players;
// pub mod teleport;
// pub mod time;
// pub mod update_equipment;
// pub mod update_health;
// pub mod voice_chat;

pub mod joins;
