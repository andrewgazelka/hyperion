use std::{fs::File, io, path::Path};

use zip::ZipArchive;

fn unzip_file_to_location(file_path: &Path, destination: &Path) -> io::Result<()> {
    let file = File::open(file_path)?;
    let mut archive = ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // todo: mangled name good?
        let outpath = destination.join(file.mangled_name());

        if (*file.name()).ends_with('/') {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(p)?;
                }
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }

    Ok(())
}

fn main() {
    // step 1 unzip generated.zip to OUT_DIR
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);

    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&root);

    let generated_zip = root.join("generated.zip");

    unzip_file_to_location(&generated_zip, out_dir).unwrap();

    generator_build::GeneratorConfig {
        input: out_dir.join("generated"),
        output: None,
    }
    .build()
    .unwrap();
}
