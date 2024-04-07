use std::{fmt::Debug, fs::File, io::Read, path::Path};

use serde::{Deserialize, Serialize};
use spin::lazy::Lazy;
use tracing::{info, instrument, warn};

mod default;

pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::load("run/config.toml").unwrap());

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub border_diameter: Option<f64>,
    pub max_players: i32,
    pub view_distance: i32,
    pub address: String,
    pub simulation_distance: i32,
    pub server_desc: String,
    pub server_image: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            border_diameter: Some(100.0),
            max_players: 10_000,
            view_distance: 32,
            address: "0.0.0.0:25565".to_owned(),
            simulation_distance: 10,
            server_desc: "Hyperion".to_owned(),
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
            warn!("configuration file not found, using defaults");
            Ok(Self::default())
        }
    }
}
