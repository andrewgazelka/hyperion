use server::Game;

fn main() -> anyhow::Result<()> {
    let mut game = Game::init()?;
    game.game_loop();

    Ok(())
}
