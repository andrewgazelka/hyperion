// use evenio::prelude::*;
// use tracing::instrument;
// use valence_protocol::{packets::play, VarInt};
//
// use crate::{
//     components::{EntityReaction, FullEntityPose, ImmuneStatus, Player, Vitals},
//     event::AttackEntity,
//     net::{Compose, StreamId},
// };
//
// #[derive(Query)]
// pub struct AttackPlayerQuery<'a> {
//     id: EntityId,
//     packets: &'a mut StreamId,
//     _player: With<&'static Player>,
// }
//
// #[derive(Query)]
// pub struct AttackEntityQuery<'a> {
//     id: EntityId,
//     pose: &'a FullEntityPose,
//     reaction: &'a mut EntityReaction,
//     immunity: &'a mut ImmuneStatus,
//     vitals: &'a mut Vitals,
// }

use flecs_ecs::{
    core::{flecs, Entity, IdOperations, QueryBuilderImpl, ReactorAPI, TermBuilderImpl, World},
    macros::observer,
};
use tracing::instrument;
use valence_protocol::{packets::play, Encode, RawBytes, VarInt};
use valence_text::IntoText;

use crate::{
    component::{Health, InGameName, Player},
    event::AttackEntity,
    net::{Compose, IoRef},
};

// #[instrument(skip_all, level = "trace")]
// // Check immunity of the entity being attacked
// pub fn check_immunity(
//     global: Single<&crate::global::Global>,
//     attack: ReceiverMut<AttackEntity, &ImmuneStatus>,
// ) {
//     if attack.query.is_invincible(&global) {
//         EventMut::take(attack.event);
//     }
// }

/// send Packet to player encoder
#[instrument(skip_all, level = "trace")]
pub fn send_pkt_attack_player(world: &World) {
    world
        .observer_named::<AttackEntity, (&IoRef, &Compose, &flecs::Any)>("send_pkt_attack_player")
        .term_at(0)
        .filter()
        .term_at(1)
        .singleton()
        .each_iter(|iter, _, (stream, compose, _)| {
            let event = iter.param();

            let mut damage_broadcast = get_red_hit_packet(event.from);
            damage_broadcast.entity_id = VarInt(0);

            compose.unicast(&damage_broadcast, stream).unwrap();
        });
}
// pub fn packet_attack_entity(world: &'static World) {
//     world
//         .observer_named::<AttackEntity, (&mut StreamId, &Compose, &flecs::Any)>(
//             "packet_attack_entity",
//         )
//         .term_at(0)
//         .filter()
//         .term_at(1)
//         .filter()
//         .each_iter(|iter, idx, (stream, compose, _)| {
//             let event = iter.param();
//
//             let mut damage_broadcast = get_package(event.from);
//             damage_broadcast.entity_id = VarInt(0);
//
//             compose.unicast(&damage_broadcast, stream).unwrap();
//         });
// }

// /// Handle Damage and knockback
// #[instrument(skip_all, level = "trace")]
// pub fn pkt_attack_entity(
//     global: Single<&crate::global::Global>,
//     attack: Receiver<AttackEntity, AttackEntityQuery>,
//     compose: Compose,
// ) {
//     let AttackEntityQuery {
//         id: entity_id,
//         pose,
//         reaction,
//         vitals,
//         immunity,
//     } = attack.query;
//
//     let damage_broadcast = get_package(entity_id);
//
//     compose.broadcast(&damage_broadcast).send().unwrap();
//
//     let event = attack.event;
//
//     let this = pose.position;
//     let other = event.from_pos;
//
//     let delta_x = other.x - this.x;
//     let delta_z = other.z - this.z;
//
//     if delta_x.abs() < 0.01 && delta_z.abs() < 0.01 {
//         // todo: implement like vanilla
//         return;
//     }
//
//     let dist_xz = delta_x.hypot(delta_z);
//     let multiplier = 0.4;
//
//     reaction.velocity.x /= 2.0;
//     reaction.velocity.y /= 2.0;
//     reaction.velocity.z /= 2.0;
//     reaction.velocity.x -= delta_x / dist_xz * multiplier;
//     reaction.velocity.y += multiplier;
//     reaction.velocity.z -= delta_z / dist_xz * multiplier;
//
//     if reaction.velocity.y > 0.4 {
//         reaction.velocity.y = 0.4;
//     }
//
//     vitals.hurt(&global, event.damage, immunity);
// }

pub fn pkt_attack_entity(world: &World) {
    observer!(
        "pkt_attack_entity",
        world,
        AttackEntity,
        &Compose,
        &InGameName,
        &mut Health,
        &Player
    )
    .term_at(0)
    .singleton()
    .term_at(1)
    .filter()
    .each_entity(|attacked, (compose, ign, health, _)| {
        let damage_broadcast = get_red_hit_packet(attacked.id());
        compose.broadcast(&damage_broadcast).send().unwrap();

        health.normal -= 1.0;

        let entity_id = VarInt(attacked.id().0 as i32);

        // https://wiki.vg/Entity_metadata#Entity_Metadata_Format
        // 9 = Health, type = float
        let mut bytes = Vec::new();
        bytes.push(9_u8);
        VarInt(3).encode(&mut bytes).unwrap();
        (health.normal).encode(&mut bytes).unwrap();

        // end with 0xff
        bytes.push(0xff);

        let tracker = play::EntityTrackerUpdateS2c {
            entity_id,
            tracked_values: RawBytes(&bytes),
        };

        compose.broadcast(&tracker).send().unwrap();

        // let display_name = format!("{ign} {health}");
        // let display_name_tab = display_name.clone().into_cow_text();

        // let entry = crate::system::player_join_world::list::PlayerListEntry {
        //     player_uuid: uuid.0,
        //     username: Cow::Borrowed(&display_name),
        //     properties: Cow::Borrowed(&[]),
        //     chat_data: None,
        //     listed: true,
        //     ping: 20,
        //     game_mode: GameMode::Survival,
        //     display_name: Some(display_name_tab.clone()),
        // };

        // compose
        //     .broadcast(&crate::system::player_join_world::list::PlayerListS2c {
        //         actions: crate::system::player_join_world::list::PlayerListActions::default()
        //             .with_add_player(true)
        //             .with_update_listed(true)
        //             .with_update_display_name(true),
        //         entries: Cow::Owned(vec![entry]),
        //     })
        //     .send()
        //     .unwrap();

        // send chat message
        let msg = format!("{ign} -> health: {health}");

        compose
            .broadcast(&play::GameMessageS2c {
                chat: msg.into_cow_text(),
                overlay: false,
            })
            .send()
            .unwrap();
    });
}

#[instrument(skip_all, level = "trace")]
fn get_red_hit_packet(id: Entity) -> play::EntityDamageS2c {
    // todo
    play::EntityDamageS2c {
        entity_id: VarInt(id.0 as i32),
        source_type_id: VarInt::default(),
        source_cause_id: VarInt::default(),
        source_direct_id: VarInt::default(),
        source_pos: None,
    }
}
