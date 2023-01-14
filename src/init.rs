use std::borrow::Cow;
use std::cmp::{max, min};
use std::convert::{TryFrom, TryInto};
use std::iter::FromIterator;
use std::mem::ManuallyDrop;
use std::sync::Arc;

use ash::vk::Handle as AshHandle;
use log::{debug, info, trace};
use sdl2::video::Window;
use smallvec::SmallVec;
use uuid::Uuid;
use vulkano::device::{Device, Features, Queue, QueueCreateInfo};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{self, Instance, InstanceExtensions};
use vulkano::library::VulkanLibrary;
use vulkano::render_pass::RenderPass;
use vulkano::swapchain::{Surface, SurfaceApi, Swapchain, SwapchainCreateInfo};
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

pub fn init_sdl_and_vulkan(select_device: Option<Uuid>) -> Init {
    let sdl_context = sdl2::init().expect("Failed to initialize SDL.");
    debug!("SDL initialized.");

    // Event pump
    let event_pump = sdl_context.event_pump().unwrap();

    let video_subsystem = sdl_context.video().unwrap();
    trace!("SDL video subsystem initialized.");
    let window = video_subsystem
        .window("Voxel", 800, 600)
        .vulkan()
        .resizable()
        .build()
        .unwrap();
    trace!("SDL window created.");
    let instance_extensions = window.vulkan_instance_extensions().unwrap();
    let instance_extensions = InstanceExtensions::from_iter(instance_extensions);

    let (instance, device, queue) = init_vulkan(instance_extensions, select_device);

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
        // FIXME: what is the right value here, and why?
        let api = SurfaceApi::DisplayPlane;
        unsafe { Surface::from_handle(instance_clone, ash_surface, api, ()) }
    };
    trace!("Vulkan Surface created from SDL surface.");

    // Finish
    info!("SDL & Vulkan initialized.");

    let surface = ManuallyDrop::new(Arc::new(surface));
    // NOTE: Do not add failures / exits from here to function end.

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

pub struct RenderDetails {
    pub swapchain: Arc<Swapchain<()>>,
    pub swapchain_images: Vec<Arc<SwapchainImage<()>>>,
    pub render_pass: Arc<RenderPass>,
}

#[derive(Debug, thiserror::Error)]
pub enum RenderDetailsError {
    #[error("failed to query surface capabilities: {0}")]
    FailedToQuerySurfaceCapabilities(vulkano::device::physical::PhysicalDeviceError),
    #[error("failed to query surface formats: {0}")]
    FailedToQuerySurfaceFormats(vulkano::device::physical::PhysicalDeviceError),
    #[error("the surface's .current_extent was None; we expect the surface to have an extent")]
    ExpectedSurfaceToHaveExtent,
    #[error("failed to create Swapchain: {0}")]
    FailedToCreateSwapchain(vulkano::swapchain::SwapchainCreationError),
    #[error("failed to create RenderPass: {0}")]
    FailedToCreateRenderPass(vulkano::render_pass::RenderPassCreationError),
}

impl RenderDetails {
    pub fn init(
        device: Arc<Device>,
        surface: Arc<Surface<()>>,
    ) -> Result<RenderDetails, RenderDetailsError> {
        info!("Creating RenderDetails…");

        // Swapchain
        let (swapchain, images, format) = {
            trace!("Querying surface capabilities");
            let caps = device
                .physical_device()
                .surface_capabilities(&surface, Default::default())
                .map_err(RenderDetailsError::FailedToQuerySurfaceCapabilities)?;
            let supported_formats = device
                .physical_device()
                .surface_formats(&surface, Default::default())
                .map_err(RenderDetailsError::FailedToQuerySurfaceFormats)?;

            debug!("Supported formats");
            for supported_format in &supported_formats {
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
            let (format, color_space) = supported_formats[0];
            debug!("[TODO] Selected first format: {:?}", (format, color_space));

            // TODO: figure this out
            // The created swapchain will be used as a color attachment for rendering.
            let usage = ImageUsage {
                color_attachment: true,
                ..ImageUsage::empty()
            };

            let swapchain_create_info = SwapchainCreateInfo {
                min_image_count: buffers_count,
                image_format: Some(format),
                image_color_space: color_space,
                image_usage: usage,
                ..Default::default()
            };
            let (swapchain, images) =
                Swapchain::new(device.clone(), surface, swapchain_create_info)
                    .map_err(RenderDetailsError::FailedToCreateSwapchain)?;

            (swapchain, images, format)
        };

        // Render pass
        let render_pass = vulkano::single_pass_renderpass!(
            device,
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
        .map_err(RenderDetailsError::FailedToCreateRenderPass)?;

        Ok(RenderDetails {
            swapchain,
            swapchain_images: images,
            render_pass,
        })
    }

    pub fn recreate_swapchain(
        &mut self,
        init: &Init,
    ) -> Result<bool, vulkano::swapchain::SwapchainCreationError> {
        debug!("Recreating swap chain");
        let create_info = self.swapchain.create_info();
        match self.swapchain.recreate(create_info) {
            Ok((new_swapchain, new_images)) => {
                self.swapchain = new_swapchain;
                self.swapchain_images = new_images;
                Ok(true)
            }
            // These happen. Examples ignore them. What exactly is going on here?
            //Err(vulkano::swapchain::SwapchainCreationError::UnsupportedDimensions) => Ok(false),
            Err(err) => Err(err),
        }
    }
}

fn init_vulkan(
    ext: InstanceExtensions,
    select_device: Option<Uuid>,
) -> (Arc<Instance>, Arc<Device>, Arc<Queue>) {
    let vk_library = VulkanLibrary::new().expect("failed to init VulkanLibrary");
    let instance = Instance::new(
        vk_library,
        instance::InstanceCreateInfo {
            application_name: Some("Voxel".to_owned()),
            // TODO: manage this somehow?
            application_version: instance::Version {
                major: 1,
                minor: 0,
                patch: 0,
            },
            enabled_extensions: ext,
            enabled_layers: Vec::new(),
            engine_name: None,
            engine_version: Default::default(),
            max_api_version: Default::default(),
            ..Default::default()
        },
    )
    .expect("failed to create Vulkan instance");

    let physical_devices = instance
        .enumerate_physical_devices()
        .expect("failed to enumerate physical devices")
        .collect::<SmallVec<[_; 2]>>();
    for physical_device in physical_devices.iter() {
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
        physical_devices
            .into_iter()
            .filter(|pd| {
                pd.properties()
                    .device_uuid
                    .as_ref()
                    .map(|id| Uuid::from_slice(id).unwrap())
                    == Some(id)
            })
            .next()
    } else {
        physical_devices.into_iter().next()
    };
    let physical_device = physical_device.expect("Failed to select Vulkan physical device");
    debug!("Selected first device: {:?}", physical_device);

    for family in physical_device.queue_family_properties() {
        debug!(
            "Found a queue family with {:?} queue(s); \
             supports_graphics = {:?}; supports_compute = {:?}",
            family.queue_count,
            family.queue_flags.graphics,
            family.queue_flags.compute,
        );
    }

    let queue_family_index = physical_device
        .queue_family_properties()
        .iter()
        .position(|q| q.queue_flags.graphics)
        .expect("Failed to find a queue family that supported graphics");

    let (device, queue) = {
        let device_extensions = vulkano::device::DeviceExtensions {
            khr_swapchain: true,
            ..vulkano::device::DeviceExtensions::empty()
        };
        let (device, mut queues) = Device::new(
            physical_device,
            vulkano::device::DeviceCreateInfo {
                enabled_extensions: device_extensions,
                enabled_features: Features::empty(),
                queue_create_infos: vec![QueueCreateInfo {
                    queue_family_index: queue_family_index.try_into().unwrap(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .expect("Failed to create Vulkan device");
        let queue = queues.next().unwrap();
        (device, queue)
    };

    info!("Vulkan initialized.");
    (instance, device, queue)
}
