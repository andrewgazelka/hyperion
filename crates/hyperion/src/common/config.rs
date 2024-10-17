//! Configuration for the server.

use std::{fmt::Debug, fs::File, io::Read, path::Path};

use flecs_ecs::macros::Component;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

/// The configuration for the server representing a `toml` file.
#[derive(Serialize, Deserialize, Debug, Component)]
pub struct Config {
    pub border_diameter: Option<f64>,
    pub max_players: i32,
    pub view_distance: i16,
    pub simulation_distance: i32,
    pub server_desc: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            border_diameter: Some(100.0),
            max_players: 10_000,
            view_distance: 32,
            simulation_distance: 10,
            server_desc: "Hyperion Test Server".to_owned(),
        }
    }
}

impl Config {
    #[instrument]
    pub fn load<P: AsRef<Path> + Debug>(path: P) -> anyhow::Result<Self> {
        info!("loading configuration file");
        if path.as_ref().exists() {
            let mut file = File::open(path)?;
            let mut contents = String::default();
            file.read_to_string(&mut contents)?;
            let config = toml::from_str::<Self>(contents.as_str())?;
            Ok(config)
        } else {
            info!("configuration file not found, using defaults");

            // make required folders
            if let Some(parent) = path.as_ref().parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    // this might happen on a read-only filesystem (i.e.,
                    // when running on a CI, profiling in Instruments, etc.)
                    warn!(
                        "failed to create parent directories for {:?}: {}, using defaults",
                        path.as_ref(),
                        e
                    );
                    return Ok(Self::default());
                }
            };

            // write default config to file
            let default_config = Self::default();
            std::fs::write(&path, toml::to_string(&default_config)?.as_bytes())?;

            info!("wrote default configuration to {:?}", path.as_ref());

            Ok(Self::default())
        }
    }
}
