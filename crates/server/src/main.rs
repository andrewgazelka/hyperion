use server::Game;

#[cfg(feature = "trace")]
fn setup_global_subscriber() -> impl Drop {
    use tracing_flame::FlameLayer;
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
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

#[cfg(all(feature = "trace-simple", not(feature = "trace")))]
fn setup_simple_trace() {
    tracing_subscriber::fmt::try_init().unwrap();
}

// https://tracing-rs.netlify.app/tracing/
fn main() -> anyhow::Result<()> {
    #[cfg(all(feature = "trace-simple", not(feature = "trace")))]
    setup_simple_trace();

    #[cfg(feature = "trace")]
    let _guard = setup_global_subscriber();

    #[cfg(feature = "pprof")]
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(2999)
        .blocklist(&["libc", "libgcc", "pthread", "vdso", "rayon"])
        .build()
        .unwrap();

    let mut game = Game::init()?;
    game.game_loop();

    #[cfg(feature = "pprof")]
    if let Ok(report) = guard.report().build() {
        let file = std::fs::File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
    };

    Ok(())
}
