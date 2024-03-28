use std::{fs::File, path::Path};

use zip::ZipArchive;

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);

    let generated_zip = root.join("generated.zip");

    let file = File::open(generated_zip).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();

    let registries = archive.by_name("generated/reports/registries.json").unwrap();

    generator_build::GeneratorConfig {
        registries,
        output: None,
    }
    .build()
    .unwrap();
}
