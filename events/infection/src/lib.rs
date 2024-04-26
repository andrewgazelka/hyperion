use server::Game;

mod system;

pub fn init_game() {
    let mut game = Game::init("127.0.0.1:25567").unwrap();

    let world = game.world_mut();

    world.add_handler(system::deny_block_break);

    game.game_loop();
}
