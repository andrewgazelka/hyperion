use std::path::PathBuf;

use anyhow::Context;
use valence_anvil::parsing::DimensionFolder;
use valence_registry::BiomeRegistry;

pub struct AnvilFolder {
    pub dim: DimensionFolder,
}

fn dot_minecraft_path() -> anyhow::Result<PathBuf> {
    if cfg!(target_os = "macos") {
        let home_dir = dirs_next::home_dir().context("could not find home directory")?;
        let dir = home_dir
            .join("Library")
            .join("Application Support")
            .join("minecraft");

        return Ok(dir);
    }

    if cfg!(target_os = "linux") {
        // todo: I do not know if this is correct
        let home_dir = dirs_next::home_dir().unwrap();
        let dir = home_dir
            .join(".minecraft")
            .join("saves")
            .join("World")
            .join("anvil");

        return Ok(dir);
    }

    unimplemented!("unimplemented for this OS")
}

fn get_latest_save() -> anyhow::Result<PathBuf> {
    let minecraft = dot_minecraft_path()?;
    let saves_dir = minecraft.join("saves");

    let mut saves: Vec<_> = saves_dir
        .read_dir()
        .context("could not read saves directory")?
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_dir()
        })
        .collect();

    saves.sort_unstable_by_key(|entry| entry.metadata().unwrap().modified().unwrap());

    let path = saves.last().context("no saves found")?.path();

    Ok(path)
}

impl AnvilFolder {
    pub fn new(biomes: &BiomeRegistry) -> anyhow::Result<Self> {
        let latest_save = get_latest_save()?;

        // todo: probs not true
        let dim = DimensionFolder::new(latest_save, biomes);

        Ok(Self { dim })
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
