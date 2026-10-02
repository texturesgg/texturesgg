//! Click-to-select: which packet, and so which textures, draw a pixel.
//!
//! A pick renders the scene again into an `R32Uint` target, clipped to one
//! pixel, with each packet's material program lowered to write its pick id
//! (see [`crate::shader::pick_shader`]). It shares the color pass's vertex
//! buffer, depth rules, culling, and TEV alpha, so it sees exactly the
//! animated pose and cut-outs on screen.
//!
//! Reading the pixel back is asynchronous, because a browser can't block on
//! the GPU: encode with [`HsdRenderer::encode_pick`], submit, call
//! [`PickReadback::map`], then poll the device until [`PendingPick::take`]
//! answers. Pick pipelines are built on the first pick, not at load.

use crate::error::{HsdRenderError, Result};
use crate::material::{PreparedMaterial, StageSource, TevStep, TevTarget};
use std::sync::{Arc, Mutex, PoisonError};

pub const PICK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;

/// One texture a picked packet samples.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedTexture {
    /// The material stage that samples it.
    pub stage: usize,
    /// Every `HsdScene::textures` index with these pixels.
    pub scene_textures: Vec<u32>,
    /// Projected by camera-space normals (an environment map) rather than
    /// laid out by texture coordinates.
    pub reflection: bool,
}

/// A pick encoded into a command encoder, waiting for its submission.
pub struct PickReadback {
    pub(crate) buffer: wgpu::Buffer,
}

type MapResult = std::result::Result<(), wgpu::BufferAsyncError>;

impl PickReadback {
    /// Start reading the pixel back. Call after submitting the encoder that
    /// holds the pick; the answer arrives as the device is polled.
    pub fn map(self) -> PendingPick {
        let state: Arc<Mutex<Option<MapResult>>> = Arc::default();
        let done = state.clone();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // Record the answer even through a poisoned lock; `take`
                // reports the poisoning rather than waiting forever.
                *done.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
            });
        PendingPick {
            buffer: self.buffer,
            state,
        }
    }
}

/// A pick being read back.
pub struct PendingPick {
    buffer: wgpu::Buffer,
    state: Arc<Mutex<Option<MapResult>>>,
}

impl PendingPick {
    /// The picked packet index (`None` for background), once the readback has
    /// arrived; `None` while it's still in flight.
    pub fn take(&self) -> Option<Result<Option<usize>>> {
        Some(take_mapped(&self.state)?.map(|()| {
            let id = {
                let mapped = self.buffer.slice(..).get_mapped_range();
                u32::from_le_bytes([mapped[0], mapped[1], mapped[2], mapped[3]])
            };
            self.buffer.unmap();
            id.checked_sub(1).map(|packet| packet as usize)
        }))
    }
}

/// The map result once it has arrived, `None` while in flight. A poisoned
/// lock is an error: waiting on it could never end.
fn take_mapped(state: &Mutex<Option<MapResult>>) -> Option<Result<()>> {
    let result = match state.lock() {
        Ok(mut state) => state.take()?,
        Err(_) => {
            return Some(Err(HsdRenderError::Gpu(
                "pick readback state was poisoned by a panic".into(),
            )));
        }
    };
    Some(result.map_err(|error| HsdRenderError::Gpu(error.to_string())))
}

/// The textures `material` samples, most defining first: stages that color
/// the surface through its texture coordinates, then specular stages, then
/// environment maps, each in TEV order. Stages without a decoded image are
/// left out.
pub(crate) fn stages_by_prominence(material: &PreparedMaterial) -> Vec<usize> {
    let mut ranked: Vec<(u8, usize)> = Vec::new();
    for step in &material.tev_plan {
        let TevStep::Stage { stage, target, .. } = *step else {
            continue;
        };
        if ranked.iter().any(|&(_, seen)| seen == stage) {
            continue;
        }
        let Some(prepared) = material.stages.get(stage) else {
            continue;
        };
        if prepared.texture_index.is_none() {
            continue;
        }
        let rank = match (prepared.source, target) {
            (StageSource::TexCoord(_), TevTarget::Color) => 0,
            (StageSource::TexCoord(_), TevTarget::Specular) => 1,
            (StageSource::Reflection, _) => 2,
        };
        ranked.push((rank, stage));
    }
    // Stable, so TEV order holds within a rank.
    ranked.sort_by_key(|&(rank, _)| rank);
    ranked.into_iter().map(|(_, stage)| stage).collect()
}

#[cfg(test)]
mod tests {
    use super::{MapResult, stages_by_prominence, take_mapped};
    use crate::error::HsdRenderError;
    use crate::material::test_support::{DIFFUSE, EXT, SPECULAR, material, stage};
    use crate::material::{StageSource, TevAlphaOp, TevColorOp};
    use std::sync::Mutex;

    #[test]
    fn surface_color_ranks_before_specular_and_reflections() {
        let texcoord = || {
            stage(
                StageSource::TexCoord(0),
                TevColorOp::Modulate,
                TevAlphaOp::None,
            )
        };
        let reflection = stage(StageSource::Reflection, TevColorOp::Add, TevAlphaOp::None);
        let mut undecoded = texcoord();
        undecoded.texture_index = None;
        let material = material(
            vec![reflection, texcoord(), texcoord(), undecoded, texcoord()],
            &[DIFFUSE, SPECULAR, EXT, DIFFUSE, DIFFUSE | EXT],
        );
        // Stages 4 (diffuse) and 2 (EXT) color the surface, 1 is specular,
        // 3 has no image, and 0 is an environment map.
        assert_eq!(stages_by_prominence(&material), [4, 2, 1, 0]);
    }

    #[test]
    fn a_pending_readback_waits_and_an_arrived_one_is_taken_once() {
        let state: Mutex<Option<MapResult>> = Mutex::new(None);
        assert!(take_mapped(&state).is_none());
        *state.lock().unwrap() = Some(Ok(()));
        assert!(matches!(take_mapped(&state), Some(Ok(()))));
        assert!(take_mapped(&state).is_none());
        *state.lock().unwrap() = Some(Err(wgpu::BufferAsyncError));
        assert!(matches!(
            take_mapped(&state),
            Some(Err(HsdRenderError::Gpu(_)))
        ));
    }

    #[test]
    fn a_poisoned_readback_is_an_error_not_still_pending() {
        let state: Mutex<Option<MapResult>> = Mutex::new(None);
        std::thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _guard = state.lock().unwrap();
                    panic!("poison the pick state");
                })
                .join();
        });
        assert!(state.is_poisoned());
        assert!(matches!(
            take_mapped(&state),
            Some(Err(HsdRenderError::Gpu(_)))
        ));
    }
}
