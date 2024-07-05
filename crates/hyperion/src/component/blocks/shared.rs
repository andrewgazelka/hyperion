use std::collections::BTreeMap;

use anyhow::Context;
use valence_protocol::Ident;
use valence_registry::{biome::BiomeId, BiomeRegistry};

use crate::component::blocks::manager::RegionManager;

/// Inner state of the [`MinecraftWorld`] component.
pub struct Shared {
    pub regions: RegionManager,
    pub biome_to_id: BTreeMap<Ident<String>, BiomeId>,
}

impl Shared {
    pub(crate) fn new(biomes: &BiomeRegistry) -> anyhow::Result<Self> {
        let regions = RegionManager::new().context("failed to get anvil data")?;

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
