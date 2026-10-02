//! wgpu renderer for dat-parser's evaluated HSD draw work.
//!
//! It draws the scene and evaluated draw work that `dat-parser` produces: a
//! caller poses a model (with `melee-dat`, for a Melee fighter or stage) and
//! hands each frame here. Parsing, evaluation and game semantics stay in those
//! crates; this one owns only GPU lowering, and borrows the caller's device.
//! The pixel baseline guards regressions; the Melee decompilation and Dolphin
//! captures are the authority.

pub mod camera;
pub mod error;
pub mod geometry;
pub mod lighting;
pub mod material;
/// Blocks on the GPU, so it is native-only: a browser can't block.
#[cfg(not(target_family = "wasm"))]
pub mod offscreen;
pub mod pick;
pub mod renderer;
pub mod shader;

pub use camera::{Camera, CameraView, Focus, Orbit};
pub use error::{HsdRenderError, Result};
pub use geometry::PreparedGeometry;
pub use lighting::{HsdLightingPreset, neutral_preview_lighting};
pub use pick::{PendingPick, PickReadback, PickedTexture};
pub use renderer::HsdRenderer;
