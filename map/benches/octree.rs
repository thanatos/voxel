#[macro_use]
extern crate criterion;

use criterion::Criterion;

use voxel_map::octree::{BlockOctree, LocationCode, SubCube};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TestBlock(u16);

#[derive(Clone)]
struct BlockDefs;

impl voxel_map::octree::BlockInfo<TestBlock> for BlockDefs {
    fn is_homogeneous(&self, block: &TestBlock) -> bool {
        true
    }
}

fn bench_octree_replace_volume(c: &mut Criterion) {
    c.bench_function("octree clear volume", move |b| {
        let mut tree: BlockOctree<TestBlock, _> = BlockOctree::new(BlockDefs);

        let sub_area = LocationCode::ROOT.push_sub_cube(SubCube::LowerNe);
        tree.set_volume(sub_area, TestBlock(2));
        b.iter_batched(
            || {
                tree.clone()
            },
            |mut tree| {
                tree.set_volume(LocationCode::ROOT, TestBlock(1));
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, bench_octree_replace_volume);
criterion_main!(benches);
