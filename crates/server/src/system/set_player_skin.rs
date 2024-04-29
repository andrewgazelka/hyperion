use std::borrow::Cow;

use evenio::{
    event::{EventMut, ReceiverMut},
    query::Query,
};
use tracing::instrument;
use valence_protocol::{
    game_mode::OptGameMode,
    ident,
    packets::{play, play::player_list_s2c::PlayerListEntry},
    GameMode, VarInt,
};

use crate::{
    components::{InGameName, Uuid},
    event::SetPlayerSkin,
    net::{Compose, Packets},
};

#[derive(Query)]
pub(crate) struct SetPlayerSkinQuery<'a> {
    packets: &'a Packets,
    uuid: &'a Uuid,
    username: &'a InGameName,
}

#[instrument(skip_all, level = "trace")]
pub fn set_player_skin(r: ReceiverMut<SetPlayerSkin, SetPlayerSkinQuery>, compose: Compose) {
    let event = EventMut::take(r.event);
    let query = r.query;

    // let entity_id = query.id.index().0 as i32;
    let entity_id = 0;
    let entity_id = VarInt(entity_id);

    let entity_ids = &[entity_id];

    let packets = query.packets;

    // destroy
    let pkt = play::EntitiesDestroyS2c {
        entity_ids: Cow::Borrowed(entity_ids),
    };

    packets.append(&pkt, &compose).unwrap();

    let uuids = &[query.uuid.0];

    // player info remove
    let pkt = play::PlayerRemoveS2c {
        uuids: Cow::Borrowed(uuids),
    };

    packets.append(&pkt, &compose).unwrap();

    let skin = event.skin;

    let property = valence_protocol::profile::Property {
        name: "textures".to_string(),
        value: skin.textures,
        signature: Some(skin.signature),
    };

    let properties = &[property];

    let player_list_entry = PlayerListEntry {
        player_uuid: query.uuid.0,
        username: query.username,
        properties: Cow::Borrowed(properties),
        chat_data: None,
        listed: false,
        ping: 0,
        game_mode: GameMode::Creative,
        display_name: None,
    };

    let entries = &[player_list_entry];

    // player info add
    let pkt = play::PlayerListS2c {
        actions: play::player_list_s2c::PlayerListActions::default().with_add_player(true),
        entries: Cow::Borrowed(entries),
    };

    packets.append(&pkt, &compose).unwrap();

    let dimension_name = ident!("overworld");
    // // respawn
    let respawn = play::PlayerRespawnS2c {
        dimension_type_name: "minecraft:overworld".try_into().unwrap(),
        dimension_name: dimension_name.into(),
        hashed_seed: 0,
        game_mode: GameMode::Adventure,
        previous_game_mode: OptGameMode(Some(GameMode::Adventure)),
        is_debug: false,
        is_flat: false,
        copy_metadata: true,
        last_death_location: None,
        portal_cooldown: 42.into(),
    };

    // send_all_chunks(packets, &compose).unwrap();
    packets.append(&respawn, &compose).unwrap();
}
