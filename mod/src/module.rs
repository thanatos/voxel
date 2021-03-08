use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, Weak};

use anyhow::Context;
use serde::Deserialize;

use super::block_defs::BlockDefinition;

/// A "mod".
///
/// The type is referred to as `Module` and instances as `module`, as `mod` is a reserved keyword
/// in Rust.
#[derive(Debug)]
pub struct Module {
    id: String,
    display_name: String,
    path: PathBuf,
    block_defs: RwLock<HashMap<String, Arc<ModuleBlockDefinition>>>,
}

impl Module {
    /// Creates a new `Module`, representing a mod.
    pub fn new(
        id: String,
        display_name: String,
        path: PathBuf,
        block_defs: HashMap<String, BlockDefinition>,
    ) -> Arc<Module> {
        let module = Arc::new(Module {
            id,
            display_name,
            path,
            block_defs: RwLock::new(HashMap::new()),
        });
        let block_defs = map_block_defs(block_defs, Arc::downgrade(&module));
        module.block_defs.write().unwrap().extend(block_defs);
        module
    }

    pub fn block_by_id(&self, id: &str) -> Option<Arc<ModuleBlockDefinition>> {
        let lock = self.block_defs.read().unwrap();
        lock.get(id).cloned()
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn load_from_path(path: PathBuf) -> anyhow::Result<Arc<Module>> {
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

        Ok(Module::new(module_yaml.id, module_yaml.name, path, block_defs))
    }
}

#[derive(Deserialize)]
struct ModuleYaml {
    id: String,
    name: String,
}

fn map_block_defs(
    defs: HashMap<String, BlockDefinition>,
    weak_module: Weak<Module>,
) -> impl Iterator<Item = (String, Arc<ModuleBlockDefinition>)> {
    defs.into_iter()
        .map(move |(id, def)| {
            let def = ModuleBlockDefinition {
                // empty ref, initialized later when we have an Arc<Module>.
                module: weak_module.clone(),
                id: id.clone(),
                def,
            };
            (id, Arc::new(def))
        })
}

#[derive(Debug)]
pub struct ModuleBlockDefinition {
    /// A reference back to the owning module.
    module: Weak<Module>,
    /// The ID for blocks of this definition
    id: String,
    /// The actual block definition:
    def: BlockDefinition,
}

impl ModuleBlockDefinition {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn module(&self) -> Arc<Module> {
        self.module.upgrade().unwrap()
    }

    #[inline]
    pub fn definition(&self) -> &BlockDefinition {
        &self.def
    }
}
