//! Constructs for obtaining a player's skin.

use anyhow::Context;
use flecs_ecs::macros::Component;
use serde::{Deserialize, Serialize};

use crate::util::{db::SkinCollection, mojang::MojangClient};

/// A signed player skin.
#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct PlayerSkin {
    /// The textures of the player skin, usually obtained from the [`MojangClient`] as a base64 string.
    pub textures: bson::Binary,
    /// The signature of the player skin, usually obtained from the [`MojangClient`] as a base64 string.
    pub signature: bson::Binary,
}

impl PlayerSkin {
    /// Creates a new [`PlayerSkin`]
    #[must_use]
    pub const fn new(textures: bson::Binary, signature: bson::Binary) -> Self {
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
        skins: &SkinCollection,
    ) -> anyhow::Result<Option<Self>> {
        if let Some(skin) = skins.find(uuid).await? {
            return Ok(Some(skin));
        }

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
            let signature_value = property_object["signature"]
                .as_str()
                .with_context(|| format!("no signature on {property_object:?}"))?;

            let textures =
                bson::Binary::from_base64(textures, None).context("invalid texture value")?;
            let signature = bson::Binary::from_base64(signature_value, None)
                .context("invalid signature value")?;

            let res = Self {
                textures,
                signature,
            };

            skins.insert(uuid, res.clone()).await?;

            return Ok(Some(res));
        }

        Ok(None)
    }

    // /// Gets a skin from a Minecraft username.
    // ///
    // /// # Arguments
    // /// * `username` - The Minecraft username.
    // ///
    // /// # Returns
    // /// A `PlayerSkin` based on a Minecraft username, or `None` if not found.
    // pub async fn from_username(
    //     username: &str,
    //     mojang: &MojangClient,
    // ) -> anyhow::Result<Option<Self>> {
    //     let json_object = mojang.response_from_username(username).await?;
    //     let uuid = json_object["id"].as_str().context("no id")?;
    //     let uuid = uuid::Uuid::parse_str(uuid).context("invalid uuid")?;
    //     Self::from_uuid(uuid, mojang).await
    // }
}
