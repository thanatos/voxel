use std::convert::{TryFrom, TryInto};
use std::sync::Arc;

use log::{debug, info, trace};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use smallvec::SmallVec;
use structopt::StructOpt;
use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::buffer::cpu_pool::CpuBufferPool;
use vulkano::buffer::{BufferUsage, TypedBufferAccess};
use vulkano::command_buffer::{AutoCommandBufferBuilder, SubpassContents};
use vulkano::descriptor_set::{PersistentDescriptorSet, WriteDescriptorSet};
use vulkano::image::view::ImageView;
use vulkano::image::SwapchainImage;
use vulkano::pipeline::{GraphicsPipeline, Pipeline, PipelineBindPoint};
use vulkano::pipeline::graphics::color_blend::ColorBlendState;
use vulkano::pipeline::graphics::input_assembly::{InputAssemblyState, PrimitiveTopology};
use vulkano::pipeline::graphics::vertex_input::BuffersDefinition;
use vulkano::pipeline::graphics::viewport::{Viewport, ViewportState};
use vulkano::render_pass::{Framebuffer, RenderPass, Subpass};
use vulkano::shader::ShaderModule;
use vulkano::swapchain::{AcquireError, Swapchain};
use vulkano::sync::{FlushError, GpuFuture};

mod camera;
mod init;
pub mod magica;
mod matrix;
mod png;
pub mod resources;
pub mod sw_image;
mod timing;
pub mod text_rendering;

use matrix::Matrix;

#[derive(Clone, Default)]
struct Position {
    x: f32,
    y: f32,
    z: f32,
}

struct Look {
    rotation_horz: f32,
    rotation_vert: f32,
}

impl Look {
    fn cursor_moved(&mut self, xrel: i32, yrel: i32) {
        const NINETY_DEG: f32 = std::f32::consts::PI / 2.; // N.b., it's in radians.

        self.rotation_horz += (xrel as f32) * degrees_to_radians(1.);
        self.rotation_vert += (yrel as f32) * degrees_to_radians(1.);

        if self.rotation_vert < -NINETY_DEG {
            self.rotation_vert = -NINETY_DEG;
        } else if NINETY_DEG < self.rotation_vert {
            self.rotation_vert = NINETY_DEG;
        }

        if self.rotation_horz < -std::f32::consts::PI * 2. {
            self.rotation_horz += std::f32::consts::PI * 2.;
        }
        if std::f32::consts::PI * 2. < self.rotation_horz {
            self.rotation_horz -= std::f32::consts::PI * 2.;
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look {
            rotation_horz: 0.,
            rotation_vert: 0.,
        }
    }
}

fn degrees_to_radians(degrees: f32) -> f32 {
    degrees * std::f32::consts::PI / 180.
}

#[derive(StructOpt)]
struct Args {
    #[structopt(long)]
    use_gpu_with_uuid: Option<uuid::Uuid>,
}

pub fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    let args = Args::from_args();
    info!("voxel started.");

    info!("init_sdl_and_vulkan()");
    let mut init = init::init_sdl_and_vulkan(args.use_gpu_with_uuid);
    info!("init_render_details()");
    let mut render_details = init::RenderDetails::init(
        init.vulkan_device.clone(),
        init.surface().clone(),
    ).unwrap();

    info!("Loading resources…");
    let mut resources = resources::Fonts::init(false).unwrap();
    info!("Loaded resources.");

    let fov_vert = 90. * std::f32::consts::PI / 180.;
    let fov_horz = fov_vert * (1. as f32) / (1. as f32);
    println!(
        "{:#?}",
        matrix::projection::perspective_fov_both(fov_horz, fov_vert, 0.1, 10.)
    );

    let vs = vs::load(init.vulkan_device.clone()).expect("failed to create shader module");
    let fs = fs::load(init.vulkan_device.clone()).expect("failed to create shader module");

    let lines_vs =
        lines::vs::load(init.vulkan_device.clone()).expect("failed to create shader module");
    let lines_fs =
        lines::fs::load(init.vulkan_device.clone()).expect("failed to create shader module");

    let blit_vs =
        blit::vs::load(init.vulkan_device.clone()).expect("failed to create shader module");
    let blit_fs =
        blit::fs::load(init.vulkan_device.clone()).expect("failed to create shader module");

    let magica_shaders = magica::MagicaShaders::load(init.vulkan_device.clone());
    let magica_model = {
        static MODEL: &'static [u8] = include_bytes!("vox/logo.vox");
        let top_chunk = magica::io::from_reader(std::io::Cursor::new(MODEL)).unwrap();
        magica::MagicaModel::new(init.vulkan_device.clone(), &top_chunk).unwrap()
    };

    let uniform_buffer_pool = CpuBufferPool::uniform_buffer(init.vulkan_device.clone());
    let blit_uniform_buffer_pool = CpuBufferPool::uniform_buffer(init.vulkan_device.clone());

    let mut previous_frame_end: Option<Box<dyn GpuFuture>> =
        Some(Box::new(vulkano::sync::now(init.vulkan_device.clone())));
    let mut swapchain_needs_recreating = false;
    let mut timer = timing::Timer::start();
    let mut frames = 0;
    let start = std::time::Instant::now();
    let mut rotation: Look = Default::default();
    let mut position: Position = Default::default();
    position.y = 1.5;
    let mut pipelines = Pipelines::new(
        init.vulkan_device.clone(),
        render_details.render_pass.clone(),
        &vs,
        &fs,
        &lines_vs,
        &lines_fs,
        &blit_vs,
        &blit_fs,
        &magica_shaders,
    );

    init.sdl_context.mouse().set_relative_mouse_mode(true);
    let mut rel_mouse = true;

    'running: loop {
        for event in init.event_pump.poll_iter() {
            match event {
                Event::MouseMotion { xrel, yrel, .. } => {
                    println!("Mouse motion: {:?}, {:?}", xrel, yrel);
                    if rel_mouse {
                        rotation.cursor_moved(xrel, yrel);
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::W),
                    ..
                } => {
                    let bearing = rotation.rotation_horz;
                    let y_change = bearing.sin();
                    let x_change = bearing.cos();
                    position.x += x_change;
                    position.z += y_change;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Q),
                    ..
                } => {
                    position.y += 0.5;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Z),
                    ..
                } => {
                    position.y -= 0.5;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Left),
                    ..
                } => {
                    position.x -= 0.5;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Right),
                    ..
                } => {
                    position.x += 0.5;
                }
                Event::KeyDown {
                    keycode: Some(Keycode::M),
                    ..
                } => {
                    rel_mouse = !rel_mouse;
                    init.sdl_context.mouse().set_relative_mouse_mode(rel_mouse);
                }
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => break 'running,
                event => {
                    println!("Unknown event: {:?}", event);
                }
            }
        }

        if swapchain_needs_recreating {
            match render_details.recreate_swapchain(&init) {
                Ok(true) => {
                    swapchain_needs_recreating = false;
                    pipelines = Pipelines::new(
                        init.vulkan_device.clone(),
                        render_details.render_pass.clone(),
                        &vs,
                        &fs,
                        &lines_vs,
                        &lines_fs,
                        &blit_vs,
                        &blit_fs,
                        &magica_shaders,
                    );
                }
                // These happen. Examples ignore them. What exactly is going on here?
                Ok(false) => continue,
                Err(err) => panic!("error recreating swapchain: {}", err),
            }
        }

        let output = render_frame(
            &init.vulkan_device,
            &init.queue,
            previous_frame_end
                .take()
                .unwrap_or_else(|| Box::new(vulkano::sync::now(init.vulkan_device.clone()))),
            &render_details.swapchain,
            &render_details.swapchain_images,
            &render_details.render_pass,
            render_details.dimensions,
            &pipelines,
            &uniform_buffer_pool,
            &blit_uniform_buffer_pool,
            (std::time::Instant::now() - start).as_secs_f32(),
            &position,
            &rotation,
            camera::camera(
                position.x,
                position.y,
                position.z,
                rotation.rotation_horz,
                rotation.rotation_vert,
            ),
            &mut resources,
            &magica_model,
        );
        match output {
            RendererOutput::Rendering(future) => {
                previous_frame_end = Some(future);
                frames += 1;
            }
            RendererOutput::SwapchainNeedsRecreating => swapchain_needs_recreating = true,
        }

        if let Some(previous_frame_end) = previous_frame_end.as_mut() {
            previous_frame_end.cleanup_finished();
        }

        if frames & 0x3 == 0 {
            let mark = timer.mark();
            if 2 <= mark.as_secs() {
                let fps = frames as f64 / mark.as_secs_f64();
                debug!(
                    "{:.3} FPS ({} frames over {}s)",
                    fps,
                    frames,
                    mark.as_secs_f64()
                );
                frames = 0;
                timer = timing::Timer::start();
            }
        }

        //::std::thread::sleep(::std::time::Duration::new(0, 1_000_000_000u32 / 60));
    }
}

enum RendererOutput {
    Rendering(Box<dyn GpuFuture>),
    SwapchainNeedsRecreating,
}

#[repr(C)]
#[derive(Clone)]
struct UniformBufferObject {
    model: Matrix,
    view: Matrix,
    proj: Matrix,
    t: f32,
}

#[repr(C)]
#[derive(Clone)]
struct BlitUniform {
    proj: Matrix,
}

fn screen_quad_to_triangle_fan(pos: (u32, u32), size: (u32, u32)) -> SmallVec<[BlitImageVertex; 4]> {
    let mut vertexes = smallvec::SmallVec::new();
    vertexes.push(BlitImageVertex {
        position: [pos.0, pos.1 + size.1],
        texture_coord: [0., 1.],
    });
    vertexes.push(BlitImageVertex {
        position: [pos.0, pos.1],
        texture_coord: [0., 0.],
    });
    vertexes.push(BlitImageVertex {
        position: [pos.0 + size.0, pos.1 + size.1],
        texture_coord: [1., 1.],
    });
    vertexes.push(BlitImageVertex {
        position: [pos.0 + size.0, pos.1],
        texture_coord: [1., 0.],
    });
    vertexes
}

/// A container for the various Vulkan graphics pipelines we create.
struct Pipelines {
    normal_pipeline: Arc<GraphicsPipeline>,
    lines_pipeline: Arc<GraphicsPipeline>,
    blit_pipeline: Arc<GraphicsPipeline>,
    magica_pipeline: Arc<GraphicsPipeline>,
}

impl Pipelines {
    fn new(
        device: Arc<vulkano::device::Device>,
        render_pass: Arc<RenderPass>,
        normal_vs: &ShaderModule,
        normal_fs: &ShaderModule,
        lines_vs: &ShaderModule,
        lines_fs: &ShaderModule,
        blit_vs: &ShaderModule,
        blit_fs: &ShaderModule,
        magica_shaders: &magica::MagicaShaders,
    ) -> Pipelines {
        let normal_pipeline = GraphicsPipeline::start()
            // Defines what kind of vertex input is expected.
            .vertex_input_state(BuffersDefinition::new().vertex::<Vertex>())
            // The vertex shader.
            .vertex_shader(normal_vs.entry_point("main").unwrap(), ())
            // Defines the viewport (explanations below).
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            // The fragment shader.
            .fragment_shader(normal_fs.entry_point("main").unwrap(), ())
            // This graphics pipeline object concerns the first pass of the render pass.
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            // Now that everything is specified, we call `build`.
            .build(device.clone())
            .unwrap();

        let lines_pipeline = GraphicsPipeline::start()
            // Defines what kind of vertex input is expected.
            .vertex_input_state(BuffersDefinition::new().vertex::<Line>())
            // The vertex shader.
            .vertex_shader(lines_vs.entry_point("main").unwrap(), ())
            // Defines the viewport (explanations below).
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            // The fragment shader.
            .fragment_shader(lines_fs.entry_point("main").unwrap(), ())
            // This graphics pipeline object concerns the first pass of the render pass.
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            .input_assembly_state(InputAssemblyState::new().topology(PrimitiveTopology::LineList))
            // Now that everything is specified, we call `build`.
            .build(device.clone())
            .unwrap();

        let blit_pipeline = GraphicsPipeline::start()
            // Defines what kind of vertex input is expected.
            .vertex_input_state(BuffersDefinition::new().vertex::<BlitImageVertex>())
            // The vertex shader.
            .vertex_shader(blit_vs.entry_point("main").unwrap(), ())
            // Defines the viewport (explanations below).
            .viewport_state(ViewportState::viewport_dynamic_scissor_irrelevant())
            // The fragment shader.
            .fragment_shader(blit_fs.entry_point("main").unwrap(), ())
            // This graphics pipeline object concerns the first pass of the render pass.
            .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
            .color_blend_state(ColorBlendState::default().blend_alpha())
            .input_assembly_state(
                InputAssemblyState::new().topology(PrimitiveTopology::TriangleStrip),
            )
            // Now that everything is specified, we call `build`.
            .build(device.clone())
            .unwrap();

        let magica_pipeline = magica::build_pipeline(device, render_pass, magica_shaders);

        Pipelines {
            normal_pipeline,
            lines_pipeline,
            blit_pipeline,
            magica_pipeline,
        }
    }
}

fn render_frame(
    device: &Arc<vulkano::device::Device>,
    queue: &Arc<vulkano::device::Queue>,
    previous_frame_end: Box<dyn GpuFuture>,
    swapchain: &Arc<Swapchain<()>>,
    swapchain_images: &[Arc<SwapchainImage<()>>],
    render_pass: &Arc<RenderPass>,
    dimensions: [u32; 2],
    pipelines: &Pipelines,
    uniform_buffer_pool: &CpuBufferPool<UniformBufferObject>,
    blit_uniform_buffer_pool: &CpuBufferPool<BlitUniform>,
    t: f32,
    position: &Position,
    look: &Look,
    view: Matrix,
    resources: &mut resources::Fonts,
    magica_model: &magica::MagicaModel,
) -> RendererOutput {
    trace!(target: "render_frame", "Building framebuffers");
    let framebuffers = swapchain_images
        .iter()
        .map(|image| {
            let image_view = ImageView::new(image.clone()).unwrap();
            let fb = Framebuffer::start(render_pass.clone())
                .add(image_view)
                .unwrap()
                .build()
                .unwrap();
            fb
        })
        .collect::<Vec<_>>();

    let fov_vert = 90. * std::f32::consts::PI / 180.;
    let aspect = (dimensions[0] as f32) / (dimensions[1] as f32);
    let ubo = UniformBufferObject {
        model: Matrix::from([
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ]),
        view,
        proj: matrix::projection::perspective_fov(fov_vert, aspect, 0.1, 80.),
        /*
        proj: Matrix::from([
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ]),
        */
        t,
    };
    let subbuffer_normal = Arc::new(uniform_buffer_pool.next(ubo.clone()).unwrap());
    let subbuffer_lines = Arc::new(uniform_buffer_pool.next(ubo).unwrap());

    let descriptor_set_normal = {
        let layout = pipelines.normal_pipeline.layout().descriptor_set_layouts()[0].clone();
        {
            let write_descriptor_set = WriteDescriptorSet::buffer(0, subbuffer_normal);
            PersistentDescriptorSet::new(layout, std::iter::once(write_descriptor_set)).unwrap()
        }
    };
    let descriptor_set_lines = {
        let layout = pipelines.lines_pipeline.layout().descriptor_set_layouts()[0].clone();
        {
            let write_descriptor_set = WriteDescriptorSet::buffer(0, subbuffer_lines);
            PersistentDescriptorSet::new(layout, std::iter::once(write_descriptor_set)).unwrap()
        }
    };

    trace!(target: "render_frame", "acquire_next_image");
    let (image_index, _, acquire_future) = {
        match vulkano::swapchain::acquire_next_image(swapchain.clone(), None) {
            Ok(r) => r,
            Err(AcquireError::OutOfDate) => return RendererOutput::SwapchainNeedsRecreating,
            Err(err) => panic!("Failed to acquire next image: {}", err),
        }
    };

    let framebuffer = &framebuffers[image_index];

    trace!(target: "render_frame", "AutoCommandBufferBuilder");
    let mut builder = AutoCommandBufferBuilder::primary(
        device.clone(),
        queue.family(),
        vulkano::command_buffer::CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();

    let viewport = Viewport {
        origin: [0.0, 0.0],
        dimensions: [dimensions[0] as f32, dimensions[1] as f32],
        depth_range: 0.0..1.0,
    };

    // Don't need to do this every frame!
    let vertex_buffer = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::vertex_buffer(),
        false,
        vec![
            /*
            Vertex { position: [-0.5, -0.5] },
            Vertex { position: [ 0.0,  0.5] },
            Vertex { position: [ 0.5, -0.25] },
            */
            /*
            Vertex { position: [-4., -4.] },
            Vertex { position: [ 0.0,  4.] },
            Vertex { position: [ 4., -2.] },
            */
            Vertex {
                position: [-4., 0.],
            },
            Vertex { position: [0., 4.] },
            Vertex { position: [4., 0.] },
        ]
        .into_iter(),
    )
    .unwrap();

    let lines = {
        let mut lines = vec![];
        for i in -10i8..=10 {
            lines.push(Line {
                position: [f32::from(i), -10.0],
                color: [1., 0., 0.],
            });
            lines.push(Line {
                position: [f32::from(i), 10.0],
                color: [1., 0., 0.],
            });
            lines.push(Line {
                position: [10.0, f32::from(i)],
                color: [0., 0., 1.],
            });
            lines.push(Line {
                position: [-10.0, f32::from(i)],
                color: [0., 0., 1.],
            });
        }
        lines
    };

    let lines_vert_buf = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::vertex_buffer(),
        false,
        lines.into_iter(),
    )
    .unwrap();

    let (image, (image_w, image_h), image_future) = {
        let t_image = text_rendering::render_text("Hello, world.", &mut resources.deja_vu, sw_image::Pixel { r: 0, g: 255, b: 0, a: 255}, &resources.deja_vu_cache).unwrap();
        let rgba_pixel_data = CpuAccessibleBuffer::from_iter(
            queue.device().clone(),
            BufferUsage::transfer_source(),
            false, // host_cached
            t_image.pixels().map(|p| (p.r, p.g, p.b, p.a)),
        )
        .unwrap();
        let width = t_image.width();
        let height = t_image.height();
        let dimensions = vulkano::image::ImageDimensions::Dim2d {
            width,
            height,
            array_layers: 1,
        };
        let (image, future) = vulkano::image::ImmutableImage::from_buffer(
            rgba_pixel_data,
            dimensions,
            vulkano::image::MipmapsCount::One,
            vulkano::format::Format::R8G8B8A8_UNORM,
            //vulkano::image::ImageLayout::ShaderReadOnlyOptimal,
            queue.clone(),
        )
        .unwrap();
        (image, (width, height), future)
    };

    let blits_vert_buf = {
        let blits = screen_quad_to_triangle_fan((32, 5), (image_w, image_h));

        CpuAccessibleBuffer::from_iter(
            device.clone(),
            BufferUsage::vertex_buffer(),
            false,
            blits.into_iter(),
        )
        .unwrap()
    };
    let descriptor_set_blits = {
        let blit_uniform = BlitUniform {
            proj: crate::matrix::screen_matrix(dimensions[0], dimensions[1]),
        };
        let subbuffer_blit = blit_uniform_buffer_pool.next(blit_uniform).unwrap();
        let layout = pipelines.blit_pipeline.layout().descriptor_set_layouts()[0].clone();
        {
            let write_buffer = WriteDescriptorSet::buffer(0, subbuffer_blit);
            let sampler = vulkano::sampler::Sampler::simple_repeat_linear_no_mipmap(device.clone()).unwrap();
            let image_view = vulkano::image::view::ImageView::new(image.clone()).unwrap();
            let write_sampler = WriteDescriptorSet::image_view_sampler(1, image_view, sampler);
            PersistentDescriptorSet::new(layout, [write_buffer, write_sampler]).unwrap()
        }
    };

    use magica::MagicaAutoCmdExt;
    trace!(target: "render_frame", "begin_render_pass");
    builder
        .begin_render_pass(
            framebuffer.clone(),
            SubpassContents::Inline,
            vec![[0.0, 0.25, 1.0, 1.0].into()],
        )
        .unwrap()
        .set_viewport(0, [viewport])
        .bind_pipeline_graphics(pipelines.normal_pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipelines.normal_pipeline.layout().clone(),
            0,
            descriptor_set_normal,
        )
        .bind_vertex_buffers(0, vertex_buffer.clone())
        .draw(vertex_buffer.len().try_into().unwrap(), 1, 0, 0)
        .unwrap()
        .bind_pipeline_graphics(pipelines.lines_pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipelines.lines_pipeline.layout().clone(),
            0,
            descriptor_set_lines,
        )
        .bind_vertex_buffers(0, lines_vert_buf.clone())
        .draw(lines_vert_buf.len().try_into().unwrap(), 1, 0, 0)
        .unwrap()
        .draw_magica(pipelines.magica_pipeline.clone(), magica_model)
        .bind_pipeline_graphics(pipelines.blit_pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Graphics,
            pipelines.blit_pipeline.layout().clone(),
            0,
            descriptor_set_blits,
        )
        .bind_vertex_buffers(0, blits_vert_buf.clone())
        .draw(blits_vert_buf.len().try_into().unwrap(), 1, 0, 0)
        .unwrap()
        .end_render_pass()
        .unwrap();

    trace!(target: "render_frame", "build command buffer");
    let command_buffer = builder.build().unwrap();

    trace!(target: "render_frame", "scheduling command buffer");
    let result = previous_frame_end
        .join(acquire_future)
        .join(image_future)
        .then_execute(queue.clone(), command_buffer)
        .expect("then_execute failed")
        .then_swapchain_present(queue.clone(), swapchain.clone(), image_index)
        .then_signal_fence_and_flush();
    match result {
        Ok(future) => RendererOutput::Rendering(Box::new(future)),
        Err(FlushError::OutOfDate) => RendererOutput::SwapchainNeedsRecreating,
        Err(err) => panic!("then_signal_fence_and_flush failed: {:?}", err),
    }
}

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        src: "
#version 450

layout(binding = 0) uniform UniformBufferObject {
    mat4 model;
    mat4 view;
    mat4 proj;
    float t;
} ubo;

layout(location = 0) in vec2 position;

void main() {
    gl_Position = ubo.proj * ubo.view * vec4(position, sin(ubo.t) * 25 - 25 - 10, 1.0);
    //gl_Position = ubo.view * ubo.proj * vec4(position, sin(ubo.t) * 25 - 25 - 10, 1.0);
}"
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        src: "
#version 450

layout(location = 0) out vec4 f_color;

void main() {
    f_color = vec4(1.0, 0.0, 0.0, 1.0);
}"
    }
}

mod lines {
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            src: "
#version 450

layout(binding = 0) uniform UniformBufferObject {
    mat4 model;
    mat4 view;
    mat4 proj;
    float t;
} ubo;

layout(location = 0) in vec2 position;
layout(location = 1) in vec3 color;

layout(location = 0) out vec3 color_out;

void main() {
    gl_Position = ubo.proj * ubo.view * vec4(position.x, 0, position.y, 1.0);
    color_out = color;
}"
        }
    }

    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            src: "
#version 450

layout(location = 0) out vec4 f_color;

layout(location = 0) in vec3 color_in;

void main() {
    //f_color = vec4(0.0, 1.0, 0.0, 1.0);
    f_color = vec4(color_in, 1.0);
}"
        }
    }
}

mod blit {
    pub mod vs {
        vulkano_shaders::shader! {
            ty: "vertex",
            src: "
#version 450

layout(binding = 0) uniform BlitUniform {
    mat4 proj;
} ubo;

layout(location = 0) in uvec2 position;
layout(location = 1) in vec2 texture_coord;
layout(location = 0) out vec2 out_texture_coord;

void main() {
    gl_Position = ubo.proj * vec4(position.x, position.y, 0.0, 1.0);
    out_texture_coord = texture_coord;
}"
        }
    }

    pub mod fs {
        vulkano_shaders::shader! {
            ty: "fragment",
            src: "
#version 450

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 f_color;
layout(set = 0, binding = 1) uniform sampler2D texSampler;

void main() {
    //f_color = vec4(0.0, 1.0, 0.0, 1.0);
    f_color = texture(texSampler, texCoord);
}"
        }
    }
}

#[derive(Default, Copy, Clone)]
struct Vertex {
    position: [f32; 2],
}

vulkano::impl_vertex!(Vertex, position);

#[derive(Default, Copy, Clone)]
struct BlitImageVertex {
    position: [u32; 2],
    texture_coord: [f32; 2],
}

vulkano::impl_vertex!(BlitImageVertex, position, texture_coord);

#[derive(Default, Copy, Clone)]
struct Line {
    position: [f32; 2],
    color: [f32; 3],
}

vulkano::impl_vertex!(Line, position, color);
