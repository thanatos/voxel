use std::borrow::Cow;
use std::cmp::{max, min};
use std::convert::TryFrom;
use std::ffi::CString;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use log::{debug, info, trace};
use sdl2::video::Window;
use uuid::Uuid;
use vulkano::device::{Device, Features, Queue, physical::PhysicalDevice};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{self, Instance, InstanceExtensions};
use vulkano::render_pass::RenderPass;
use vulkano::swapchain::{Surface, Swapchain};
use vulkano::{Handle, VulkanObject};

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

pub fn init_sdl_and_vulkan(select_device: Option<Uuid>) -> Init {
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
    let instance_extensions = window
        .vulkan_instance_extensions()
        .unwrap()
        .into_iter()
        .map(|v| CString::new(v).unwrap())
        .collect::<Vec<_>>();
    let instance_extensions =
        InstanceExtensions::from(instance_extensions.iter().map(|v| v.as_c_str()));

    let (instance, device, queue) = init_vulkan(&instance_extensions, select_device);

    trace!("Creating surface in SDL.");
    let surface_handle = window
        .vulkan_create_surface({
            // FIXME: `ash`, which is the raw-bindings that Vulkano uses internally, thinks all
            // Vulkan handles, like `Instance`, are `u64`. They're not: they're opaque pointers.
            // `usize` would be a more appropriate type, and `u64` is flat out wrong on 32-bit
            // platforms.
            let ash_handle = instance.internal_object().as_raw();
            usize::try_from(ash_handle).expect("this should never fail")
        })
        .unwrap();
    trace!("Surface created in SDL.");
    let surface = {
        let ash_surface = ash::vk::SurfaceKHR::from_raw(surface_handle);
        let instance_clone = instance.clone();
        unsafe { Surface::from_raw_surface(instance_clone, ash_surface, ()) }
    };
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
        debug!("[TODO] Selected first format: {:?}", (format, color_space));

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

fn init_vulkan(
    ext: &InstanceExtensions,
    select_device: Option<Uuid>,
) -> (Arc<Instance>, Arc<Device>, Arc<Queue>) {
    let instance = Instance::new(None, instance::Version::V1_1, ext, None)
        .expect("failed to create Vulkan instance");

    for physical_device in PhysicalDevice::enumerate(&instance) {
        let properties = physical_device.properties();
        let device_id = match properties.device_uuid {
            Some(b) => Cow::from(Uuid::from_slice(&b).unwrap().to_string()),
            None => Cow::from("None"),
        };
        debug!(
            "Physical device: {} / {:?}\n  ID: {}\n  type: {:?}\n  API version: {:?}",
            properties.device_name,
            physical_device,
            device_id,
            properties.device_type,
            physical_device.api_version(),
        );
    }

    let physical_device = if let Some(id) = select_device {
        PhysicalDevice::enumerate(&instance)
            .filter(|pd| {
                pd.properties()
                    .device_uuid
                    .map(|id| Uuid::from_slice(&id).unwrap())
                    == Some(id)
            })
            .next()
    } else {
        PhysicalDevice::enumerate(&instance).next()
    };
    let physical_device = physical_device.expect("Failed to select Vulkan physical device");
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
