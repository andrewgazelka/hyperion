mod schema;

use std::{
    env,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;

use crate::schema::MinecraftData;

// java -DbundlerMainClass=net.minecraft.data.Main -jar minecraft_server.jar

pub struct GeneratorConfig<T> {
    pub registries: T,
    pub output: Option<PathBuf>,
}

impl<T: Read> GeneratorConfig<T> {
    #[must_use]
    pub const fn new(registries: T) -> Self {
        Self {
            registries,
            output: None,
        }
    }

    pub fn build(self) -> anyhow::Result<()> {
        let output = match &self.output {
            Some(path) => path.clone(),
            None => Path::new(&env::var("OUT_DIR")?).join("generator-output.rs"),
        };

        let generated = self.generate()?;

        let mut file = File::create(output)?;
        file.write_all(generated.as_bytes())?;

        Ok(())
    }

    fn generate(self) -> anyhow::Result<String> {
        let data: MinecraftData = serde_json::from_reader(self.registries)?;

        let result = data.entity_type.to_token_stream("EntityType")?;

        let item = syn::parse2(result).context("failed to parse generated code")?;

        // https://stackoverflow.com/a/72382348
        let file = syn::File {
            attrs: vec![],
            items: vec![item],
            shebang: None,
        };

        let result = prettyplease::unparse(&file);

        Ok(result)
    }
}
