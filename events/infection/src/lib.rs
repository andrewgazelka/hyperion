#![feature(lint_reasons)]

use server::{valence_server::protocol::anyhow, Game};

mod system;

pub fn init_game() -> anyhow::Result<()> {
    let mut game = Game::init_with("127.0.0.1:25567", |world| {
        world.add_handler(system::scramble_player_name);
        world.add_handler(system::deny_block_break);
        world.add_handler(system::disguise_player_command);
    })?;

    game.game_loop();
    Ok(())
}
