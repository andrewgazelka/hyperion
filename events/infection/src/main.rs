use infection::init_game;

fn main() {
    tracing_subscriber::fmt::init();
    init_game().unwrap();
}
