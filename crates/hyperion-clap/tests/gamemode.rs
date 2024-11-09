use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "gamemode")]
#[command(about = "Change the gamemode of a player")]
struct Gamemode {
    /// The gamemode to set
    #[arg(value_enum)]
    mode: hyperion_clap::GameMode,

    /// The player to change the gamemode of
    player: Option<String>,
}
