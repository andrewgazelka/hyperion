mod schema;

use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context};
use heck::ToUpperCamelCase;
use itertools::Itertools;
use quote::quote;

use crate::schema::MinecraftData;

// java -DbundlerMainClass=net.minecraft.data.Main -jar minecraft_server.jar

pub struct GeneratorConfig {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
}

impl GeneratorConfig {
    #[must_use]
    pub const fn new(input: PathBuf) -> Self {
        Self {
            input,
            output: None,
        }
    }

    pub fn build(&self) -> anyhow::Result<()> {
        let generated = self.generate()?;

        let output = match &self.output {
            Some(path) => path.clone(),
            None => Path::new(&env::var("OUT_DIR")?).join("generator-output.rs"),
        };

        let mut file = File::create(output)?;
        file.write_all(generated.as_bytes())?;

        Ok(())
    }

    fn generate(&self) -> anyhow::Result<String> {
        // ensure input exists
        ensure!(
            self.input.exists(),
            "input path {:?} does not exist",
            self.input.display()
        );

        let reports = self.input.join("reports");
        ensure!(reports.exists(), "reports directory does not exist");

        let regitry = reports.join("registries.json");
        ensure!(regitry.exists(), "registries.json does not exist");

        let s = std::fs::read_to_string(regitry)?;

        let data: MinecraftData = serde_json::from_str(&s)?;

        let entity = data.entity_type.entries;
        let entity: Vec<_> = entity
            .into_iter()
            .map(|(name, id)| {
                // remove minecraft: prefix
                let name = name
                    .split(':')
                    .last()
                    .context("missing minecraft: prefix")?;
                let id = id.protocol_id;

                // use heck to turn into UpperCamelCase
                let name = name.to_upper_camel_case();

                // turn name into ident
                let name = syn::Ident::new(&name, proc_macro2::Span::call_site());

                anyhow::Ok((name, id))
            })
            .try_collect()?;

        let (names, ids): (Vec<_>, Vec<_>) = entity.into_iter().unzip();

        let result = quote! {
            #[repr(i32)]
            #[non_exhaustive]
            pub enum EntityType {
              #( #names = #ids, )*
            }
        };

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
