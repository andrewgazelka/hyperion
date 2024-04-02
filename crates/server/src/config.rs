use std::fs::File;
use std::io::Read;
use std::path::Path;
use evenio::component::Component;
use serde::{Deserialize, Serialize};
use spin::lazy::Lazy;
use tracing::{info, instrument};

pub static CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::load("run/config.toml").unwrap()
});

#[derive(Serialize, Deserialize, Debug, Component)]
pub struct Config {
    pub border_diameter: Option<f64>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub server_desc: String
}

impl Default for Config {
    fn default() -> Self {
        Self {
            border_diameter: Some(100.0),
            max_players: 10_000,
            view_distance: 32,
            simulation_distance: 10,
            server_desc: "10k babyyyy".to_owned()
        }
    }
}

impl Config {
    #[instrument(skip_all)]
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        info!("loading configuration file");
        if path.as_ref().exists() {
            let mut file = File::open(path)?;
            let mut contents = String::default();
            file.read_to_string(&mut contents)?;
            let config = toml::from_str::<Config>(contents.as_str())?;
            Ok(config)
        } else {
            info!("configuration file not found, using defaults");
            Ok(Self::default())
        }
    }
}
