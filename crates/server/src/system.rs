//! All systems in the ECS framework.

#![allow(clippy::missing_docs_in_private_items, reason = "self-explanatory")]

mod block_update;
mod chat_message;
mod disguise_player;
mod egress;
mod entity_detect_collisions;
mod entity_move_logic;
mod generate_egress_packets;
pub mod ingress;
mod init_entity;
mod init_player;
mod keep_alive;
mod kill_all;
mod pkt_attack;
mod pkt_hand_swing;
mod player_detect_mob_hits;
mod player_join_world;
mod player_kick;
mod rebuild_player_location;
mod recalculate_bounding_boxes;
mod stats_message;
mod sync_entity_position;
mod sync_players;
mod update_health;
mod update_time;
mod voice_chat;

pub use block_update::block_update;
pub use chat_message::chat_message;
pub use disguise_player::disguise_player;
pub use egress::egress;
pub use entity_detect_collisions::entity_detect_collisions;
pub use entity_move_logic::entity_move_logic;
pub use generate_egress_packets::generate_egress_packets;
pub use ingress::generate_ingress_events;
pub use init_entity::init_entity;
pub use init_player::init_player;
pub use keep_alive::keep_alive;
pub use kill_all::kill_all;
pub use pkt_attack::{check_immunity, pkt_attack_entity, pkt_attack_player};
pub use pkt_hand_swing::pkt_hand_swing;
pub use player_detect_mob_hits::player_detect_mob_hits;
pub use player_join_world::player_join_world;
pub use player_kick::player_kick;
pub use rebuild_player_location::rebuild_player_location;
pub use recalculate_bounding_boxes::recalculate_bounding_boxes;
pub use stats_message::stats_message;
pub use sync_entity_position::sync_entity_position;
pub use sync_players::sync_players;
pub use update_health::update_health;
pub use update_time::update_time;
