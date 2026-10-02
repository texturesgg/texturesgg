//! Errors raised while preparing or rendering HSD draw work.

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
        "scene texture {scene_texture} update is {actual_width}x{actual_height} \
         ({bytes} bytes), but the texture is {width}x{height}"
    )]
    TextureSizeMismatch {
        scene_texture: u32,
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
    #[error("GPU operation failed: {0}")]
    Gpu(String),
    #[error(transparent)]
    Source(#[from] dat_parser::hsd::source::HsdSourceError),
}

pub type Result<T> = std::result::Result<T, HsdRenderError>;

pub(crate) fn invalid_scene<T>(message: impl Into<String>) -> Result<T> {
    Err(HsdRenderError::InvalidScene(message.into()))
}

pub(crate) fn invalid_draw_work<T>(message: impl Into<String>) -> Result<T> {
    Err(HsdRenderError::InvalidDrawWork(message.into()))
}
