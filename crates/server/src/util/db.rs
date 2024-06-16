//! Constructs for connecting and working with a `MongoDB` database.

use bson::doc;
use derive_more::Deref;
use flecs_ecs::macros::Component;
use mongodb::{options::ClientOptions, IndexModel};
use serde::{Deserialize, Serialize};

use crate::{
    component::{Inventory, Pose},
    util::player_skin::PlayerSkin,
};

#[derive(Serialize, Deserialize, Debug)]
struct CorePlayer {
    uuid: bson::Uuid,
    name_on_login: String,
    health: f32,
    pose: Pose,
    last_login: chrono::DateTime<chrono::Utc>,
    inventory: Inventory,
}

/// The document for [`SkinCollection`]
#[derive(Serialize, Deserialize, Debug)]
pub struct SkinDocument {
    /// The UUID of the player.
    pub uuid: bson::Uuid,
    /// The skin of the player.
    pub skin: PlayerSkin,
}

/// A wrapper around a [`mongodb::Database`]
#[derive(Component, Debug, Clone, Deref)]
pub struct Db {
    inner: mongodb::Database,
}

impl Db {
    /// Creates a new [`Db`] with the given connection string.
    ///
    /// ```no_run
    /// use hyperion::util::db::Db;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let db = Db::new("mongodb://localhost:27017").await?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Also see crate level documention [here](crate::util::db).
    pub async fn new(s: impl AsRef<str>) -> anyhow::Result<Self> {
        let mut client_options = ClientOptions::parse(s).await?;

        // Manually set an option.
        client_options.app_name = Some("Hyperion".to_string());

        // Get a handle to the deployment.
        let client = mongodb::Client::with_options(client_options)?;

        let db = client.database("hyperion");

        Ok(Self { inner: db })
    }
}

/// A collection of [`SkinDocument`]s.
#[derive(Component, Debug, Clone, Deref)]
pub struct SkinCollection {
    inner: mongodb::Collection<SkinDocument>,
}

impl SkinCollection {
    /// Creates a new [`SkinCollection`] from a given [`Db`].
    pub async fn new(db: &Db) -> anyhow::Result<Self> {
        let skins = db.collection::<SkinDocument>("skins");

        let index_model = IndexModel::builder()
            .keys(doc! {"uuid": 1})
            .options(None)
            .build();

        skins.create_index(index_model, None).await?;

        Ok(Self { inner: skins })
    }

    /// Finds a [`PlayerSkin`] by its UUID.
    pub async fn find(&self, uuid: uuid::Uuid) -> anyhow::Result<Option<PlayerSkin>> {
        let uuid = bson::Uuid::from_uuid_1(uuid);
        let result = self.find_one(doc! {"uuid": uuid}, None).await?;
        Ok(result.map(|x| x.skin))
    }

    /// Inserts a [`PlayerSkin`] into the database.
    pub async fn insert(&self, uuid: uuid::Uuid, skin: PlayerSkin) -> anyhow::Result<()> {
        let uuid = bson::Uuid::from_uuid_1(uuid);

        let skin = SkinDocument { uuid, skin };

        self.insert_one(skin, None).await?;

        Ok(())
    }
}
