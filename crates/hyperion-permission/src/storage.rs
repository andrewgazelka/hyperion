use derive_more::Display;
use enumset::{EnumSet, EnumSetType};
use flecs_ecs::macros::Component;
use heed::{byteorder::NativeEndian, types, Database, Env};
use hyperion::storage::LocalDb;

#[derive(EnumSetType, Display)]
enum Group {
    Banned,
    Spectator,
    Player,
    Moderator,
    Admin,
}

#[derive(Component)]
pub struct PermissionStorage {
    env: Env,
    perms: Database<types::U128<NativeEndian>, types::U8>,
}

impl PermissionStorage {
    pub fn new(db: &LocalDb) -> anyhow::Result<Self> {
        // We open the default unnamed database
        let perms = {
            let mut wtxn = db.write_txn()?;
            let db = db.create_database(&mut wtxn, Some("uuid-to-perms"))?;
            wtxn.commit()?;
            db
        };

        Ok(Self {
            env: (**db).clone(),
            perms,
        })
    }

    pub fn get(&self, uuid: uuid::Uuid) -> EnumSet<Group> {
        let uuid = uuid.as_u128();
        let rtxn = self.env.read_txn().unwrap();
        let Some(perms) = self.perms.get(&rtxn, &uuid).unwrap() else {
            return EnumSet::empty();
        };

        EnumSet::from_u8(perms)
    }

    pub fn add(&self, uuid: uuid::Uuid, group: Group) -> anyhow::Result<()> {
        let uuid = uuid.as_u128();
        let mut wtxn = self.env.write_txn()?;

        let mut groups =
            (self.perms.get(&wtxn, &uuid)?).map_or_else(EnumSet::empty, EnumSet::from_u8);

        groups.insert(group);

        self.perms.put(&mut wtxn, &uuid, &groups.as_u8())?;
        wtxn.commit()?;
        Ok(())
    }

    pub fn set(&self, uuid: uuid::Uuid, groups: EnumSet<Group>) -> anyhow::Result<()> {
        let uuid = uuid.as_u128();
        let mut wtxn = self.env.write_txn()?;
        self.perms.put(&mut wtxn, &uuid, &groups.as_u8())?;
        wtxn.commit()?;
        Ok(())
    }

    pub fn remove(&self, uuid: uuid::Uuid, group: Group) -> anyhow::Result<()> {
        let uuid = uuid.as_u128();
        let mut wtxn = self.env.write_txn()?;

        let mut groups = match self.perms.get(&wtxn, &uuid)? {
            Some(perms) => EnumSet::from_u8(perms),
            None => return Ok(()), // Nothing to remove
        };

        groups.remove(group);

        self.perms.put(&mut wtxn, &uuid, &groups.as_u8())?;
        wtxn.commit()?;
        Ok(())
    }
}
