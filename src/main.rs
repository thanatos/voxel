use std::sync::Arc;

use log::{debug, info, trace};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use vulkano::buffer::BufferUsage;
use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::buffer::cpu_pool::CpuBufferPool;
use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState};
use vulkano::descriptor::descriptor_set::{PersistentDescriptorSet, UnsafeDescriptorSetLayout};
use vulkano::descriptor::pipeline_layout::PipelineLayoutDesc;
use vulkano::framebuffer::{Framebuffer, RenderPassAbstract, Subpass};
use vulkano::image::SwapchainImage;
use vulkano::pipeline::GraphicsPipeline;
use vulkano::pipeline::viewport::Viewport;
use vulkano::swapchain::{AcquireError, Swapchain, SwapchainCreationError};
use vulkano::sync::{FlushError, GpuFuture};

mod init;
mod matrix;
mod timing;

use matrix::Matrix;

struct Look {
    rotation_horz: f32,
    rotation_vert: f32,
}

impl Look {
    fn cursor_moved(&mut self, xrel: i32, yrel: i32) {
        const NINETY_DEG: f32 = std::f32::consts::PI / 2.;  // N.b., it's in radians.

        self.rotation_horz += -(xrel as f32) * degrees_to_radians(1.);
        self.rotation_vert += -(yrel as f32) * degrees_to_radians(1.);

        if self.rotation_vert < -NINETY_DEG {
            self.rotation_vert = -NINETY_DEG;
        } else if NINETY_DEG < self.rotation_vert {
            self.rotation_vert = NINETY_DEG;
        }
    }

    fn to_matrix(&self) -> Matrix {
        matrix::transformations::rotate_y(self.rotation_horz)
            * matrix::transformations::rotate_x(self.rotation_vert)
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

fn main() {
    env_logger::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    info!("init_sdl_and_vulkan()");
    let mut init = init::init_sdl_and_vulkan();
    info!("init_render_details()");
    let mut render_details = init::init_render_details(
        init.vulkan_device.clone(),
        &init.queue,
        init.surface().clone(),
    );

    let fov_vert = 90. * std::f32::consts::PI / 180.;
    let fov_horz = fov_vert * (1. as f32) / (1. as f32);
    println!("{:#?}", matrix::projection::perspective_fov_both(fov_horz, fov_vert, 0.1, 10.));

    let vs = vs::Shader::load(init.vulkan_device.clone()).expect("failed to create shader module");
    let fs = fs::Shader::load(init.vulkan_device.clone()).expect("failed to create shader module");

    let lines_vs = lines::vs::Shader::load(init.vulkan_device.clone()).expect("failed to create shader module");
    let lines_fs = lines::fs::Shader::load(init.vulkan_device.clone()).expect("failed to create shader module");

    let uniform_buffer_pool = CpuBufferPool::uniform_buffer(init.vulkan_device.clone());

    let mut previous_frame_end: Option<Box<dyn GpuFuture>> = Some(Box::new(
        vulkano::sync::now(init.vulkan_device.clone())
    ));
    let mut swapchain_needs_recreating = false;
    let mut timer = timing::Timer::start();
    let mut frames = 0;
    let start = std::time::Instant::now();
    let mut rotation: Look = Default::default();

    init.sdl_context.mouse().set_relative_mouse_mode(true);
    'running: loop {
        for event in init.event_pump.poll_iter() {
            match event {
                Event::MouseMotion { xrel, yrel, .. } => {
                    println!("Mouse motion: {:?}, {:?}", xrel, yrel);
                    rotation.cursor_moved(xrel, yrel);
                }
                Event::Quit {..} | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                }
                event => {
                    println!("Unknown event: {:?}", event);
                }
            }
        }

        if swapchain_needs_recreating {
            debug!("Recreating swap chain");
            let dimensions = {
                let (new_width, new_height) = init.window().size();
                [new_width, new_height]
            };
            let (new_swapchain, new_images) = {
                match render_details.swapchain.recreate_with_dimensions(dimensions) {
                    Ok(r) => r,
                    // These happen. Examples ignore them. What exactly is going on here?
                    Err(SwapchainCreationError::UnsupportedDimensions) => continue,
                    Err(err) => panic!(err),
                }
            };
            render_details.swapchain = new_swapchain;
            render_details.swapchain_images = new_images;
            render_details.dimensions = dimensions;
            swapchain_needs_recreating = false;
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
            &vs,
            &fs,
            &lines_vs,
            &lines_fs,
            &uniform_buffer_pool,
            (std::time::Instant::now() - start).as_secs_f32(),
            &rotation,
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
            println!("checking: {:?}", mark);
            if 2 <= mark.as_secs() {
                let fps = frames as f64 / mark.as_secs_f64();
                debug!("{} FPS ({} frames over {}s)", fps, frames, mark.as_secs_f64());
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
struct UniformBufferObject {
    model: Matrix,
    view: Matrix,
    proj: Matrix,
    t: f32,
}

fn render_frame(
    device: &Arc<vulkano::device::Device>,
    queue: &Arc<vulkano::device::Queue>,
    previous_frame_end: Box<dyn GpuFuture>,
    swapchain: &Arc<Swapchain<()>>,
    swapchain_images: &[Arc<SwapchainImage<()>>],
    render_pass: &Arc<dyn RenderPassAbstract + Send + Sync>,
    dimensions: [u32; 2],
    vs: &vs::Shader,
    fs: &fs::Shader,
    lines_vs: &lines::vs::Shader,
    lines_fs: &lines::fs::Shader,
    uniform_buffer_pool: &CpuBufferPool<UniformBufferObject>,
    t: f32,
    rotation: &Look,
) -> RendererOutput {
    trace!(target: "render_frame", "Building framebuffers");
    let framebuffers = swapchain_images
        .iter()
        .map(|image| {
            let fb = Framebuffer::start(render_pass.clone())
                .add(image.clone())
                .unwrap()
                .build()
                .unwrap();
            Arc::new(fb)
        })
        .collect::<Vec<_>>();

    trace!(target: "render_frame", "Creating pipeline");
    let pipeline = Arc::new(GraphicsPipeline::start()
        // Defines what kind of vertex input is expected.
        .vertex_input_single_buffer::<Vertex>()
        // The vertex shader.
        .vertex_shader(vs.main_entry_point(), ())
        // Defines the viewport (explanations below).
        .viewports_dynamic_scissors_irrelevant(1)
        // The fragment shader.
        .fragment_shader(fs.main_entry_point(), ())
        // This graphics pipeline object concerns the first pass of the render pass.
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        // Now that everything is specified, we call `build`.
        .build(device.clone())
        .unwrap());

    /*
    let fov_vert = 90. * std::f32::consts::PI / 180.;
    let fov_horz = fov_vert * (dimensions[0] as f32) / (dimensions[1] as f32);
    */
    let fov_vert = 90. * std::f32::consts::PI / 180.;
    let aspect = (dimensions[0] as f32) / (dimensions[1] as f32);
    let subbuffer = uniform_buffer_pool.next(UniformBufferObject {
        model: Matrix::from([
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ]),
        view: rotation.to_matrix(),
        proj: matrix::projection::perspective_fov(fov_vert, aspect, 0.1, 10.),
        /*
        proj: Matrix::from([
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
        ]),
        */
        t,
    }).unwrap();

    let descriptor_set = {
        let layout = Arc::new(UnsafeDescriptorSetLayout::new(
            device.clone(),
            [
                Some(pipeline.descriptor(0, 0).unwrap()),
            ].iter().cloned(),
        ).unwrap());
        let pds = PersistentDescriptorSet::<()>::start(layout)
            .add_buffer(subbuffer)
            .unwrap()
            .build()
            .unwrap();
        Arc::new(pds)
    };

    //PersistentDescriptorSet::<()>::start(pipeline.layout());

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
    let mut builder = AutoCommandBufferBuilder::primary_one_time_submit(
        device.clone(),
        queue.family()
    ).unwrap();

    let dynamic_state = DynamicState {
        viewports: Some(vec![Viewport {
            origin: [0.0, 0.0],
            dimensions: [dimensions[0] as f32, dimensions[1] as f32],
            depth_range: 0.0 .. 1.0,
        }]),
        .. DynamicState::none()
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
            Vertex { position: [-4., 0.] },
            Vertex { position: [0., 4.] },
            Vertex { position: [4., 0.] },
        ].into_iter(),
    ).unwrap();

    let lines = {
        let mut lines = vec![];
        for i in -10i8 .. 10 {
            lines.push(Vertex { position: [f32::from(i), -10.0] });
            lines.push(Vertex { position: [f32::from(i), 10.0] });
            lines.push(Vertex { position: [10.0, f32::from(i)] });
            lines.push(Vertex { position: [-10.0, f32::from(i)] });
        }
        lines
    };

    let lines_vert_buf = CpuAccessibleBuffer::from_iter(
        device.clone(),
        BufferUsage::vertex_buffer(),
        false,
        lines.into_iter(),
    ).unwrap();

    let lines_pipeline = Arc::new(GraphicsPipeline::start()
        // Defines what kind of vertex input is expected.
        .vertex_input_single_buffer::<Vertex>()
        // The vertex shader.
        .vertex_shader(lines_vs.main_entry_point(), ())
        // Defines the viewport (explanations below).
        .viewports_dynamic_scissors_irrelevant(1)
        // The fragment shader.
        .fragment_shader(lines_fs.main_entry_point(), ())
        // This graphics pipeline object concerns the first pass of the render pass.
        .render_pass(Subpass::from(render_pass.clone(), 0).unwrap())
        .line_list()
        // Now that everything is specified, we call `build`.
        .build(device.clone())
        .unwrap());

    trace!(target: "render_frame", "begin_render_pass");
    builder
        .begin_render_pass(framebuffer.clone(), false, vec![[0.0, 0.25, 1.0, 1.0].into()])
        .unwrap()
        .draw(pipeline.clone(), &dynamic_state, vertex_buffer.clone(), descriptor_set.clone(), ())
        .unwrap()
        .draw(lines_pipeline.clone(), &dynamic_state, lines_vert_buf.clone(), descriptor_set, ())
        .unwrap()
        .end_render_pass()
        .unwrap();

    trace!(target: "render_frame", "build command buffer");
    let command_buffer = builder.build().unwrap();

    trace!(target: "render_frame", "scheduling command buffer");
    let result = previous_frame_end
        .join(acquire_future)
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
    vulkano_shaders::shader!{
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
}"
    }
}

mod fs {
    vulkano_shaders::shader!{
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
        vulkano_shaders::shader!{
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
    gl_Position = ubo.proj * ubo.view * vec4(position.x, 0, position.y, 1.0);
}"
        }
    }

    pub mod fs {
        vulkano_shaders::shader!{
            ty: "fragment",
            src: "
#version 450

layout(location = 0) out vec4 f_color;

void main() {
    f_color = vec4(0.0, 1.0, 0.0, 1.0);
}"
        }
    }
}

#[derive(Default, Copy, Clone)]
struct Vertex {
    position: [f32; 2],
}

vulkano::impl_vertex!(Vertex, position);
