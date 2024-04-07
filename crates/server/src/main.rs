use server::Game;

mod tracing_utils;

fn main() -> anyhow::Result<()> {
    tracing_utils::with_tracing(init)
}

fn init() -> anyhow::Result<()> {
    let default_address = "0.0.0.0:25565";
    let mut game = Game::init(default_address)?;
    game.game_loop();
    Ok(())
}
