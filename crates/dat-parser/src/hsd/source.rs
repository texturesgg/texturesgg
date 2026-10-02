//! A DAT loaded for drawing: the parsed scene and a prepared draw-work
//! evaluator, within the budgets of [`hsd_scene_limits`].

use crate::hsd::HsdScene;
use crate::hsd::draw::{
    HsdDrawEvaluationPolicy, HsdDrawWorkError, HsdDrawWorkEvaluator, HsdDrawWorkLimits,
    HsdEvaluatedDrawWork,
};
use crate::hsd::scene::{HSD_SCENE_MAX_DAT_BYTES, HsdSceneError, MObjId, hsd_scene_limits};
use crate::{DatFile, DatParseError};

/// Why a DAT did not load for drawing. Each variant carries the error of the
/// stage that refused it, so a host can tell an oversized file from a
/// truncated descriptor from a budget from state no contract represents.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HsdSourceError {
    #[error("the DAT is {len} bytes, over the scene input budget of {limit}")]
    TooLarge { len: usize, limit: usize },
    #[error("the DAT does not parse: {0}")]
    Parse(#[from] DatParseError),
    #[error("HSD scene is invalid: {0}")]
    InvalidScene(#[from] HsdSceneError),
    /// A material's render mode names no display pass (NO_ZUPDATE without
    /// XLU), so a renderer has nowhere to draw its display object.
    #[error("MObj {:#x} render mode sets NO_ZUPDATE without XLU", .material.0)]
    NoDrawPass { material: MObjId },
    #[error("HSD draw work is invalid: {0}")]
    InvalidDrawWork(#[from] HsdDrawWorkError),
}

pub type Result<T> = std::result::Result<T, HsdSourceError>;

/// What a viewer should frame when that is less than the whole scene: an
/// upright rectangle facing +Z, centered on `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsdFocus {
    pub center: [f32; 3],
    pub half_width: f32,
    pub half_height: f32,
}

pub struct HsdSource {
    pub policy: HsdDrawEvaluationPolicy,
    /// The part of the scene a viewer should frame, when the loader knows of
    /// one (a stage's camera range). `None` frames the whole scene.
    pub focus: Option<HsdFocus>,
    pub scene: HsdScene,
    pub evaluator: HsdDrawWorkEvaluator,
}

impl HsdSource {
    pub fn from_dat(bytes: &[u8], policy: HsdDrawEvaluationPolicy) -> Result<Self> {
        Self::from_parsed(&parse(bytes)?, policy)
    }

    /// [`Self::from_dat`] for a DAT [`parse`] already read.
    pub fn from_parsed(dat: &DatFile, policy: HsdDrawEvaluationPolicy) -> Result<Self> {
        let limits = hsd_scene_limits();
        let scene = HsdScene::from_dat_with_limits(dat, limits)?;
        scene.validate()?;
        require_draw_passes(&scene)?;
        let evaluator = HsdDrawWorkEvaluator::prepare_with_limits(
            &scene,
            policy,
            HsdDrawWorkLimits {
                max_joint_occurrences: limits.max_joints,
                max_packets: limits.max_polygons,
                max_vertices: limits.max_vertices,
            },
        )?;
        Ok(Self {
            policy,
            focus: None,
            scene,
            evaluator,
        })
    }

    /// Evaluate the bind pose; returns the scene with its draw work.
    pub fn evaluate_bind_pose(&mut self) -> Result<(&HsdScene, &HsdEvaluatedDrawWork)> {
        let work = self.evaluator.evaluate_bind_pose(&self.scene)?;
        Ok((&self.scene, work))
    }
}

/// Parse a DAT no larger than [`HSD_SCENE_MAX_DAT_BYTES`].
pub fn parse(bytes: &[u8]) -> Result<DatFile> {
    if bytes.len() > HSD_SCENE_MAX_DAT_BYTES {
        return Err(HsdSourceError::TooLarge {
            len: bytes.len(),
            limit: HSD_SCENE_MAX_DAT_BYTES,
        });
    }
    Ok(DatFile::parse(bytes)?)
}

/// Every display object must name a display pass: a renderer has nowhere to
/// draw one whose render mode sets NO_ZUPDATE without XLU.
fn require_draw_passes(scene: &HsdScene) -> Result<()> {
    let display_objects = scene
        .roots
        .iter()
        .flat_map(|root| &root.joints)
        .flat_map(|joint| &joint.display_objects);
    for display_object in display_objects {
        // Without a material the render mode is zero, which names a pass.
        if let Some(material) = &display_object.material
            && display_object.pass().is_none()
        {
            return Err(HsdSourceError::NoDrawPass {
                material: material.source_id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsd::scene::{
        DObjId, HsdDisplayObject, HsdJoint, HsdMaterial, HsdSceneRoot, HsdTransform, JObjId,
    };

    fn scene(render_flags: u32) -> HsdScene {
        HsdScene {
            roots: vec![HsdSceneRoot {
                source_id: JObjId(0),
                name: None,
                joints: vec![HsdJoint {
                    source_id: JObjId(0),
                    parent: None,
                    children: Vec::new(),
                    flags: 0,
                    local: HsdTransform {
                        scale: [1.0; 3],
                        rotation: [0.0; 3],
                        translation: [0.0; 3],
                    },
                    inverse_bind_transform: None,
                    display_objects: vec![HsdDisplayObject {
                        source_id: DObjId(4),
                        material: Some(HsdMaterial {
                            source_id: MObjId(0x10),
                            render_flags,
                            custom_pe: None,
                            colors: None,
                            textures: Vec::new(),
                        }),
                        polygons: Vec::new(),
                    }],
                }],
            }],
            textures: Vec::new(),
        }
    }

    #[test]
    fn a_render_mode_with_no_display_pass_fails_the_model() {
        assert!(require_draw_passes(&scene(0x6000_0000)).is_ok());
        // NO_ZUPDATE without XLU.
        assert!(matches!(
            require_draw_passes(&scene(0x2000_0000)),
            Err(HsdSourceError::NoDrawPass {
                material: MObjId(0x10)
            })
        ));
    }
}
