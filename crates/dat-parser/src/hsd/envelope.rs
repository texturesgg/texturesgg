//! Renderer-neutral HSD envelope matrix construction and vertex deformation.
//!
//! The evaluator mirrors HSD's bounded ten-entry GX position/normal matrix
//! palette without selecting models, cameras, materials, or backend resources.
//!
//! Matrices and deformed vertices are world-space. View matrices and Melee's
//! view-dependent `ft_jobj_scale` normal correction remain draw-state inputs;
//! this layer does not silently fold either into model semantics.

use crate::hsd::scene::{HsdEnvelope, JObjId};
use crate::math::Mat4;
use thiserror::Error;

/// GX exposes ten position/normal matrix slots to HSD's envelope draw path.
pub const HSD_ENVELOPE_PALETTE_LEN: usize = 10;

pub(crate) const HSD_MATRIX_INVERSE_EPSILON: f32 = 1.0e-10;

/// Runtime matrices associated with one serialized envelope joint reference.
#[derive(Clone, Copy, Debug)]
pub struct HsdEnvelopeJointMatrices {
    pub current_world: Mat4,
    pub inverse_bind: Option<Mat4>,
}
/// Matching source paths use slightly different full-weight fast-path tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HsdEnvelopeSingleWeightPolicy {
    /// `SetupEnvelopeModelMtx`: `weight >= 1.0 - FLT_EPSILON`.
    GenericHsd,
    /// `ftPartsSetupEnvelopeMtx`: `weight >= 1.0`.
    MeleeFighter,
}

impl HsdEnvelopeSingleWeightPolicy {
    fn selects_full_weight(self, weight: f32) -> bool {
        match self {
            Self::GenericHsd => weight >= 1.0 - f32::EPSILON,
            Self::MeleeFighter => weight >= 1.0,
        }
    }
}

/// Inverse-transpose 3x3 normal matrix, stored column-major.
#[derive(Clone, Copy, Debug)]
pub struct HsdNormalMatrix([[f32; 3]; 3]);

impl HsdNormalMatrix {
    fn transform(self, normal: [f32; 3]) -> [f32; 3] {
        [
            self.0[0][0] * normal[0] + self.0[1][0] * normal[1] + self.0[2][0] * normal[2],
            self.0[0][1] * normal[0] + self.0[1][1] * normal[1] + self.0[2][1] * normal[2],
            self.0[0][2] * normal[0] + self.0[1][2] * normal[1] + self.0[2][2] * normal[2],
        ]
    }
}

/// World-space position and normal matrices for one HSD/GX palette entry.
#[derive(Clone, Copy, Debug)]
pub struct HsdEnvelopeMatrix {
    position: Mat4,
    normal: HsdNormalMatrix,
}

impl HsdEnvelopeMatrix {
    const IDENTITY: Self = Self {
        position: Mat4([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]),
        normal: HsdNormalMatrix([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
    };

    /// Build the rigid-path world matrices from a current world matrix.
    pub fn from_rigid(current_world: Mat4) -> Result<Self, HsdEnvelopeEvaluationError> {
        if !matrix_is_finite(current_world) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteRigidMatrix);
        }
        let normal = inverse_transpose(current_world)
            .ok_or(HsdEnvelopeEvaluationError::NonFiniteRigidNormalMatrix)?;
        Ok(Self {
            position: current_world,
            normal,
        })
    }

    pub fn position_matrix(&self) -> Mat4 {
        self.position
    }

    /// Apply the inverse-transpose normal matrix without normalization.
    ///
    /// GX emboss texgen preserves the transformed binormal and tangent
    /// magnitudes when projecting the selected light direction.
    pub fn transform_normal_basis(
        &self,
        direction: [f32; 3],
    ) -> Result<[f32; 3], HsdEnvelopeEvaluationError> {
        if !vector_is_finite(direction) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteVertexInput);
        }
        let transformed = self.normal.transform(direction);
        if !vector_is_finite(transformed) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteVertexOutput);
        }
        Ok(transformed)
    }

    /// Apply the position matrix and normalized inverse-transpose normal matrix.
    ///
    /// A zero source normal is the decoded sentinel for geometry without a GX
    /// normal attribute and remains zero. A nonzero normal that collapses under
    /// the matrix is rejected.
    pub fn deform(
        &self,
        position: [f32; 3],
        normal: [f32; 3],
    ) -> Result<HsdDeformedVertex, HsdEnvelopeEvaluationError> {
        if !vector_is_finite(position) || !vector_is_finite(normal) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteVertexInput);
        }

        let position = self.position.transform_point(position);
        let transformed_normal = self.normal.transform(normal);
        if !vector_is_finite(position) || !vector_is_finite(transformed_normal) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteVertexOutput);
        }

        let source_has_normal = normal.into_iter().any(|component| component != 0.0);
        let normal = if source_has_normal {
            let length_squared = transformed_normal[0] * transformed_normal[0]
                + transformed_normal[1] * transformed_normal[1]
                + transformed_normal[2] * transformed_normal[2];
            if !length_squared.is_finite() {
                return Err(HsdEnvelopeEvaluationError::NonFiniteVertexOutput);
            }
            if length_squared == 0.0 {
                return Err(HsdEnvelopeEvaluationError::DegenerateNormal);
            }
            let inverse_length = length_squared.sqrt().recip();
            [
                transformed_normal[0] * inverse_length,
                transformed_normal[1] * inverse_length,
                transformed_normal[2] * inverse_length,
            ]
        } else {
            [0.0; 3]
        };
        if !vector_is_finite(normal) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteVertexOutput);
        }

        Ok(HsdDeformedVertex { position, normal })
    }
}

/// A bounded HSD envelope palette. Unused GX slots are never addressable.
#[derive(Debug)]
pub(crate) struct HsdEnvelopePalette {
    matrices: [HsdEnvelopeMatrix; HSD_ENVELOPE_PALETTE_LEN],
    len: usize,
}

impl HsdEnvelopePalette {
    /// Build HSD's envelope matrices from source-ordered palette entries.
    ///
    /// `single_weight_policy` makes the matching generic/fighter threshold
    /// difference explicit. `model_node_correction` is the optional matrix
    /// returned by `_HSD_mkEnvelopeModelNodeMtx`; it is `None` for a
    /// skeleton-root model node. The resolver must provide current world and
    /// serialized inverse-bind matrices for referenced joints. View and
    /// fighter-specific draw-state normal corrections are intentionally absent.
    pub(crate) fn build<F>(
        entries: &[HsdEnvelope],
        single_weight_policy: HsdEnvelopeSingleWeightPolicy,
        model_node_correction: Option<Mat4>,
        mut resolve_joint: F,
    ) -> Result<Self, HsdEnvelopeEvaluationError>
    where
        F: FnMut(JObjId) -> Option<HsdEnvelopeJointMatrices>,
    {
        if entries.is_empty() {
            return Err(HsdEnvelopeEvaluationError::EmptyPalette);
        }
        if entries.len() > HSD_ENVELOPE_PALETTE_LEN {
            return Err(HsdEnvelopeEvaluationError::PaletteTooLarge {
                count: entries.len(),
                limit: HSD_ENVELOPE_PALETTE_LEN,
            });
        }
        if model_node_correction.is_some_and(|matrix| !matrix_is_finite(matrix)) {
            return Err(HsdEnvelopeEvaluationError::NonFiniteModelNodeCorrection);
        }

        let mut matrices = [HsdEnvelopeMatrix::IDENTITY; HSD_ENVELOPE_PALETTE_LEN];
        for (palette_index, entry) in entries.iter().enumerate() {
            let first = entry
                .weights
                .first()
                .ok_or(HsdEnvelopeEvaluationError::EmptyEnvelope { palette_index })?;
            if !first.weight.is_finite() {
                return Err(HsdEnvelopeEvaluationError::NonFiniteWeight {
                    palette_index,
                    influence_index: 0,
                });
            }

            let position = if single_weight_policy.selects_full_weight(first.weight) {
                let joint =
                    resolve_required_joint(palette_index, 0, first.joint, &mut resolve_joint)?;
                if let Some(correction) = model_node_correction {
                    let inverse_bind = required_inverse_bind(palette_index, 0, first.joint, joint)?;
                    joint.current_world.mul(&inverse_bind).mul(&correction)
                } else {
                    joint.current_world
                }
            } else {
                let mut blended = Mat4::zero();
                for (influence_index, influence) in entry.weights.iter().enumerate() {
                    if !influence.weight.is_finite() {
                        return Err(HsdEnvelopeEvaluationError::NonFiniteWeight {
                            palette_index,
                            influence_index,
                        });
                    }
                    let joint = resolve_required_joint(
                        palette_index,
                        influence_index,
                        influence.joint,
                        &mut resolve_joint,
                    )?;
                    let inverse_bind = required_inverse_bind(
                        palette_index,
                        influence_index,
                        influence.joint,
                        joint,
                    )?;
                    blended.add_scaled(&joint.current_world.mul(&inverse_bind), influence.weight);
                }
                if let Some(correction) = model_node_correction {
                    blended.mul(&correction)
                } else {
                    blended
                }
            };

            if !matrix_is_finite(position) {
                return Err(HsdEnvelopeEvaluationError::NonFiniteComposedMatrix { palette_index });
            }
            let normal = inverse_transpose(position).ok_or(
                HsdEnvelopeEvaluationError::NonFiniteComposedNormalMatrix { palette_index },
            )?;
            matrices[palette_index] = HsdEnvelopeMatrix { position, normal };
        }

        Ok(Self {
            matrices,
            len: entries.len(),
        })
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Resolve GX's position/normal matrix-row address (`0, 3, ... 27`).
    pub fn matrix_for_selector(
        &self,
        pn_mtx_idx: u16,
    ) -> Result<&HsdEnvelopeMatrix, HsdEnvelopeEvaluationError> {
        if !pn_mtx_idx.is_multiple_of(3) {
            return Err(HsdEnvelopeEvaluationError::InvalidPaletteSelector { pn_mtx_idx });
        }
        let palette_index = usize::from(pn_mtx_idx / 3);
        self.matrices
            .get(palette_index)
            .filter(|_| palette_index < self.len)
            .ok_or(HsdEnvelopeEvaluationError::PaletteSelectorOutOfRange {
                pn_mtx_idx,
                palette_index,
                palette_len: self.len,
            })
    }

    #[cfg(test)]
    fn deform(
        &self,
        pn_mtx_idx: u16,
        position: [f32; 3],
        normal: [f32; 3],
    ) -> Result<HsdDeformedVertex, HsdEnvelopeEvaluationError> {
        self.matrix_for_selector(pn_mtx_idx)?
            .deform(position, normal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsdDeformedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum HsdEnvelopeEvaluationError {
    #[error("envelope palette is empty")]
    EmptyPalette,
    #[error("envelope palette has {count} entries; limit is {limit}")]
    PaletteTooLarge { count: usize, limit: usize },
    #[error("envelope palette entry {palette_index} has no influences")]
    EmptyEnvelope { palette_index: usize },
    #[error(
        "envelope palette entry {palette_index} influence {influence_index} references missing joint {source_id:#010x}"
    )]
    MissingJoint {
        palette_index: usize,
        influence_index: usize,
        source_id: u32,
    },
    #[error(
        "envelope palette entry {palette_index} influence {influence_index} has no inverse-bind matrix for joint {source_id:#010x}"
    )]
    MissingInverseBind {
        palette_index: usize,
        influence_index: usize,
        source_id: u32,
    },
    #[error(
        "envelope palette entry {palette_index} influence {influence_index} has a non-finite weight"
    )]
    NonFiniteWeight {
        palette_index: usize,
        influence_index: usize,
    },
    #[error(
        "envelope joint matrix is non-finite at palette entry {palette_index} influence {influence_index}"
    )]
    NonFiniteJointMatrix {
        palette_index: usize,
        influence_index: usize,
    },
    #[error("envelope model-node correction matrix is non-finite")]
    NonFiniteModelNodeCorrection,
    #[error("envelope palette entry {palette_index} produced a non-finite matrix")]
    NonFiniteComposedMatrix { palette_index: usize },
    #[error("envelope palette entry {palette_index} produced a non-finite normal matrix")]
    NonFiniteComposedNormalMatrix { palette_index: usize },
    #[error("rigid current-world matrix is non-finite")]
    NonFiniteRigidMatrix,
    #[error("rigid current-world matrix produces a non-finite normal matrix")]
    NonFiniteRigidNormalMatrix,
    #[error("GX position/normal matrix selector {pn_mtx_idx} is not divisible by three")]
    InvalidPaletteSelector { pn_mtx_idx: u16 },
    #[error(
        "GX position/normal matrix selector {pn_mtx_idx} resolves to palette entry {palette_index}, but palette length is {palette_len}"
    )]
    PaletteSelectorOutOfRange {
        pn_mtx_idx: u16,
        palette_index: usize,
        palette_len: usize,
    },
    #[error("vertex position or normal input is non-finite")]
    NonFiniteVertexInput,
    #[error("deformed vertex position or normal is non-finite")]
    NonFiniteVertexOutput,
    #[error("normal cannot be normalized because its transformed length is zero")]
    DegenerateNormal,
}

fn resolve_required_joint<F>(
    palette_index: usize,
    influence_index: usize,
    source_id: JObjId,
    resolve_joint: &mut F,
) -> Result<HsdEnvelopeJointMatrices, HsdEnvelopeEvaluationError>
where
    F: FnMut(JObjId) -> Option<HsdEnvelopeJointMatrices>,
{
    let joint = resolve_joint(source_id).ok_or(HsdEnvelopeEvaluationError::MissingJoint {
        palette_index,
        influence_index,
        source_id: source_id.0,
    })?;
    if !matrix_is_finite(joint.current_world) {
        return Err(HsdEnvelopeEvaluationError::NonFiniteJointMatrix {
            palette_index,
            influence_index,
        });
    }
    Ok(joint)
}

fn required_inverse_bind(
    palette_index: usize,
    influence_index: usize,
    source_id: JObjId,
    joint: HsdEnvelopeJointMatrices,
) -> Result<Mat4, HsdEnvelopeEvaluationError> {
    let inverse_bind =
        joint
            .inverse_bind
            .ok_or(HsdEnvelopeEvaluationError::MissingInverseBind {
                palette_index,
                influence_index,
                source_id: source_id.0,
            })?;
    if !matrix_is_finite(inverse_bind) {
        return Err(HsdEnvelopeEvaluationError::NonFiniteJointMatrix {
            palette_index,
            influence_index,
        });
    }
    Ok(inverse_bind)
}

fn inverse_transpose(matrix: Mat4) -> Option<HsdNormalMatrix> {
    let a00 = matrix.0[0][0];
    let a01 = matrix.0[1][0];
    let a02 = matrix.0[2][0];
    let a10 = matrix.0[0][1];
    let a11 = matrix.0[1][1];
    let a12 = matrix.0[2][1];
    let a20 = matrix.0[0][2];
    let a21 = matrix.0[1][2];
    let a22 = matrix.0[2][2];

    let c00 = a11 * a22 - a12 * a21;
    let c01 = a12 * a20 - a10 * a22;
    let c02 = a10 * a21 - a11 * a20;
    let c10 = a02 * a21 - a01 * a22;
    let c11 = a00 * a22 - a02 * a20;
    let c12 = a01 * a20 - a00 * a21;
    let c20 = a01 * a12 - a02 * a11;
    let c21 = a02 * a10 - a00 * a12;
    let c22 = a00 * a11 - a01 * a10;

    let determinant = a00 * c00 + a01 * c01 + a02 * c02;
    if !determinant.is_finite() {
        return None;
    }
    if determinant.abs() < HSD_MATRIX_INVERSE_EPSILON {
        return Some(HsdNormalMatrix([
            [a00, a10, a20],
            [a01, a11, a21],
            [a02, a12, a22],
        ]));
    }
    let inverse_determinant = determinant.recip();
    let normal = HsdNormalMatrix([
        [
            c00 * inverse_determinant,
            c10 * inverse_determinant,
            c20 * inverse_determinant,
        ],
        [
            c01 * inverse_determinant,
            c11 * inverse_determinant,
            c21 * inverse_determinant,
        ],
        [
            c02 * inverse_determinant,
            c12 * inverse_determinant,
            c22 * inverse_determinant,
        ],
    ]);
    normal
        .0
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then_some(normal)
}

fn matrix_is_finite(matrix: Mat4) -> bool {
    matrix.0.iter().flatten().all(|value| value.is_finite())
}

fn vector_is_finite(vector: [f32; 3]) -> bool {
    vector.into_iter().all(f32::is_finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsd::scene::HsdWeight;

    fn envelope(weights: &[(u32, f32)]) -> HsdEnvelope {
        HsdEnvelope {
            source_offset: 0,
            weights: weights
                .iter()
                .map(|(joint, weight)| HsdWeight {
                    joint: JObjId(*joint),
                    weight: *weight,
                })
                .collect(),
        }
    }

    fn matrix(scale: [f32; 3], translation: [f32; 3]) -> Mat4 {
        Mat4::from_srt(scale, [0.0; 3], translation)
    }

    fn assert_vec3_near(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
        }
    }

    #[test]
    fn rigid_path_uses_inverse_transpose_and_normalizes() {
        let matrices = HsdEnvelopeMatrix::from_rigid(matrix([2.0, 1.0, 0.5], [4.0, 5.0, 6.0]))
            .expect("rigid matrices");
        let output = matrices
            .deform([1.0, 2.0, 3.0], [1.0, 1.0, 0.0])
            .expect("deformed vertex");

        assert_vec3_near(output.position, [6.0, 7.0, 7.5]);
        assert_vec3_near(output.normal, [0.447_213_6, 0.894_427_2, 0.0]);
    }

    #[test]
    fn weighted_path_blends_current_world_times_inverse_bind_without_renormalizing_weights() {
        let entries = [envelope(&[(4, 0.25), (8, 0.5)])];
        let palette = HsdEnvelopePalette::build(
            &entries,
            HsdEnvelopeSingleWeightPolicy::MeleeFighter,
            None,
            |joint| match joint.0 {
                4 => Some(HsdEnvelopeJointMatrices {
                    current_world: matrix([2.0, 1.0, 1.0], [8.0, 0.0, 0.0]),
                    inverse_bind: Some(matrix([1.0; 3], [-2.0, 0.0, 0.0])),
                }),
                8 => Some(HsdEnvelopeJointMatrices {
                    current_world: matrix([1.0, 3.0, 1.0], [0.0, 4.0, 0.0]),
                    inverse_bind: Some(matrix([1.0; 3], [0.0, -1.0, 0.0])),
                }),
                _ => None,
            },
        )
        .expect("weighted palette");

        let output = palette
            .deform(0, [2.0, 2.0, 1.0], [1.0, 1.0, 0.0])
            .expect("deformed vertex");
        assert_vec3_near(output.position, [3.0, 4.0, 0.75]);
        assert_vec3_near(output.normal, [0.868_243_16, 0.496_138_93, 0.0]);
    }

    #[test]
    fn rotated_nonuniform_matrix_preserves_inverse_transpose_orientation() {
        let matrices = HsdEnvelopeMatrix::from_rigid(Mat4::from_srt(
            [2.0, 1.0, 1.0],
            [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            [0.0; 3],
        ))
        .expect("rigid matrices");

        let output = matrices
            .deform([0.0; 3], [1.0, 1.0, 0.0])
            .expect("deformed vertex");
        assert_vec3_near(output.normal, [-0.894_427_2, 0.447_213_6, 0.0]);
    }

    #[test]
    fn full_weight_fast_path_only_uses_inverse_bind_with_model_node_correction() {
        let entries = [envelope(&[(4, 1.0), (8, 0.5)])];
        let resolve = |joint: JObjId| {
            (joint.0 == 4).then_some(HsdEnvelopeJointMatrices {
                current_world: matrix([1.0; 3], [5.0, 0.0, 0.0]),
                inverse_bind: Some(matrix([1.0; 3], [-2.0, 0.0, 0.0])),
            })
        };
        let root_palette = HsdEnvelopePalette::build(
            &entries,
            HsdEnvelopeSingleWeightPolicy::MeleeFighter,
            None,
            resolve,
        )
        .expect("skeleton-root palette");
        let corrected_palette = HsdEnvelopePalette::build(
            &entries,
            HsdEnvelopeSingleWeightPolicy::MeleeFighter,
            Some(matrix([1.0; 3], [3.0, 0.0, 0.0])),
            resolve,
        )
        .expect("corrected palette");

        assert_vec3_near(
            root_palette
                .deform(0, [0.0; 3], [1.0, 0.0, 0.0])
                .unwrap()
                .position,
            [5.0, 0.0, 0.0],
        );
        assert_vec3_near(
            corrected_palette
                .deform(0, [0.0; 3], [1.0, 0.0, 0.0])
                .unwrap()
                .position,
            [6.0, 0.0, 0.0],
        );
    }

    #[test]
    fn skeleton_root_fast_path_ignores_an_unused_inverse_bind() {
        let entries = [envelope(&[(4, 1.0)])];
        let mut unused_inverse_bind = Mat4::identity();
        unused_inverse_bind.0[0][0] = f32::NAN;
        let resolve = |_| {
            Some(HsdEnvelopeJointMatrices {
                current_world: Mat4::identity(),
                inverse_bind: Some(unused_inverse_bind),
            })
        };

        assert!(
            HsdEnvelopePalette::build(
                &entries,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                resolve,
            )
            .is_ok()
        );
        assert_eq!(
            HsdEnvelopePalette::build(
                &entries,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                Some(Mat4::identity()),
                resolve,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::NonFiniteJointMatrix {
                palette_index: 0,
                influence_index: 0,
            }
        );
    }

    #[test]
    fn single_weight_threshold_matches_the_selected_source_path() {
        let entries = [envelope(&[(4, 1.0 - f32::EPSILON)])];
        let resolve = |_| {
            Some(HsdEnvelopeJointMatrices {
                current_world: Mat4::identity(),
                inverse_bind: None,
            })
        };

        assert!(
            HsdEnvelopePalette::build(
                &entries,
                HsdEnvelopeSingleWeightPolicy::GenericHsd,
                None,
                resolve,
            )
            .is_ok()
        );
        assert_eq!(
            HsdEnvelopePalette::build(
                &entries,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                resolve,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::MissingInverseBind {
                palette_index: 0,
                influence_index: 0,
                source_id: 4,
            }
        );
    }

    #[test]
    fn selector_is_a_multiple_of_three_and_cannot_address_unused_slots() {
        let entries = [envelope(&[(1, 1.0)]), envelope(&[(2, 1.0)])];
        let palette = HsdEnvelopePalette::build(
            &entries,
            HsdEnvelopeSingleWeightPolicy::MeleeFighter,
            None,
            |joint| {
                Some(HsdEnvelopeJointMatrices {
                    current_world: matrix([1.0; 3], [joint.0 as f32, 0.0, 0.0]),
                    inverse_bind: None,
                })
            },
        )
        .unwrap();

        assert!(palette.matrix_for_selector(0).is_ok());
        assert!(palette.matrix_for_selector(3).is_ok());
        assert_eq!(
            palette.matrix_for_selector(1).unwrap_err(),
            HsdEnvelopeEvaluationError::InvalidPaletteSelector { pn_mtx_idx: 1 }
        );
        assert_eq!(
            palette.matrix_for_selector(6).unwrap_err(),
            HsdEnvelopeEvaluationError::PaletteSelectorOutOfRange {
                pn_mtx_idx: 6,
                palette_index: 2,
                palette_len: 2,
            }
        );
    }

    #[test]
    fn malformed_palette_and_vertex_inputs_fail_explicitly() {
        assert_eq!(
            HsdEnvelopePalette::build(
                &[],
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                |_| None,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::EmptyPalette
        );
        let oversized = (0..=HSD_ENVELOPE_PALETTE_LEN)
            .map(|joint| envelope(&[(joint as u32, 1.0)]))
            .collect::<Vec<_>>();
        assert_eq!(
            HsdEnvelopePalette::build(
                &oversized,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                |_| None,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::PaletteTooLarge {
                count: HSD_ENVELOPE_PALETTE_LEN + 1,
                limit: HSD_ENVELOPE_PALETTE_LEN,
            }
        );
        let empty = [envelope(&[])];
        assert_eq!(
            HsdEnvelopePalette::build(
                &empty,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                |_| None,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::EmptyEnvelope { palette_index: 0 }
        );
        let missing = [envelope(&[(7, 0.5)])];
        assert_eq!(
            HsdEnvelopePalette::build(
                &missing,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                |_| None,
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::MissingJoint {
                palette_index: 0,
                influence_index: 0,
                source_id: 7,
            }
        );
        let no_bind = [envelope(&[(7, 0.5)])];
        assert_eq!(
            HsdEnvelopePalette::build(
                &no_bind,
                HsdEnvelopeSingleWeightPolicy::MeleeFighter,
                None,
                |_| {
                    Some(HsdEnvelopeJointMatrices {
                        current_world: Mat4::identity(),
                        inverse_bind: None,
                    })
                }
            )
            .unwrap_err(),
            HsdEnvelopeEvaluationError::MissingInverseBind {
                palette_index: 0,
                influence_index: 0,
                source_id: 7,
            }
        );

        let rigid = HsdEnvelopeMatrix::from_rigid(Mat4::identity()).unwrap();
        assert_eq!(
            rigid.deform([f32::NAN, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Err(HsdEnvelopeEvaluationError::NonFiniteVertexInput)
        );
        assert_eq!(rigid.deform([0.0; 3], [0.0; 3]).unwrap().normal, [0.0; 3]);
        let singular = HsdEnvelopeMatrix::from_rigid(matrix([0.0, 1.0, 1.0], [0.0; 3])).unwrap();
        assert_eq!(
            singular.deform([0.0; 3], [1.0, 0.0, 0.0]),
            Err(HsdEnvelopeEvaluationError::DegenerateNormal)
        );
        assert_eq!(
            singular.deform([0.0; 3], [0.0, 1.0, 0.0]).unwrap().normal,
            [0.0, 1.0, 0.0]
        );
    }
}
