//! Headless device creation and texture readback for captures and tests.

use crate::error::{GpuError, Result};
use crate::geometry::PacketIndex;
use crate::renderer::HsdRenderer;

/// Captures use a raw 8-bit target, like the site's non-sRGB canvas.
pub const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Request a headless adapter. `software` asks for a CPU fallback adapter
    /// (lavapipe/llvmpipe on Linux) for reproducible captures.
    pub fn request(software: bool) -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: software,
            compatible_surface: None,
        }))
        .map_err(GpuError::from)?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("hsd-render offscreen"),
            ..Default::default()
        }))
        .map_err(GpuError::from)?;
        Ok(Self {
            adapter,
            device,
            queue,
        })
    }
}

pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Render one frame offscreen, at the renderer's size, and read it back as
/// opaque RGBA8.
///
/// Alpha is forced to 255, matching an `alphaMode: "opaque"` canvas.
pub fn capture(gpu: &Gpu, renderer: &HsdRenderer) -> Result<RgbaImage> {
    let Gpu { device, queue, .. } = gpu;
    let (width, height) = renderer.size();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hsd-render capture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: CAPTURE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let bytes_per_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hsd-render readback"),
        size: u64::from(bytes_per_row) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.encode(&mut encoder, &target.create_view(&Default::default()));
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(GpuError::from(error).into());
    }

    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(GpuError::from)?;
    receiver
        .recv()
        .map_err(|_| GpuError::ReadbackLost)?
        .map_err(GpuError::from)?;
    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for row in mapped.chunks_exact(bytes_per_row as usize) {
        for pixel in row[..width as usize * 4].as_chunks::<4>().0 {
            pixels.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    drop(mapped);
    readback.unmap();
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// Pick the packet drawing device pixel `(x, y)`, blocking until the readback
/// arrives. `None` is background.
pub fn pick(gpu: &Gpu, renderer: &mut HsdRenderer, x: u32, y: u32) -> Result<Option<PacketIndex>> {
    let Gpu { device, queue, .. } = gpu;
    let mut encoder = device.create_command_encoder(&Default::default());
    let readback = renderer.encode_pick(device, &mut encoder, (x, y))?;
    queue.submit([encoder.finish()]);
    let pending = readback.map();
    loop {
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(GpuError::from)?;
        if let Some(picked) = pending.take() {
            return picked;
        }
    }
}
