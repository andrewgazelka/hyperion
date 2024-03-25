use server::Game;
use tracing_flame::FlameLayer;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[allow(dead_code)]
fn setup_global_subscriber() -> impl Drop {
    let fmt_layer = fmt::Layer::default();

    let (flame_layer, guard) = FlameLayer::with_file("./tracing.folded").unwrap();

    // Define an environment filter layer
    // This reads the `RUST_LOG` environment variable to set the log level
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info")) // Fallback to "info" level if `RUST_LOG` is not set
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(flame_layer)
        .init();

    guard
}

// https://tracing-rs.netlify.app/tracing/
fn main() -> anyhow::Result<()> {
    #[cfg(feature = "trace")]
    let _guard = setup_global_subscriber();

    let mut game = Game::init()?;
    game.game_loop();

    Ok(())
}
