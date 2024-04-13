use std::path::PathBuf;

use valence_anvil::parsing::DimensionFolder;
use valence_protocol::ChunkPos;
use valence_registry::BiomeRegistry;

pub struct AnvilFolder {
    pub dim: DimensionFolder,
}

fn dot_minecraft_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        let home_dir = dirs_next::home_dir().unwrap();
        return home_dir
            .join("Library")
            .join("Application Support")
            .join("minecraft");
    }

    if cfg!(target_os = "linux") {
        // todo: I do not know if this is correct
        let home_dir = dirs_next::home_dir().unwrap();
        return home_dir
            .join(".minecraft")
            .join("saves")
            .join("World")
            .join("anvil");
    }

    unimplemented!("unimplemented for this OS")
}

fn get_latest_save() -> PathBuf {
    let minecraft = dot_minecraft_path();
    let saves_dir = minecraft.join("saves");

    let mut saves: Vec<_> = saves_dir
        .read_dir()
        .unwrap()
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir()
        })
        .collect();

    saves.sort_unstable_by_key(|entry| entry.metadata().unwrap().modified().unwrap());

    saves.last().unwrap().path()
}

impl AnvilFolder {
    pub fn new() -> Self {
        let latest_save = get_latest_save();
        let registry = BiomeRegistry::default();

        // todo: probs not true
        let mut dim = DimensionFolder::new(latest_save, &registry);

        Self { dim }
    }

}

#[cfg(test)]
mod tests {
    #[test]
    fn test_get_latest_save() {
        let path = super::get_latest_save();
        println!("{path:?}");
    }
}
