use clap::{command, CommandFactory, Parser};
use clap_complete::{generate, Shell};
use server::Game;

use crate::tracing_utils::with_tracing;

mod tracing_utils;

// https://tracing-rs.netlify.app/tracing/
fn main() -> anyhow::Result<()> {
    with_tracing(run)
}

#[derive(Parser)] // requires `derive` feature
#[command(name = "hyperion", version)]
#[command(bin_name = "hyperion")]
enum CargoCli {
    ExampleDerive(ExampleDeriveArgs),
}

#[derive(clap::Args)]
#[command(version, about, long_about = None)]
struct ExampleDeriveArgs {
    #[arg(long)]
    manifest_path: Option<std::path::PathBuf>,
}

fn run() -> anyhow::Result<()> {
    let args = CargoCli::parse();
    // let mut command = CargoCli::command();
    // 
    // let name = command.get_name().to_string();
    // generate(Shell::Fish, &mut command, name, &mut std::io::stdout());

    // clap_complete::generate(clap::Command::new("hyperion"), Shell::Bash, "hyperion", &mut std::io::stdout())?;
    // let x = clap::Command::new("hyperion")
    //     .gen
    //
    let default_address = "0.0.0.0:25565";
    let mut game = Game::init(default_address)?;
    game.game_loop();
    Ok(())
}
