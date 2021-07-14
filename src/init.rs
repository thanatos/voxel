use std::cmp::{max, min};
use std::ffi::CString;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use log::{debug, info, trace};
use sdl2::video::Window;
use vulkano::device::{Device, Features, Queue};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice, RawInstanceExtensions};
use vulkano::render_pass::RenderPass;
use vulkano::swapchain::{Surface, Swapchain};
use vulkano::VulkanObject;

pub struct Init {
    pub sdl_context: sdl2::Sdl,
    pub vulkan: Arc<Instance>,
    pub vulkan_device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub event_pump: sdl2::EventPump,

    surface: ManuallyDrop<Arc<Surface<()>>>,
    window: ManuallyDrop<Window>,
}

impl Init {
    pub fn window(&self) -> &Window {
        &self.window
    }

    pub fn surface(&self) -> &Arc<Surface<()>> {
        &self.surface
    }
}

impl Drop for Init {
    fn drop(&mut self) {
        // We *must* drop the Surface before the window. It depends on the Window existing, but
        // has no wany of tracking that. (The <W> in Surface requires Send, but an SDLWindow is
        // !Send; this bit guarantees that we drop in the right order or panic.)
        //
        // See: https://github.com/Rust-SDL2/rust-sdl2/pull/785
        // See: https://github.com/vulkano-rs/vulkano/issues/994
        if Arc::strong_count(&self.surface) != 1 {
            panic!("something was still referencing the surface.")
        }
        unsafe {
            ManuallyDrop::drop(&mut self.surface);
            ManuallyDrop::drop(&mut self.window);
        }
    }
}

pub struct RenderDetails {
    pub swapchain: Arc<Swapchain<()>>,
    pub swapchain_images: Vec<Arc<SwapchainImage<()>>>,
    pub render_pass: Arc<RenderPass>,
    pub dimensions: [u32; 2],
}

pub fn init_sdl_and_vulkan() -> Init {
    let sdl_context = sdl2::init().expect("Failed to initialize SDL.");
    debug!("SDL initialized.");

    // Event pump
    let event_pump = sdl_context.event_pump().unwrap();

    let video_subsystem = sdl_context.video().unwrap();
    trace!("SDL video subsystem initialized.");
    let window = video_subsystem
        .window("Voxel", 640, 480)
        .vulkan()
        .resizable()
        .build()
        .unwrap();
    trace!("SDL window created.");
    let instance_extensions = window.vulkan_instance_extensions().unwrap();
    let raw_instance_extensions = RawInstanceExtensions::new(
        instance_extensions
            .iter()
            .map(|&v| CString::new(v).unwrap()),
    );

    let (instance, device, queue) = init_vulkan(raw_instance_extensions);

    trace!("Creating surface in SDL.");
    let surface_handle = window
        .vulkan_create_surface(instance.internal_object())
        .unwrap();
    trace!("Surface created in SDL.");
    let surface = unsafe { Surface::from_raw_surface(instance.clone(), surface_handle, ()) };
    trace!("Vulkan Surface created from SDL surface.");
    let surface = ManuallyDrop::new(Arc::new(surface));
    // NOTE: Do not add failures / exits from here to function end.

    // Finish
    info!("SDL & Vulkan initialized.");

    Init {
        sdl_context,
        vulkan: instance,
        vulkan_device: device,
        queue,
        window: ManuallyDrop::new(window),
        surface,
        event_pump,
    }
}

pub fn init_render_details(
    device: Arc<Device>,
    queue: &Arc<Queue>,
    surface: Arc<Surface<()>>,
) -> RenderDetails {
    info!("Creating RenderDetails…");

    // Swapchain
    let (swapchain, images, dimensions, format) = {
        trace!("Querying surface capabilities");
        let caps = surface
            .capabilities(device.physical_device())
            .expect("Failed to query device capabilities");

        debug!("Supported formats");
        for supported_format in &caps.supported_formats {
            debug!("  {:?}", supported_format);
        }

        // Try to use double-buffering.
        let buffers_count = match caps.max_image_count {
            None => max(2, caps.min_image_count),
            Some(limit) => min(max(2, caps.min_image_count), limit),
        };

        // Just use the first format
        // TODO: Do we need to be more aware of this value, or can we just render into whatever we
        // get and not care? It seems like we'd *have* to care?
        let (format, color_space) = caps.supported_formats[0];

        // TODO: figure this out
        // The created swapchain will be used as a color attachment for rendering.
        let usage = ImageUsage {
            color_attachment: true,
            ..ImageUsage::none()
        };

        let dimensions = caps
            .current_extent
            .expect("Unable to get surface extent for swapchain.");

        let (swapchain, images) = Swapchain::start(device.clone(), surface)
            .num_images(buffers_count)
            .format(format)
            .dimensions(dimensions)
            .usage(usage)
            .transform(caps.current_transform)
            .color_space(color_space)
            .build()
            .expect("Failed to create swapchain");

        (swapchain, images, dimensions, format)
    };

    // Render pass
    let render_pass = Arc::new(
        vulkano::single_pass_renderpass!(device,
            attachments: {
                color: {
                    load: Clear,
                    store: Store,
                    //format: vulkano::format::Format::R8G8B8A8Unorm,
                    format: format,
                    samples: 1,
                }
            },
            pass: {
                color: [color],
                depth_stencil: {}
            }
        )
        .unwrap(),
    );
    RenderDetails {
        swapchain,
        swapchain_images: images,
        render_pass,
        dimensions,
    }
}

fn init_vulkan<Ext: Into<RawInstanceExtensions>>(
    ext: Ext,
) -> (Arc<Instance>, Arc<Device>, Arc<Queue>) {
    let instance = Instance::new(None, ext, None).expect("failed to create Vulkan instance");

    for physical_device in PhysicalDevice::enumerate(&instance) {
        debug!(
            "Physical device: {} / {:?}\n  type: {:?}\n  API version: {:?}",
            physical_device.name(),
            physical_device,
            physical_device.ty(),
            physical_device.api_version(),
        );
    }

    let physical_device = PhysicalDevice::enumerate(&instance)
        .next()
        .expect("Failed to select Vulkan physical device");
    debug!("Selected first device: {:?}", physical_device);

    for family in physical_device.queue_families() {
        debug!(
            "Found a queue family with {:?} queue(s); \
             supports_graphics = {:?}; supports_compute = {:?}",
            family.queues_count(),
            family.supports_graphics(),
            family.supports_compute(),
        );
    }

    let queue_family = physical_device
        .queue_families()
        .find(|&q| q.supports_graphics())
        .expect("Failed to find a queue family that supported graphics");

    let (device, queue) = {
        let device_extensions = vulkano::device::DeviceExtensions {
            khr_swapchain: true,
            ..vulkano::device::DeviceExtensions::none()
        };
        let (device, mut queues) = Device::new(
            physical_device,
            &Features::none(),
            &device_extensions,
            [(queue_family, 0.5)].iter().cloned(),
        )
        .expect("Failed to create Vulkan device");
        let queue = queues.next().unwrap();
        (device, queue)
    };

    info!("Vulkan initialized.");
    (instance, device, queue)
}
