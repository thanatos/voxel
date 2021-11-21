use std::fmt;
use std::sync::Arc;

use voxel_mod::ModuleBlockDefinition;

pub mod io;
/// An octree implementation for space-efficient map data.
pub mod octree;
pub mod region;

use octree::{LocationCode, SubCube};

#[derive(Clone)]
struct OctreeBlock(Option<Arc<ModuleBlockDefinition>>);

impl PartialEq for OctreeBlock {
    fn eq(&self, other: &OctreeBlock) -> bool {
        match (&self.0, &other.0) {
            (None, None) => true,
            (Some(s), Some(o)) => Arc::ptr_eq(s, o),
            _ => false,
        }
    }
}

impl fmt::Debug for OctreeBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(b) => {
                write!(f, "OctreeBlock(Some(")?;
                b.fmt(f)?;
                write!(f, ")),")
            }
            None => write!(f, "OctreeBlock(None)"),
        }
    }
}

impl Eq for OctreeBlock {}

#[derive(Debug)]
struct BlockInfo;

impl octree::BlockInfo<OctreeBlock> for BlockInfo {
    fn is_homogeneous(&self, block: &OctreeBlock) -> bool {
        match &block.0 {
            Some(block) => block.definition().is_homogeneous(),
            None => true,
        }
    }
}

/// A 3D cube representing a subsection of the world.
///
/// Chunks are 64×64×64 cubes of blocks. (See [`CHUNK_SIDE_LENGTH`].)
pub struct Chunk {
    octree: octree::BlockOctree<OctreeBlock, BlockInfo>,
}

/// A coordinate within a chunk, that is, relative to the chunk.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ChunkRelativeCoord {
    x: u8,
    y: u8,
    z: u8,
}

/// The length of the side of a chunk.
pub const CHUNK_SIDE_LENGTH: u8 = 64;

impl ChunkRelativeCoord {
    pub fn new(x: u8, y: u8, z: u8) -> ChunkRelativeCoord {
        if x < CHUNK_SIDE_LENGTH && y < CHUNK_SIDE_LENGTH && z < CHUNK_SIDE_LENGTH {
            ChunkRelativeCoord { x, y, z }
        } else {
            panic!("({}, {}, {}) is not a valid chunk coordinate", x, y, z);
        }
    }

    fn to_location_code(&self) -> LocationCode {
        let mut code = LocationCode::ROOT;

        // A chunk is 64x64x64; 64 is 2**6 so we need to do this 6 times:
        for idx in 0..6 {
            let shift = 6 + 1 - idx;
            let xbit = self.x >> shift;
            let ybit = self.y >> shift;
            let zbit = self.z >> shift;
            let sub_cube = SubCube::from_xyz(xbit, ybit, zbit).unwrap();
            code = code.push_sub_cube(sub_cube);
        }
        code
    }
}

impl Chunk {
    pub fn new() -> Chunk {
        Chunk {
            octree: octree::BlockOctree::with_block(BlockInfo, OctreeBlock(None)),
        }
    }

    pub fn set_block(
        &mut self,
        chunk_coord: ChunkRelativeCoord,
        block: Option<Arc<ModuleBlockDefinition>>,
    ) {
        let location_code = chunk_coord.to_location_code();
        self.octree.set_volume(location_code, OctreeBlock(block));
    }

    pub fn blocks(
        &self,
    ) -> impl Iterator<Item = (LocationCode, &Option<Arc<ModuleBlockDefinition>>)> {
        self.octree.iter().map(|(k, v)| (k, &v.0))
    }

    pub(crate) fn get_octree(&self) -> &octree::BlockOctree<OctreeBlock, BlockInfo> {
        &self.octree
    }
}
