//! Errors raised while preparing or rendering HSD draw work.

use crate::geometry::PacketIndex;
use crate::renderer::ModelId;
use dat_parser::hsd::scene::HsdTextureIndex;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HsdRenderError {
    #[error("HSD scene is invalid: {0}")]
    InvalidScene(String),
    #[error("HSD draw work is invalid: {0}")]
    InvalidDrawWork(String),
    #[error("HSD geometry exceeds the {label} budget ({actual} > {maximum})")]
    ResourceLimit {
        label: &'static str,
        maximum: usize,
        actual: usize,
    },
    #[error("HSD geometry has no renderable triangles")]
    EmptyGeometry,
    #[error("lighting preset is invalid: {0}")]
    InvalidLighting(String),
    #[error(
        "scene texture {} update is {actual_width}x{actual_height} \
         ({bytes} bytes), but the texture is {width}x{height}",
        .scene_texture.0
    )]
    TextureSizeMismatch {
        scene_texture: HsdTextureIndex,
        width: u32,
        height: u32,
        actual_width: u32,
        actual_height: u32,
        bytes: usize,
    },
    #[error("pick ({x}, {y}) is outside the {width}x{height} target")]
    PickOutOfBounds {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    #[error("HSD output is raw GX color and needs a non-sRGB target, not {0:?}")]
    SrgbTarget(wgpu::TextureFormat),
    #[error("render size {width}x{height} is outside 1..={maximum}")]
    RenderSize {
        width: u32,
        height: u32,
        maximum: u32,
    },
    #[error("packet {} is not in the prepared geometry", .0.0)]
    UnknownPacket(PacketIndex),
    #[error("{0:?} is not one of the renderer's models")]
    UnknownModel(ModelId),
    #[error("GPU operation failed: {0}")]
    Gpu(#[from] GpuError),
    #[error(transparent)]
    Source(#[from] dat_parser::hsd::source::HsdSourceError),
}

/// What the GPU, or the wait on it, reported.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GpuError {
    /// A validation or out-of-memory error from an error scope.
    #[error(transparent)]
    Device(#[from] wgpu::Error),
    #[error("no compatible adapter: {0}")]
    NoAdapter(#[from] wgpu::RequestAdapterError),
    #[error("device request failed: {0}")]
    NoDevice(#[from] wgpu::RequestDeviceError),
    #[error(transparent)]
    Poll(#[from] wgpu::PollError),
    #[error(transparent)]
    Readback(#[from] wgpu::BufferAsyncError),
    /// The readback's answer was lost: its sender went away, or a panic
    /// poisoned the state it was left in.
    #[error("the readback was abandoned before it answered")]
    ReadbackLost,
}

pub type Result<T> = std::result::Result<T, HsdRenderError>;

pub(crate) fn invalid_scene<T>(message: impl Into<String>) -> Result<T> {
    Err(HsdRenderError::InvalidScene(message.into()))
}

pub(crate) fn invalid_draw_work<T>(message: impl Into<String>) -> Result<T> {
    Err(HsdRenderError::InvalidDrawWork(message.into()))
}
