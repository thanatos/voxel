use std::collections::HashMap;
use std::io::Read;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Deserialize)]
pub struct BlockDefinition {
    /// Path to a texture for the block.
    // TODO: all sorts of things. Loading? Multiple textures per block? Texture mapping?
    texture: String,
    /// A primitive color for the block.
    color: Color,
    /// Whether the block is homogeneous, and nearby blocks can be losslessly merged with it.
    homogeneous: bool,
}

impl BlockDefinition {
    #[inline]
    pub fn is_homogeneous(&self) -> bool {
        self.homogeneous
    }
}

/// Load block definitions from a YAML file.
pub fn load_block_definitions<R: Read>(
    reader: R,
) -> Result<HashMap<String, BlockDefinition>, serde_yaml::Error> {
    serde_yaml::from_reader(reader)
}
