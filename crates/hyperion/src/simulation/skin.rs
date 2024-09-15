//! Constructs for obtaining a player's skin.
use anyhow::Context;
use base64::{engine::general_purpose, Engine as _};
use flecs_ecs::macros::Component;
use serde::{Deserialize, Serialize};

use crate::{storage::SkinHandler, util::mojang::MojangClient};

/// A signed player skin.
#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct PlayerSkin {
    /// The textures of the player skin, usually obtained from the [`MojangClient`] as a base64 string.
    pub textures: String,
    /// The signature of the player skin, usually obtained from the [`MojangClient`] as a base64 string.
    pub signature: String,
}

impl PlayerSkin {
    /// Creates a new [`PlayerSkin`]
    #[must_use]
    pub const fn new(textures: String, signature: String) -> Self {
        Self {
            textures,
            signature,
        }
    }

    /// Gets a skin from a Mojang UUID.
    ///
    /// # Arguments
    /// * `uuid` - A Mojang UUID.
    ///
    /// # Returns
    /// A `PlayerSkin` based on the UUID, or `None` if not found.
    pub async fn from_uuid(
        uuid: uuid::Uuid,
        mojang: &MojangClient,
        skins: &SkinHandler,
    ) -> anyhow::Result<Option<Self>> {
        if let Some(skin) = skins.find(uuid)? {
            return Ok(Some(skin));
        }

        println!("cache miss for {uuid}");

        let json_object = mojang.data_from_uuid(&uuid).await?;
        let properties_array = json_object["properties"]
            .as_array()
            .with_context(|| format!("no properties on {json_object:?}"))?;
        for property_object in properties_array {
            let name = property_object["name"]
                .as_str()
                .with_context(|| format!("no name on {property_object:?}"))?;
            if name != "textures" {
                continue;
            }
            let textures = property_object["value"]
                .as_str()
                .with_context(|| format!("no value on {property_object:?}"))?;
            let signature = property_object["signature"]
                .as_str()
                .with_context(|| format!("no signature on {property_object:?}"))?;

            // Validate base64 encoding
            general_purpose::STANDARD
                .decode(textures)
                .context("invalid texture value")?;
            general_purpose::STANDARD
                .decode(signature)
                .context("invalid signature value")?;

            let res = Self {
                textures: textures.to_string(),
                signature: signature.to_string(),
            };
            skins.insert(uuid, res.clone())?;
            return Ok(Some(res));
        }
        Ok(None)
    }
}
