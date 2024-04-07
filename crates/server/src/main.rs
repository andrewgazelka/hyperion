use anyhow::Context;
use server::{adjust_file_limits, Game};

mod tracing_utils;

fn main() -> anyhow::Result<()> {
    tracing_utils::with_tracing(init)
}

fn init() -> anyhow::Result<()> {
    // 10k players, we want at least 2^14 = 16,384 file handles
    adjust_file_limits(16_384)?;

    rayon::ThreadPoolBuilder::new()
        .spawn_handler(|thread| {
            std::thread::spawn(|| no_denormals::no_denormals(|| thread.run()));
            Ok(())
        })
        .build_global()
        .context("failed to build thread pool")?;

    let default_address = "0.0.0.0:25565";

    no_denormals::no_denormals(|| {
        let mut game = Game::init(default_address)?;
        game.game_loop();
        Ok(())
    })
}
