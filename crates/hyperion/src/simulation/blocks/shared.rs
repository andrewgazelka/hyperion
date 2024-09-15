use std::collections::BTreeMap;

use anyhow::Context;
use tokio::runtime::Runtime;
use valence_protocol::Ident;
use valence_registry::{biome::BiomeId, BiomeRegistry};

use super::manager::RegionManager;

/// Inner state of the [`MinecraftWorld`] component.
pub struct Shared {
    pub regions: RegionManager,
    pub biome_to_id: BTreeMap<Ident<String>, BiomeId>,
}

impl Shared {
    pub(crate) fn new(biomes: &BiomeRegistry, runtime: &Runtime) -> anyhow::Result<Self> {
        let regions = RegionManager::new(runtime).context("failed to get anvil data")?;

        let biome_to_id = biomes
            .iter()
            .map(|(id, name, _)| (name.to_string_ident(), id))
            .collect();

        Ok(Self {
            regions,
            biome_to_id,
        })
    }
}
