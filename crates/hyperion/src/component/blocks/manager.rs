use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use glam::IVec2;
use tokio::{
    fs::File,
    runtime::Runtime,
    sync::{mpsc, oneshot},
};

use crate::{blocks::get_nyc_save, component::blocks::region::Region};

enum RegionRequest {
    Get {
        coord: IVec2,
        response: oneshot::Sender<std::io::Result<Arc<Region>>>,
    },
}

pub struct RegionManager {
    root: PathBuf,
    sender: mpsc::Sender<RegionRequest>,
}

impl RegionManager {
    pub fn new(runtime: &Runtime) -> anyhow::Result<Self> {
        let save = get_nyc_save()?;
        let root = save.join("region");
        let (sender, receiver) = mpsc::channel(100);

        runtime.spawn(RegionManagerTask::new(root.clone(), receiver).run());

        Ok(Self { root, sender })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn get_region_from_chunk(
        &self,
        pos_x: i32,
        pos_z: i32,
    ) -> std::io::Result<Arc<Region>> {
        let region_x = pos_x.div_euclid(32);
        let region_z = pos_z.div_euclid(32);
        let coord = IVec2::new(region_x, region_z);

        let (response_tx, response_rx) = oneshot::channel();
        self.sender
            .send(RegionRequest::Get {
                coord,
                response: response_tx,
            })
            .await
            .expect("RegionManagerTask has been dropped");

        response_rx
            .await
            .expect("RegionManagerTask has been dropped")
    }
}

struct RegionManagerTask {
    root: PathBuf,
    receiver: mpsc::Receiver<RegionRequest>,
    regions: HashMap<IVec2, Arc<Region>>,
}

impl RegionManagerTask {
    fn new(root: PathBuf, receiver: mpsc::Receiver<RegionRequest>) -> Self {
        Self {
            root,
            receiver,
            regions: HashMap::new(),
        }
    }

    fn region_path(&self, pos_x: i32, pos_z: i32) -> PathBuf {
        self.root.join(format!("r.{pos_x}.{pos_z}.mca"))
    }

    async fn region_file(&self, pos_x: i32, pos_z: i32) -> std::io::Result<File> {
        File::open(self.region_path(pos_x, pos_z)).await
    }

    async fn run(mut self) {
        while let Some(request) = self.receiver.recv().await {
            self.handle_request(request).await;
        }
    }

    async fn handle_request(&mut self, request: RegionRequest) {
        match request {
            RegionRequest::Get { coord, response } => {
                let region = self.get_or_create_region(coord).await;
                // todo: what should we  do here
                drop(response.send(region));
            }
        }
    }

    async fn get_or_create_region(&mut self, coord: IVec2) -> std::io::Result<Arc<Region>> {
        if let Some(region) = self.regions.get(&coord) {
            Ok(region.clone())
        } else {
            self.create_and_insert_region(coord).await
        }
    }

    async fn create_and_insert_region(&mut self, coord: IVec2) -> std::io::Result<Arc<Region>> {
        let file = self.region_file(coord.x, coord.y).await?;
        let region = Region::open(file).unwrap();
        let region = Arc::new(region);
        self.regions.insert(coord, region.clone());
        Ok(region)
    }
}
