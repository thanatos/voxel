use std::sync::Arc;

use voxel_mod::ModuleBlockDefinition;

pub mod io;
/// An octree implementation for space-efficient map data.
pub mod octree;

#[derive(Clone)]
struct OctreeBlock(Arc<ModuleBlockDefinition>);

impl PartialEq for OctreeBlock {
    fn eq(&self, other: &OctreeBlock) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for OctreeBlock {}

struct BlockInfo;

impl octree::BlockInfo<OctreeBlock> for BlockInfo {
    fn is_homogeneous(&self, block: &OctreeBlock) -> bool {
        block.0.definition().is_homogeneous()
    }
}

pub struct Chunk {
    octree: octree::BlockOctree<OctreeBlock, BlockInfo>,
}

impl Chunk {
    pub fn with_block(root_block: Arc<ModuleBlockDefinition>) -> Chunk {
        Chunk {
            octree: octree::BlockOctree::with_block(BlockInfo, OctreeBlock(root_block))
        }
    }
}

impl Chunk {
    pub fn blocks(&self) -> impl Iterator<Item = (octree::LocationCode, &Arc<ModuleBlockDefinition>)> {
        self.octree.iter().map(|(k, v)| (k, &v.0))
    }
}
