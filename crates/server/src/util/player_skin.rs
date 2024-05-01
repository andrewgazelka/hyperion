use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::util::mojang::MojangClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSkin {
    pub textures: String,
    pub signature: String,
}

impl PlayerSkin {
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
    ) -> anyhow::Result<Option<Self>> {
        let json_object = mojang.response_from_uuid(&uuid).await?;
        let properties_array = json_object["properties"]
            .as_array()
            .context("no properties")?;

        for property_object in properties_array {
            let name = property_object["name"].as_str().context("no name")?;
            if name != "textures" {
                continue;
            }
            let texture_value = property_object["value"].as_str().context("no value")?;
            let signature_value = property_object["signature"]
                .as_str()
                .context("no signature")?;

            return Ok(Some(Self {
                textures: texture_value.to_string(),
                signature: signature_value.to_string(),
            }));
        }

        Ok(None)
    }

    /// Gets a skin from a Minecraft username.
    ///
    /// # Arguments
    /// * `username` - The Minecraft username.
    ///
    /// # Returns
    /// A `PlayerSkin` based on a Minecraft username, or `None` if not found.
    pub async fn from_username(
        username: &str,
        mojang: &MojangClient,
    ) -> anyhow::Result<Option<Self>> {
        let json_object = mojang.response_from_username(username).await?;
        let uuid = json_object["id"].as_str().context("no id")?;
        let uuid = uuid::Uuid::parse_str(uuid).context("invalid uuid")?;
        Self::from_uuid(uuid, mojang).await
    }
}
