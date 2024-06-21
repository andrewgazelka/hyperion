use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

use glam::IVec2;
use tokio::{
    fs::File,
    sync::{Notify, RwLock},
};

use crate::{blocks::get_nyc_save, component::blocks::region::Region};

enum RegionState {
    Pending(Weak<Notify>),
    Loaded(Arc<tokio::sync::Mutex<Region>>),
}

pub struct Regions {
    root: PathBuf,
    regions: RwLock<HashMap<IVec2, RegionState>>,
}

impl Regions {
    pub fn new() -> anyhow::Result<Self> {
        let save = get_nyc_save()?;

        Ok(Self {
            root: save.join("region"),
            regions: RwLock::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn region_path(&self, pos_x: i32, pos_z: i32) -> PathBuf {
        self.root.join(format!("r.{pos_x}.{pos_z}.mca"))
    }

    // #[instrument(skip_all, level = "trace")]
    async fn region_file(&self, pos_x: i32, pos_z: i32) -> File {
        File::open(self.region_path(pos_x, pos_z)).await.unwrap()
    }

    // #[instrument(skip_all, level = "trace")]
    pub async fn get_region_from_chunk(
        &self,
        pos_x: i32,
        pos_z: i32,
    ) -> Arc<tokio::sync::Mutex<Region>> {
        let region_x = pos_x.div_euclid(32);
        let region_z = pos_z.div_euclid(32);

        let coord = IVec2::new(region_x, region_z);

        let mut write = self.regions.write().await;

        if let Some(value) = write.get(&coord) {
            return match value {
                RegionState::Pending(notifier) => {
                    let notifier = notifier.clone();
                    drop(write);

                    notifier.upgrade().unwrap().notified().await;

                    let read = self.regions.read().await;
                    let value = read.get(&coord).unwrap();

                    match value {
                        RegionState::Pending(_) => unreachable!(),
                        RegionState::Loaded(loaded) => {
                            let res = loaded.clone();
                            drop(read);
                            res
                        }
                    }
                }
                RegionState::Loaded(loaded) => loaded.clone(),
            };
        }

        // insert
        let pending = Arc::new(Notify::new());
        {
            let pending = Arc::downgrade(&pending);
            write.insert(coord, RegionState::Pending(pending));
        }

        drop(write);

        let file = self.region_file(region_x, region_z).await;
        let region = Box::pin(Region::open(file)).await.unwrap();

        let mut write = self.regions.write().await;
        let region = Arc::new(tokio::sync::Mutex::new(region));
        let prev = write
            .insert(coord, RegionState::Loaded(region.clone()))
            .unwrap();
        drop(write);

        let RegionState::Pending(notifier) = prev else {
            unreachable!();
        };

        notifier.upgrade().unwrap().notify_waiters();

        region
    }
}
