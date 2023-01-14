//! Utility code for building in-GPU vertex buffers/index buffers for models.

use std::convert::TryFrom;
use std::collections::{hash_map, HashMap};
use std::hash::Hash;
use std::sync::Arc;

use bytemuck::Pod;
use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer, TypedBufferAccess};
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::memory::allocator::MemoryAllocator;

pub struct ModelBuilder<V> {
    vertex_to_index: HashMap<V, usize>,
    vertexes: Vec<V>,
    index_map: Vec<usize>,
}

impl<V: Clone + Eq + Hash> ModelBuilder<V> {
    pub fn new() -> ModelBuilder<V> {
        ModelBuilder {
            vertex_to_index: HashMap::new(),
            vertexes: Vec::new(),
            index_map: Vec::new(),
        }
    }

    pub fn push_vertex(&mut self, vertex: V) {
        let index = match self.vertex_to_index.entry(vertex.clone()) {
            hash_map::Entry::Occupied(occ) => *occ.get(),
            hash_map::Entry::Vacant(vacancy) => {
                let next_index = self.vertexes.len();
                self.vertexes.push(vertex);
                vacancy.insert(next_index);
                next_index
            }
        };
        self.index_map.push(index);
    }

    pub fn into_gpu<F, U: Pod + Send + Sync + 'static>(self, memory_allocator: &(impl MemoryAllocator + ?Sized), vertex_map: F, u8_ext: bool) -> (Arc<CpuAccessibleBuffer<[U]>>, IndexBuffer) where F: Fn(V) -> U {
        // TODO: use DeviceLocalBuffer, maybe ImmutableBuffer.
        // (This TODO was from an old version of Vulkano, 0.30.0 or earlier. Does it still apply?)
        let vertex_buffer = CpuAccessibleBuffer::from_iter(
            memory_allocator,
            BufferUsage {
                vertex_buffer: true,
                ..BufferUsage::empty()
            },
            false,
            self.vertexes.into_iter().map(vertex_map),
        )
        .unwrap();

        let index_buffer = IndexBuffer::new(memory_allocator, u8_ext, &self.index_map);
        (vertex_buffer, index_buffer)
    }
}

enum IndexBufferRepr {
    U8(Arc<CpuAccessibleBuffer<[u8]>>),
    U16(Arc<CpuAccessibleBuffer<[u16]>>),
    U32(Arc<CpuAccessibleBuffer<[u32]>>),
}

pub struct IndexBuffer(IndexBufferRepr);

impl IndexBuffer {
    fn new(memory_allocator: &(impl MemoryAllocator + ?Sized), u8_ext: bool, indexes: &[usize]) -> IndexBuffer {
        let max_index = indexes.iter().max().expect("expected at least one index");
        let buffer_usage = BufferUsage {
            index_buffer: true,
            ..BufferUsage::empty()
        };
        let repr = match (max_index, u8_ext) {
            (0..=0xff, true) => CpuAccessibleBuffer::from_iter(
                memory_allocator,
                buffer_usage,
                false,
                indexes
                    .iter()
                    .map(|v| u8::try_from(*v).expect("all indexes should have fit in a u8")),
            )
            .map(IndexBufferRepr::U8)
            .unwrap(),
            (0..=0xff, false) => CpuAccessibleBuffer::from_iter(
                memory_allocator,
                buffer_usage,
                false,
                indexes
                    .iter()
                    .map(|v| u16::try_from(*v).expect("all indexes should have fit in a u8, let alone a u16")),
            )
            .map(IndexBufferRepr::U16)
            .unwrap(),
            (0x100..=0xffff, _) => CpuAccessibleBuffer::from_iter(
                memory_allocator,
                buffer_usage,
                false,
                indexes
                    .iter()
                    .map(|v| u16::try_from(*v).expect("all indexes should have fit in a u16")),
            )
            .map(IndexBufferRepr::U16)
            .unwrap(),
            (0x10000..=0xffff_ffff, _) => CpuAccessibleBuffer::from_iter(
                memory_allocator,
                buffer_usage,
                false,
                indexes
                    .iter()
                    .map(|v| u32::try_from(*v).expect("all indexes should have fit in a u32")),
            )
            .map(IndexBufferRepr::U32)
            .unwrap(),
            _ => panic!(
                "max index of {} exceeds GPU limits of 32-bit indexes",
                max_index
            ),
        };
        IndexBuffer(repr)
    }

    pub fn len(&self) -> vulkano::DeviceSize {
        match &self.0 {
            IndexBufferRepr::U8(b) => b.len(),
            IndexBufferRepr::U16(b) => b.len(),
            IndexBufferRepr::U32(b) => b.len(),
        }
    }

    pub fn bind<L>(&self, cb: &mut AutoCommandBufferBuilder<L>) {
        match &self.0 {
            IndexBufferRepr::U8(buf) => cb.bind_index_buffer(buf.clone()),
            IndexBufferRepr::U16(buf) => cb.bind_index_buffer(buf.clone()),
            IndexBufferRepr::U32(buf) => cb.bind_index_buffer(buf.clone()),
        };
    }
}
