use std::{fs::File, io::BufReader, path::Path};

use anyhow::{bail, Context};
use tar::Archive;
use zstd::stream::read::Decoder;

fn build_from_tar_zst(archive_path: &Path, target_file_path: &Path) -> anyhow::Result<()> {
    // Open the compressed file.
    let tar_zst_file = File::open(archive_path)?;
    let decoder = Decoder::new(BufReader::new(tar_zst_file))?;

    // Create a new archive from the decompressed stream.
    let mut archive = Archive::new(decoder);

    // Iterate over the contents of the archive.
    for file in archive.entries()? {
        let file = file?;

        let path = file.path()?;

        if path == target_file_path {
            let registries = file;

            generator_build::GeneratorConfig {
                registries,
                output: None,
            }
            .build()
            .context("Failed to build generator")?;

            return Ok(());
        }
    }

    bail!("File not found in archive: {:?}", target_file_path);
}

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);

    let generated_archive = root.join("generated.tar.zst");

    build_from_tar_zst(
        &generated_archive,
        Path::new("generated/reports/registries.json"),
    )
    .unwrap();
}
