use server::{adjust_file_limits, Game};

mod tracing_utils;

fn main() -> anyhow::Result<()> {
    tracing_utils::with_tracing(init)
}

fn init() -> anyhow::Result<()> {
    // 10k players, we want at least 2^14 = 16,384 file handles
    adjust_file_limits(16_384)?;

    let default_address = "0.0.0.0:25565";
    let mut game = Game::init(default_address)?;
    game.game_loop();
    Ok(())
}
