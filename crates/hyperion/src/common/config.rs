//! Configuration for the server.

use std::{fmt::Debug, fs::File, io::Read, path::Path};

use flecs_ecs::macros::Component;
use glam::IVec2;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

/// The configuration for the server representing a `toml` file.
#[derive(Serialize, Deserialize, Debug, Component)]
pub struct Config {
    pub border_diameter: Option<f64>,
    pub max_players: i32,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub server_desc: String,
    pub spawn_chebyshev_radius: Spawn,
}

#[derive(Serialize, Deserialize, Debug, Component)]
pub struct Spawn {
    pub radius: Radius,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
enum Radius {
    Chebyshev(i32),
    Euclidean(i32),
}

impl Radius {
    pub fn get_random_2d(self) -> IVec2 {
        match self {
            Radius::Chebyshev(radius) => {
                let x = fastrand::i32(-radius..radius);
                let z = fastrand::i32(-radius..radius);
                IVec2::new(x, z)
            }
            Radius::Euclidean(radius) => {
                let r = fastrand::f32() * radius as f32;
                let theta = fastrand::f32() * 2.0 * std::f32::consts::PI;

                let x = r * theta.cos();
                let z = r * theta.sin();

                let x = x as i32;
                let z = z as i32;

                IVec2::new(x, z)
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            border_diameter: Some(100.0),
            max_players: 10_000,
            view_distance: 32,
            simulation_distance: 10,
            server_desc: "Hyperion Test Server".to_owned(),
            spawn_chebyshev_radius: 10_000,
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
