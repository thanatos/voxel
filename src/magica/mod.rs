use std::convert::TryFrom;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use vulkano::buffer::CpuAccessibleBuffer;
use vulkano::command_buffer::AutoCommandBufferBuilder;
use vulkano::device::Device;
use vulkano::memory::allocator::MemoryAllocator;
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::ViewportState;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::render_pass::{RenderPass, Subpass};
use vulkano::shader::ShaderModule;

/// Load MagicaVoxel files
pub mod io;

use io::{Chunk, ChunkData, Color, Voxel};

/// A MagicaVoxel model that's been uploaded to the GPU, and can be rendered.
pub struct MagicaModel {
    vertex_buffer: Arc<CpuAccessibleBuffer<[MagicaVertex]>>,
    index_buffer: crate::model_util::IndexBuffer,
}

impl MagicaModel {
    pub fn new(memory_allocator: &(impl MemoryAllocator + ?Sized), top_chunk: &Chunk) -> anyhow::Result<MagicaModel> {
        let voxels = find_xyzi_data(&top_chunk)?;
        let palette = find_rgba_data(&top_chunk)?;
        let mut model_builder = crate::model_util::ModelBuilder::new();
        for voxel in voxels {
            eprintln!("dump Voxel: {:?}", voxel);
            for side in CUBE_VERTEXES.iter() {
                let side_vertexes = [
                    // Triangle 1
                    side[0],
                    side[1],
                    side[2],
                    // Triangle 2
                    side[0],
                    side[2],
                    side[3],
                ];
                for vertex in side_vertexes {
                    let x = u16::from(voxel.x) + u16::from(vertex.0);
                    let y = u16::from(voxel.y) + u16::from(vertex.1);
                    let z = u16::from(voxel.z) + u16::from(vertex.2);
                    model_builder.push_vertex((x, y, z, voxel.color_index));
                }
            }
        }

        let (vertex_buffer, index_buffer) = model_builder.into_gpu(
            memory_allocator,
            |(x, y, z, color_idx)| MagicaVertex {
                position: [f32::from(x), y as f32, z as f32],
                color: palette
                    .get(usize::from(color_idx))
                    .map(|c| [u32::from(c.r), u32::from(c.g), u32::from(c.b)])
                    .expect("palette should contain a color for every index"),
            },
            false,
        );

        Ok(MagicaModel {
            vertex_buffer,
            index_buffer,
        })
    }
}

#[rustfmt::skip]
static CUBE_VERTEXES: &[[(u8, u8, u8); 4]] = &[
    // Bottom face
    [
        (0, 0, 0),
        (1, 0, 0),
        (1, 0, 1),
        (0, 0, 1),
    ],
    // Side "front"
    [
        (0, 0, 0),
        (1, 0, 0),
        (1, 1, 0),
        (0, 1, 0),
    ],
    // Side "back"
    [
        (0, 0, 1),
        (0, 1, 1),
        (1, 1, 1),
        (1, 0, 1),
    ],
    // Side "right"
    [
        (1, 0, 0),
        (1, 0, 1),
        (1, 1, 1),
        (1, 1, 0),
    ],
    // Side "left"
    [
        (0, 0, 0),
        (0, 1, 0),
        (0, 1, 1),
        (0, 0, 1),
    ],
    // Top face
    [
        (0, 1, 0),
        (0, 1, 1),
        (1, 1, 1),
        (1, 1, 0),
    ],
];

/// Get the voxel data from the loaded Magica file.
fn find_xyzi_data(top_chunk: &Chunk) -> anyhow::Result<&[Voxel]> {
    if !matches!(top_chunk.data, ChunkData::Main) {
        anyhow::bail!("top-level chunk was not the main chunk?");
    }
    let mut xyzi_voxels = None;
    for child in top_chunk.children.iter() {
        if let ChunkData::Xyzi { voxels } = &child.data {
            if xyzi_voxels.is_some() {
                anyhow::bail!("Multiple XYZI chunks in model?");
            }
            xyzi_voxels = Some(voxels.as_slice());
        }
    }
    xyzi_voxels.ok_or_else(|| anyhow::anyhow!("no XYZI chunk in model"))
}

/// Get the voxel data from the loaded Magica file.
fn find_rgba_data(top_chunk: &Chunk) -> anyhow::Result<&[Color]> {
    if !matches!(top_chunk.data, ChunkData::Main) {
        anyhow::bail!("top-level chunk was not the main chunk?");
    }
    let mut palette = None;
    for child in top_chunk.children.iter() {
        if let ChunkData::Rgba { palette: pal } = &child.data {
            if palette.is_some() {
                anyhow::bail!("Multiple RGBA chunks in model?");
            }
            palette = Some(pal.as_slice());
        }
    }
    palette.ok_or_else(|| anyhow::anyhow!("no RGBA chunk in model"))
}

pub(super) struct MagicaShaders {
    vs: Arc<ShaderModule>,
    fs: Arc<ShaderModule>,
}

impl MagicaShaders {
    pub(super) fn load(device: Arc<Device>) -> MagicaShaders {
        let vs = vs::load(device.clone()).expect("failed to load vertex shader");
        let fs = fs::load(device.clone()).expect("failed to load fragment shader");
        MagicaShaders { vs, fs }
    }
}

pub(super) fn build_pipeline(
    device: Arc<Device>,
    render_pass: Arc<RenderPass>,
    shaders: &MagicaShaders,
) -> Arc<GraphicsPipeline> {
    GraphicsPipeline::start()
        // Defines what kind of vertex input is expected.
        .vertex_input_state(BuffersDefinition::new().vertex::<MagicaVertex>())
        // The vertex shader.
        .vertex_shader(shaders.vs.entry_point("main").unwrap(), ())
        // Defines the viewport (explanations below).
        .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
        // The fragment shader.
        .fragment_shader(shaders.fs.entry_point("main").unwrap(), ())
        // This graphics pipeline object concerns the first pass of the render pass.
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        // Now that everything is specified, we call `build`.
        .build(device.clone())
        .unwrap()
}

pub(super) trait MagicaAutoCmdExt {
    fn draw_magica(&mut self, pipeline: Arc<GraphicsPipeline>, model: &MagicaModel) -> &mut Self;
}

impl<L> MagicaAutoCmdExt for AutoCommandBufferBuilder<L> {
    fn draw_magica(&mut self, pipeline: Arc<GraphicsPipeline>, model: &MagicaModel) -> &mut AutoCommandBufferBuilder<L> {
        self
            .bind_pipeline_graphics(pipeline)
            .bind_vertex_buffers(0, model.vertex_buffer.clone());
        model.index_buffer.bind(self);
        self
            .draw_indexed(
                u32::try_from(model.index_buffer.len()).unwrap(),
                1, // instance_count
                0, // first_index
                0, // vertex_offset
                0, // first_instance
            )
            .unwrap()
    }
}

#[repr(C)]
#[derive(Default, Clone, Copy, Zeroable, Pod)]
struct MagicaVertex {
    position: [f32; 3],
    color: [u32; 3],
}

vulkano::impl_vertex!(MagicaVertex, position, color);

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "\
#version 450

layout(binding = 0) uniform UniformBufferObject {
    mat4 model;
    mat4 view;
    mat4 proj;
} ubo;

layout(location = 0) in vec3 position;
layout(location = 1) in uvec3 color;

layout(location = 0) out vec3 color_out;

void main() {
    gl_Position = ubo.proj * ubo.view * vec4(position.x, position.y, position.z, 1.0);
    color_out = vec3(color.r / 255.0, color.g / 255.0, color.b / 255.0);
}"
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: "\
#version 450

layout(location = 0) in vec3 in_color;

layout(location = 0) out vec3 out_color;

void main() {
    out_color = in_color;
}"
    }
}
