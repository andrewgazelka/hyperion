//! All systems in the ECS framework.

#![allow(clippy::missing_docs_in_private_items, reason = "self-explanatory")]

pub mod chunk_comm;
pub mod ingress;

pub mod plugin;

pub mod player_join_world;
pub mod stats;
pub mod sync_entity_position;

pub mod joins;

pub mod egress;
