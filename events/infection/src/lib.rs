#![feature(lint_reasons)]
#![feature(allocator_api)]

use server::{valence_server::protocol::anyhow, Game};

mod components;
mod system;

pub fn init_game() -> anyhow::Result<()> {
    let mut game = Game::init_with("127.0.0.1:25567", |world| {
        // join events
        world.add_handler(system::scramble_player_name);
        world.add_handler(system::assign_team_on_join);

        world.add_handler(system::disable_attack_team);

        world.add_handler(system::deny_block_break);

        world.add_handler(system::respawn_on_death);

        world.add_handler(system::bump_into_player);

        // commands
        world.add_handler(system::zombie_command);
    })?;

    game.game_loop();
    Ok(())
}
