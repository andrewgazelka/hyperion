//! Constructs for connecting and working with a `Heed` database.

use std::path::Path;

use byteorder::NativeEndian;
use flecs_ecs::macros::Component;
use heed::{types, Database, Env, EnvOpenOptions};
use uuid::Uuid;

use crate::simulation::skin::PlayerSkin;

/// A wrapper around a `Heed` database
#[derive(Component, Debug, Clone)]
pub struct Db {
    env: Env,

    // mapping of UUID to Skin
    skins: Database<types::U128<NativeEndian>, types::SerdeBincode<PlayerSkin>>,
}

impl Db {
    /// Creates a new [`Db`]
    pub fn new() -> anyhow::Result<Self> {
        let path = Path::new("db").join("heed.mdb");

        std::fs::create_dir_all(&path)?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(10 * 1024 * 1024) // 10MB
                .max_dbs(1)
                .open(&path)?
        };
        // We open the default unnamed database
        let inner = {
            let mut wtxn = env.write_txn()?;
            let db = env.create_database(&mut wtxn, Some("uuid-to-skins"))?;
            wtxn.commit()?;
            db
        };

        Ok(Self { skins: inner, env })
    }
}

/// A handler for player skin operations
#[derive(Component, Debug, Clone)]
pub struct SkinHandler {
    something: u32,
    // db: Db,
}

impl SkinHandler {
    // /// Creates a new [`SkinHandler`] from a given [`Db`].
    // #[must_use]
    // pub const fn new(db: Db) -> Self {
    //     Self { db }
    // }
    pub const fn new() -> Self {
        Self {
            something: 0,
        }
    }

    /// Finds a [`PlayerSkin`] by its UUID.
    pub fn find(&self, uuid: Uuid) -> anyhow::Result<Option<PlayerSkin>> {
        Ok(None)
        // // We open a read transaction to check if those values are now available
        // 
        // let uuid = uuid.as_u128();
        // 
        // let rtxn = self.db.env.read_txn()?;
        // let skin = self.db.skins.get(&rtxn, &uuid);
        // 
        // let Some(skin) = skin? else {
        //     return Ok(None);
        // };
        // 
        // Ok(Some(skin))
    }

    /// Inserts a [`PlayerSkin`] into the database.
    pub fn insert(&self, uuid: Uuid, skin: PlayerSkin) -> anyhow::Result<()> {
        // let uuid = uuid.as_u128();
        // 
        // let mut wtxn = self.db.env.write_txn()?;
        // self.db.skins.put(&mut wtxn, &uuid, &skin)?;
        // wtxn.commit()?;

        Ok(())
    }
}
