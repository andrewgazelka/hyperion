#![feature(lint_reasons)]

use server::{valence_server::protocol::anyhow, Game};

mod system;

pub fn init_game() -> anyhow::Result<()> {
    let mut game = Game::init("127.0.0.1:25567")?;

    let world = game.world_mut();

    world.add_handler(system::deny_block_break);

    game.game_loop();
    Ok(())
}
