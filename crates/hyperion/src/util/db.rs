//! Constructs for connecting and working with a `PostgreSQL` database.

use derive_more::Deref;
use flecs_ecs::macros::Component;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tracing::info;
use uuid::Uuid;

use crate::util::player_skin::PlayerSkin;

/// The database row for player skins
#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct SkinRow {
    /// The UUID of the player.
    pub uuid: Uuid,
    /// The skin of the player.
    pub skin: serde_json::Value, // Storing PlayerSkin as JSON
}

/// A wrapper around a `PostgreSQL` connection pool
#[derive(Component, Debug, Clone, Deref)]
pub struct Db {
    inner: Pool<Postgres>,
}

impl Db {
    /// Creates a new [`Db`] with the given connection string.
    ///
    /// ```no_run
    /// use hyperion::util::db::Db;
    ///
    /// #[tokio::main]
    /// async fn main() -> anyhow::Result<()> {
    ///     let db = Db::new("postgres://username:password@localhost/hyperion").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn new(connection_str: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(connection_str)
            .await?;

        // Initialize the database
        sqlx::query!(
            // language=postgresql
            r#"
            CREATE TABLE IF NOT EXISTS player_skins (
                uuid UUID PRIMARY KEY,
                skin JSONB NOT NULL
            )
            "#
        )
        .execute(&pool)
        .await?;

        Ok(Self { inner: pool })
    }
}

/// A handler for player skin operations
#[derive(Component, Debug, Clone)]
pub struct SkinHandler {
    db: Db,
}

impl SkinHandler {
    /// Creates a new [`SkinHandler`] from a given [`Db`].
    #[must_use]
    pub const fn new(db: Db) -> Self {
        Self { db }
    }

    /// Finds a [`PlayerSkin`] by its UUID.
    pub async fn find(&self, uuid: Uuid) -> anyhow::Result<Option<PlayerSkin>> {
        let result = sqlx::query_as!(
            SkinRow,
            // language=postgresql
            r#"SELECT uuid, skin as "skin: serde_json::Value" FROM player_skins WHERE uuid = $1"#,
            uuid
        )
        .fetch_optional(&*self.db)
        .await?;

        Ok(result.map(|row| serde_json::from_value(row.skin).unwrap()))
    }

    /// Inserts a [`PlayerSkin`] into the database.
    pub async fn insert(&self, uuid: Uuid, skin: PlayerSkin) -> anyhow::Result<()> {
        info!("inserting skin for {uuid}");

        sqlx::query!(
            // language=postgresql
            r#"
            INSERT INTO player_skins (uuid, skin)
            VALUES ($1, $2)
            ON CONFLICT (uuid) DO UPDATE SET skin = EXCLUDED.skin
            "#,
            uuid,
            serde_json::to_value(skin)? as _
        )
        .execute(&*self.db)
        .await?;

        Ok(())
    }
}
