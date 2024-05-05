#![feature(lint_reasons)]
#![feature(allocator_api)]

use std::net::ToSocketAddrs;

use evenio::{entity::EntityId, event::Event};
use server::{valence_server::protocol::anyhow, Hyperion};

use crate::components::HumanLocations;

mod components;
mod system;

#[derive(Event)]
struct ToZombie {
    #[event(target)]
    target: EntityId,
}

pub fn init_game(address: impl ToSocketAddrs + Send + Sync + 'static) -> anyhow::Result<()> {
    let mut game = Hyperion::init_with(address, |world| {
        // join events
        world.add_handler(system::scramble_player_name);
        world.add_handler(system::assign_team_on_join);

        world.add_handler(system::disable_attack_team);

        // world.add_handler(system::deny_block_break);

        world.add_handler(system::respawn_on_death);

        world.add_handler(system::bump_into_player);

        // commands
        world.add_handler(system::zombie_command);

        world.add_handler(system::calculate_chunk_level_bvh);
        world.add_handler(system::point_close_player);

        world.add_handler(system::to_zombie);

        let locations = world.spawn();
        world.insert(locations, HumanLocations::default());
    })?;

    game.game_loop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use server::{
        util::{mojang::MojangClient, player_skin::PlayerSkin},
        uuid::uuid,
    };

    // /give @p minecraft:player_head{SkullOwner:{Id:[I;2085624826,-1409197759,-1436634827,1159444628],Properties:{textures:[{Value:"e3RleHR1cmVzOntTS0lOOnt1cmw6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvNDJiNDNhYzg0ZjkwMGEyNDE0NmZhNTJhYjk1OTc3ZmVjMmY2YTNmYjA5NzNlZDFkNDcxMzFlMWNlZmE0ZTk3MiJ9fX0="}]}},display:{Lore:["{\"text\":\"https://namemc.com/skin/c55038d86e00003a\"}"]}}
    // textures:[{Value:"e3RleHR1cmVzOntTS0lOOnt1cmw6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvNDJiNDNhYzg0ZjkwMGEyNDE0NmZhNTJhYjk1OTc3ZmVjMmY2YTNmYjA5NzNlZDFkNDcxMzFlMWNlZmE0ZTk3MiJ9fX0="}]}},display:{Lore:["{\"text\":\"https://namemc.com/skin/c55038d86e00003a\"}"]}}
    #[tokio::test]
    async fn gen_zombie_skin() {
        // let textures = "e3RleHR1cmVzOntTS0lOOnt1cmw6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvNDJiNDNhYzg0ZjkwMGEyNDE0NmZhNTJhYjk1OTc3ZmVjMmY2YTNmYjA5NzNlZDFkNDcxMzFlMWNlZmE0ZTk3MiJ9fX0=";
        let uuid_to_get_skin = uuid!("c0b5eca8-5000-4101-b511-44a532130abf");

        let mojang = MojangClient::default();
        let skin = PlayerSkin::from_uuid(uuid_to_get_skin, &mojang)
            .await
            .unwrap()
            .unwrap();

        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open("zombie_skin.json")
            .unwrap();

        serde_json::to_writer_pretty(file, &skin).unwrap();
    }
}
