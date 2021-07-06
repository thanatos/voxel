use std::collections::HashMap;

mod location_code;

pub use location_code::{LocationCode, SubCube};

/// A node in a block octree. Either subdivided into 8, or present with the block data.
#[derive(Debug)]
pub enum OctreeNode<T> {
    /// This node is present; the given value is there.
    Present(T),
    /// This node in the octree is subdivided into smaller nodes.
    Subdivided,
}

impl<T: Clone> Clone for OctreeNode<T> {
    fn clone(&self) -> Self {
        match self {
            OctreeNode::Present(t) => OctreeNode::Present(t.clone()),
            OctreeNode::Subdivided => OctreeNode::Subdivided,
        }
    }
}

/// A struct containing information about blocks in the octree. It can either derive this info from
/// the block itself, or from some sort of list of definitions, e.g., if many blocks share the same
/// info.
pub trait BlockInfo<T> {
    /// Does this block (/material) allow itself to be free joined/split by the octree?
    ///
    /// For example: "stone" is the same, and it doesn't matter if an octree joins or splits it.
    /// However, some blocks are complex things (e.g., a machine block) and shouldn't be
    /// joined/split by the tree.
    fn is_homogeneous(&self, block: &T) -> bool;
}

/// An octree containing blocks.
///
/// This octree always has some `T` occupying the entire volume. Setting a volume might cause `T`
/// to get subdivided, so `T: Copy`. Setting a volume might also cause volumes to merge (it the two
/// volumes are the "same" block). `T: OctreeBlock`, which is a trait the octree uses to know when
/// it can and cannot merge or split volumes.
#[derive(Debug)]
pub struct BlockOctree<T, BI> {
    octree: HashMap<LocationCode, OctreeNode<T>>,
    block_info: BI,
}

impl<T: Clone, BI: Clone> Clone for BlockOctree<T, BI> {
    fn clone(&self) -> Self {
        BlockOctree {
            octree: self.octree.clone(),
            block_info: self.block_info.clone(),
        }
    }
}

impl<T: Default, BI: BlockInfo<T>> BlockOctree<T, BI> {
    /// Create a new `BlockOctree`, with the volume filled with `T::default()`.
    pub fn new(block_info: BI) -> BlockOctree<T, BI> {
        Self::with_block(block_info, T::default())
    }
}

impl<T, BI: BlockInfo<T>> BlockOctree<T, BI> {
    pub fn with_block(block_info: BI, root_block: T) -> BlockOctree<T, BI> {
        let mut octree = HashMap::new();
        octree.insert(LocationCode::ROOT, OctreeNode::Present(root_block));

        BlockOctree {
            octree,
            block_info,
        }
    }
}

impl<T: Clone + Eq + PartialEq, BI: BlockInfo<T>> BlockOctree<T, BI> {
    /// Iterate through the contents of the tree, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (LocationCode, &T)> {
        self.octree.iter().map(|(k, v)| (*k, v)).filter_map(|(k, v)| match v {
            OctreeNode::Present(vdata) => Some((k, vdata)),
            OctreeNode::Subdivided => None,
        })
    }

    /// Iterate through the octree, depth first, returning intermediate levels even if the level is
    /// subdivided.
    /*
    pub fn depth_first_all_levels(&self) -> impl Iterator<Item = (LocationCode, &OctreeNode<T>)> {
        unimplemented!()
    }
    */

    /// Iterate through the contents of the tree, depth first.
    pub fn depth_first_blocks(&self) -> impl Iterator<Item = (LocationCode, &T)> {
        DepthFirstIterator {
            octree: &self.octree,
            next_location: Some(LocationCode::ROOT),
        }
    }

    pub fn get_volume(&self, volume: LocationCode) -> Option<&OctreeNode<T>> {
        self.octree.get(&volume)
    }

    /// Set a volume of space inside the tree to the given data.
    ///
    /// If there is already something contained in that space, if it is "homogeneous"
    /// (combinable/splittable) then it is split up (or remove) & the volume is replaced. If the
    /// volume is not homogeneous, then setting the volume fails.
    ///
    /// Returns a `bool`, `true` if the given volume could be set, `false` if it could not.
    /// until only the desired volume is replaced. "Set" (`true`) includes setting a volume to a
    /// homogeneous value that is set at a larger volume. (The sub-volume is
    /// instantly/merged/consumed.)
    pub fn set_volume(&mut self, volume: LocationCode, data: T) -> bool {
        let is_homogeneous = self.block_info.is_homogeneous(&data);
        // Start at the root, and work our way towards our target volume. If at any point we see a
        // matching volume & our target data is homogenous, we can abort: the volume is already
        // that block / material.
        //
        // Otherwise, if the volume is subdivided, keep going. If it *isn't* subdivided, divide it
        // and keep going.
        for location_code in volume.from_root_to_just_above_here() {
            let volume_data = self.octree.get(&location_code).unwrap();
            match volume_data {
                OctreeNode::Subdivided => (),
                OctreeNode::Present(vd) => {
                    if is_homogeneous && *vd == data {
                        return true;
                    } else if self.block_info.is_homogeneous(vd) {
                        let new_volume_data = vd.clone();
                        self.subdivide(location_code, new_volume_data);
                    } else {
                        // Non-homogeneous, and it's a different size. We consider those different.
                        return false;
                    }
                }
            }
        }

        match self.octree.get(&volume).unwrap() {
            // There's a whole subtree of blocks here; clear them out & set the target volume.
            OctreeNode::Subdivided => {
                self.clear_subvolume_and_set(volume, data);
            }
            // There's a block here, but it will be replaced.
            OctreeNode::Present(_) => {
                self.octree.insert(volume, OctreeNode::Present(data));
            }
        }
        true
    }

    // Clear a volume from the tree. This leaves a void in the tree, which is an invariant of the
    // tree! You must make sure the void gets filled in after calling this.
    fn clear_subvolume_and_set(&mut self, volume: LocationCode, data: T) {
        // LowerSw is the first sub cube in the chain of siblings that `next_sibling` iterates
        // through.
        let mut current_volume = volume.push_sub_cube(SubCube::LowerSw);
        while current_volume != volume {
            match self.octree.get(&current_volume).unwrap() {
                OctreeNode::Subdivided => {
                    current_volume = current_volume.push_sub_cube(SubCube::LowerSw);
                }
                OctreeNode::Present(_) => {
                    self.octree.remove(&current_volume);

                    while current_volume != volume {
                        let (parent, subcube) = current_volume
                            .sub_cube()
                            // We are always below `volume`, so we always have a parent volume.
                            .unwrap();

                        let next = subcube
                            .next_sibling()
                            .map(|sc| parent.push_sub_cube(sc))
                            .unwrap_or_else(|| parent);

                        current_volume = next;
                    }
                }
            }
        }
        self.octree.insert(volume, OctreeNode::Present(data));
    }

    fn subdivide(&mut self, volume: LocationCode, value: T) {
        for sub_cube in SubCube::all_sub_cubes() {
            let smaller_volume = volume.push_sub_cube(sub_cube);
            self.octree.insert(smaller_volume, OctreeNode::Present(value.clone()));
        }
        self.octree.insert(volume, OctreeNode::Subdivided);
    }
}

struct DepthFirstIterator<'a, T> {
    octree: &'a HashMap<LocationCode, OctreeNode<T>>,
    next_location: Option<LocationCode>,
}

impl<T> DepthFirstIterator<'_, T> {
    fn next_sibling_of(mut location: LocationCode) -> Option<LocationCode> {
        while let Some((parent, sub_cube)) = location.sub_cube() {
            if let Some(sibling) = sub_cube.next_sibling() {
                return Some(parent.push_sub_cube(sibling));
            } else {
                location = parent;
            }
        }
        None
    }
}

impl<'a, T> Iterator for DepthFirstIterator<'a, T> {
    type Item = (LocationCode, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        let mut this_location = match self.next_location {
            Some(loc) => loc,
            None => return None,
        };

        loop {
            match self.octree.get(&this_location) {
                Some(OctreeNode::Present(block)) => {
                    self.next_location = Self::next_sibling_of(this_location);
                    return Some((this_location, block));
                }
                Some(OctreeNode::Subdivided) => {
                    this_location = this_location.push_sub_cube(SubCube::LowerSw);
                }
                None => panic!(),
            };
        }
    }
}

impl<T> std::iter::FusedIterator for DepthFirstIterator<'_, T> {}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::{BlockOctree, LocationCode, SubCube};

    #[derive(Clone, Copy, Default, Eq, PartialEq)]
    struct TestBlock(u16);

    // Forces the Debug `#?` output to a single line, which is just easier to read.
    impl fmt::Debug for TestBlock {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "TestBlock({:?})", self.0)
        }
    }

    #[derive(Debug)]
    struct BlockDefs;

    impl super::BlockInfo<TestBlock> for BlockDefs {
        fn is_homogeneous(&self, _block: &TestBlock) -> bool {
            true
        }
    }

    #[test]
    fn test_octree() {
        let mut tree: BlockOctree<TestBlock, BlockDefs> = BlockOctree::new(BlockDefs);

        println!("Tree before insert: {:#?}", tree);
        let sub_area = LocationCode::ROOT.push_sub_cube(SubCube::LowerNe);
        tree.set_volume(sub_area, TestBlock(2));
        println!("Tree after insert: {:#?}", tree);


        let iter = tree.depth_first_blocks();
        let items = iter.map(|(l, b)| (l, *b)).collect::<Vec<_>>();
        assert!(
            items == &[
                (LocationCode::ROOT.push_sub_cube(SubCube::LowerSw), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::LowerSe), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::LowerNw), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::LowerNe), TestBlock(2)),
                (LocationCode::ROOT.push_sub_cube(SubCube::UpperSw), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::UpperSe), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::UpperNw), TestBlock(0)),
                (LocationCode::ROOT.push_sub_cube(SubCube::UpperNe), TestBlock(0)),
            ]
        );
    }

}
