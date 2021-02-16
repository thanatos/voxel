use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use serde::Deserialize;

use super::block_defs::BlockDefinition;

/// A "mod".
///
/// The type is referred to as `Module` and instances as `module`, as `mod` is a reserved keyword
/// in Rust.
#[derive(Debug)]
pub struct Module {
    name: String,
    path: PathBuf,
    block_defs: HashMap<String, Arc<ModuleBlockDefinition>>,
}

impl Module {
    /// Creates a new `Module`, representing a mod.
    pub fn new(
        name: String,
        path: PathBuf,
        block_defs: HashMap<String, BlockDefinition>,
    ) -> Module {
        let block_defs = map_block_defs(block_defs);
        Module {
            name,
            path,
            block_defs,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn load_from_path(path: PathBuf) -> anyhow::Result<Self> {
        let module_yaml: ModuleYaml = {
            let module_path = path.join("module.yaml");
            let module_file = File::open(&module_path).with_context(|| {
                format!(
                    "failed to open module definition YAML at {}",
                    module_path.display()
                )
            })?;
            serde_yaml::from_reader(module_file).with_context(|| {
                format!(
                    "failed to parse module definition YAML at {}",
                    module_path.display()
                )
            })?
        };

        let block_defs = {
            let block_defs_path = path.join("block-definitions.yaml");
            let block_defs_file = File::open(&block_defs_path).with_context(|| {
                format!(
                    "failed to open block definitions YAML at {}",
                    block_defs_path.display()
                )
            })?;
            super::block_defs::load_block_definitions(block_defs_file)
                .with_context(|| {
                    format!(
                        "failed to parse block definition YAML at {}",
                        block_defs_path.display()
                    )
                })?
        };

        Ok(Module::new(module_yaml.name, path, block_defs))
    }
}

#[derive(Deserialize)]
struct ModuleYaml {
    name: String,
}

fn map_block_defs(
    defs: HashMap<String, BlockDefinition>,
) -> HashMap<String, Arc<ModuleBlockDefinition>> {
    defs.into_iter()
        .map(|(id, def)| {
            let def = ModuleBlockDefinition {
                id: id.clone(),
                def,
            };
            (id, Arc::new(def))
        })
        .collect::<HashMap<_, _>>()
}

#[derive(Debug)]
pub struct ModuleBlockDefinition {
    /// The ID for blocks of this definition
    id: String,
    /// The actual block definition:
    def: BlockDefinition,
}

impl ModuleBlockDefinition {
    #[inline]
    pub fn definition(&self) -> &BlockDefinition {
        &self.def
    }
}
