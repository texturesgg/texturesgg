//! Renderer-neutral evaluated HSD draw work.
//!
//! This module consumes a settled [`HsdScene`], an
//! optional root-local pose, and the source-backed envelope evaluator. It emits
//! evaluator-owned world matrices and position/normal streams in source occurrence order;
//! it does not select cameras, materials, render passes, or backend resources.

use super::{
    envelope::{
        HSD_MATRIX_INVERSE_EPSILON, HsdEnvelopeEvaluationError, HsdEnvelopeJointMatrices,
        HsdEnvelopeMatrix, HsdEnvelopePalette, HsdEnvelopeSingleWeightPolicy,
    },
    scene::{
        DObjId, HsdJointIndex, HsdPolygonBinding, HsdScene, HsdSceneError, HsdSceneLimits,
        HsdSceneRoot, HsdTransform, JObjId, PObjId,
    },
};
use crate::{descriptor::jobj, math::Mat4};
use std::collections::HashMap;
use thiserror::Error;

/// Explicit semantic choices applied while evaluating draw work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsdDrawEvaluationPolicy {
    pub envelope_single_weight: HsdEnvelopeSingleWeightPolicy,
}

impl HsdDrawEvaluationPolicy {
    /// Matching policy for Melee fighter model rendering.
    pub const MELEE_FIGHTER: Self = Self {
        envelope_single_weight: HsdEnvelopeSingleWeightPolicy::MeleeFighter,
    };

    /// Matching policy for generic HSD model rendering.
    pub const GENERIC_HSD: Self = Self {
        envelope_single_weight: HsdEnvelopeSingleWeightPolicy::GenericHsd,
    };
}

/// Aggregate draw expansion budgets across every model root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsdDrawWorkLimits {
    pub max_joint_occurrences: usize,
    pub max_packets: usize,
    pub max_vertices: usize,
}

impl Default for HsdDrawWorkLimits {
    fn default() -> Self {
        let limits = HsdSceneLimits::default();
        Self {
            max_joint_occurrences: limits.max_joints,
            max_packets: limits.max_polygons,
            max_vertices: limits.max_vertices,
        }
    }
}

/// One optional root-local pose supplied to an evaluation.
#[derive(Clone, Copy, Debug)]
pub struct HsdRootPose<'a> {
    pub root_index: usize,
    /// Exact depth-first joint order from the matching [`super::scene::HsdSceneRoot`].
    pub transforms: &'a [HsdTransform],
    /// Runtime JOBJ_HIDDEN per joint in the same order; `None` uses serialized flags.
    /// An INSTANCE JObj's state must match preparation, which fixes its expansion.
    pub hidden_joints: Option<&'a [bool]>,
}

/// Stable identity and vertex range for one evaluated source PObj.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsdEvaluatedDrawPacket {
    pub joint_index: HsdJointIndex,
    pub display_object_index: usize,
    pub polygon_index: usize,
    pub polygon_source_id: PObjId,
    pub first_vertex: usize,
    pub vertex_count: usize,
    /// False when the owning JObj is JOBJ_HIDDEN this frame. HSD_JObjDispDObj
    /// skips its DObjs, but the packet keeps its range so topology stays fixed.
    pub visible: bool,
}

/// Evaluated world-space work for one model root.
#[derive(Debug)]
pub struct HsdEvaluatedDrawRoot {
    pub source_id: JObjId,
    pub joint_world_matrices: Vec<Mat4>,
    /// Parallel to [`Self::normals`], in evaluated packet/vertex order.
    pub positions: Vec<[f32; 3]>,
    /// Parallel to [`Self::positions`], normalized after inverse-transpose.
    pub normals: Vec<[f32; 3]>,
    /// Parallel to [`Self::positions`]; inverse-transpose transformed but not normalized.
    pub binormals: Vec<[f32; 3]>,
    /// Parallel to [`Self::positions`]; inverse-transpose transformed but not normalized.
    pub tangents: Vec<[f32; 3]>,
    pub packets: Vec<HsdEvaluatedDrawPacket>,
}

/// One complete frame of renderer-neutral evaluated draw work.
#[derive(Debug)]
pub struct HsdEvaluatedDrawWork {
    pub roots: Vec<HsdEvaluatedDrawRoot>,
}

const INSTANCE: u32 = 0x1000;
const HIDDEN: u32 = 0x10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparedBinding {
    Rigid(Option<JObjId>),
    Envelope,
}

impl PreparedBinding {
    fn new(binding: &HsdPolygonBinding) -> Self {
        match binding {
            HsdPolygonBinding::Rigid { joint } => Self::Rigid(*joint),
            HsdPolygonBinding::Envelope { .. } => Self::Envelope,
        }
    }
}

#[derive(Debug)]
struct PreparedPacket {
    joint_index: HsdJointIndex,
    display_object_index: usize,
    polygon_index: usize,
    polygon_source_id: PObjId,
    vertex_count: usize,
    binding: PreparedBinding,
    rigid_joint_index: Option<HsdJointIndex>,
}

#[derive(Debug)]
struct PreparedJoint {
    source_id: JObjId,
    parent: Option<HsdJointIndex>,
    children: Vec<HsdJointIndex>,
    /// Only INSTANCE | HIDDEN; other flags are evaluated per frame.
    flags: u32,
    display_objects: Vec<(DObjId, usize)>,
    packets: std::ops::Range<usize>,
}

#[derive(Debug)]
struct PreparedOccurrence {
    packet_index: usize,
    /// The most recently entered instance, not an accumulated instance chain.
    instance: Option<HsdJointIndex>,
}

#[derive(Debug)]
struct PreparedRoot {
    source_id: JObjId,
    joints: Vec<PreparedJoint>,
    source_to_joint: HashMap<JObjId, HsdJointIndex>,
    /// Every source packet, including packets suppressed by INSTANCE.
    packets: Vec<PreparedPacket>,
    occurrences: Vec<PreparedOccurrence>,
    hidden_display_objects: std::collections::HashSet<DObjId>,
    instances: Vec<HsdJointIndex>,
    vertex_count: usize,
}

impl PreparedRoot {
    fn occurrence_visible(&self, occurrence: &PreparedOccurrence) -> bool {
        let packet = &self.packets[occurrence.packet_index];
        let source_id =
            self.joints[packet.joint_index.0].display_objects[packet.display_object_index].0;
        !self.hidden_display_objects.contains(&source_id)
    }
}

/// Frame-local caches; clearing values never releases their prepared capacity.
#[derive(Debug)]
struct DrawRootScratch {
    target_inverses: Vec<Option<Mat4>>,
    instance_corrections: Vec<Option<Mat4>>,
    model_node_corrections: Vec<Option<Option<Mat4>>>,
    rigid_matrices: HashMap<(Option<HsdJointIndex>, HsdJointIndex), HsdEnvelopeMatrix>,
    palettes: HashMap<usize, HsdEnvelopePalette>,
    corrected_palettes: HashMap<
        (HsdJointIndex, usize),
        [Option<HsdEnvelopeMatrix>; super::envelope::HSD_ENVELOPE_PALETTE_LEN],
    >,
}

/// Reusable topology preparation for deterministic bind-pose or animated draws.
#[derive(Debug)]
pub struct HsdDrawWorkEvaluator {
    policy: HsdDrawEvaluationPolicy,
    roots: Vec<PreparedRoot>,
    output: HsdEvaluatedDrawWork,
    scratch: Vec<DrawRootScratch>,
    /// Indices into this call's pose slice, never borrowed transforms from an old frame.
    pose_by_root: Vec<Option<usize>>,
}

impl HsdDrawWorkEvaluator {
    /// [`Self::prepare_with_limits`] under the default limits.
    #[cfg(test)]
    pub(crate) fn prepare(
        scene: &HsdScene,
        policy: HsdDrawEvaluationPolicy,
    ) -> Result<Self, HsdDrawWorkError> {
        Self::prepare_with_limits(scene, policy, HsdDrawWorkLimits::default())
    }

    /// Validate immutable scene topology, resolve source joint identities once,
    /// and bound expansion before allocating draw occurrences.
    pub fn prepare_with_limits(
        scene: &HsdScene,
        policy: HsdDrawEvaluationPolicy,
        limits: HsdDrawWorkLimits,
    ) -> Result<Self, HsdDrawWorkError> {
        scene.validate()?;

        // Count the complete expansion first. In particular, empty instance DAGs
        // still consume joint occurrences, and roots share the same budgets.
        let mut counts = DrawCounts::default();
        let mut root_counts = Vec::with_capacity(scene.roots.len());
        for (root_index, root) in scene.roots.iter().enumerate() {
            validate_instance_contract(root, root_index)?;
            let before = counts;
            walk_occurrences(root, root_index, &mut counts, limits, |_, _| {})?;
            root_counts.push((
                counts.packets - before.packets,
                counts.vertices - before.vertices,
            ));
        }

        let mut roots = Vec::with_capacity(scene.roots.len());
        for (root_index, (root, (packet_count, vertex_count))) in
            scene.roots.iter().zip(root_counts).enumerate()
        {
            let source_to_joint = root
                .joints
                .iter()
                .enumerate()
                .map(|(joint_index, joint)| (joint.source_id, HsdJointIndex(joint_index)))
                .collect::<HashMap<_, _>>();
            if source_to_joint.len() != root.joints.len() {
                return Err(HsdDrawWorkError::DuplicateJointSourceId { root_index });
            }

            let mut packets = Vec::new();
            let mut joints = Vec::with_capacity(root.joints.len());
            for (joint_index, joint) in root.joints.iter().enumerate() {
                let first_packet = packets.len();
                for (display_object_index, display_object) in
                    joint.display_objects.iter().enumerate()
                {
                    for (polygon_index, polygon) in display_object.polygons.iter().enumerate() {
                        let rigid_joint_index = match polygon.binding {
                            // DispAll never dispatches an INSTANCE's own DObjs.
                            // Retain their source metadata without resolving an
                            // unused geometry binding.
                            _ if joint.flags & INSTANCE != 0 => None,
                            HsdPolygonBinding::Rigid {
                                joint: Some(source_id),
                            } => Some(source_to_joint.get(&source_id).copied().ok_or(
                                HsdDrawWorkError::MissingRigidJoint {
                                    root_index,
                                    polygon_source_id: polygon.source_id,
                                    source_id,
                                },
                            )?),
                            HsdPolygonBinding::Rigid { joint: None } => {
                                Some(HsdJointIndex(joint_index))
                            }
                            HsdPolygonBinding::Envelope { .. } => None,
                        };
                        packets.push(PreparedPacket {
                            joint_index: HsdJointIndex(joint_index),
                            display_object_index,
                            polygon_index,
                            polygon_source_id: polygon.source_id,
                            vertex_count: polygon.decoded.vertices.len(),
                            binding: PreparedBinding::new(&polygon.binding),
                            rigid_joint_index,
                        });
                    }
                }
                joints.push(PreparedJoint {
                    source_id: joint.source_id,
                    parent: joint.parent,
                    children: joint.children.clone(),
                    flags: joint.flags & (INSTANCE | HIDDEN),
                    display_objects: joint
                        .display_objects
                        .iter()
                        .map(|object| (object.source_id, object.polygons.len()))
                        .collect(),
                    packets: first_packet..packets.len(),
                });
            }

            let mut occurrences = Vec::with_capacity(packet_count);
            let mut instances = Vec::new();
            let mut seen_instance = vec![false; root.joints.len()];
            walk_occurrences(
                root,
                root_index,
                &mut DrawCounts::default(),
                limits,
                |index, instance| {
                    if root.joints[index.0].flags & INSTANCE != 0 {
                        if root.joints[index.0].flags & HIDDEN == 0 && !seen_instance[index.0] {
                            seen_instance[index.0] = true;
                            instances.push(index);
                        }
                    } else {
                        for packet_index in joints[index.0].packets.clone() {
                            occurrences.push(PreparedOccurrence {
                                packet_index,
                                instance,
                            });
                        }
                    }
                },
            )?;
            roots.push(PreparedRoot {
                source_id: root.source_id,
                joints,
                source_to_joint,
                packets,
                occurrences,
                hidden_display_objects: std::collections::HashSet::new(),
                instances,
                vertex_count,
            });
        }
        // All expanded output and frame-cache capacities are bounded above before
        // allocation. Envelope contents can change each frame; only their ten-slot
        // palette bound, not their matrices or influences, is retained.
        let mut output = HsdEvaluatedDrawWork {
            roots: Vec::with_capacity(roots.len()),
        };
        let mut scratch = Vec::with_capacity(roots.len());
        for root in &roots {
            let mut first_vertex = 0;
            let packets = root
                .occurrences
                .iter()
                .map(|occurrence| {
                    let packet = &root.packets[occurrence.packet_index];
                    let metadata = HsdEvaluatedDrawPacket {
                        joint_index: packet.joint_index,
                        display_object_index: packet.display_object_index,
                        polygon_index: packet.polygon_index,
                        polygon_source_id: packet.polygon_source_id,
                        first_vertex,
                        vertex_count: packet.vertex_count,
                        visible: root.joints[packet.joint_index.0].flags & HIDDEN == 0,
                    };
                    first_vertex += packet.vertex_count;
                    metadata
                })
                .collect();
            output.roots.push(HsdEvaluatedDrawRoot {
                source_id: root.source_id,
                joint_world_matrices: Vec::with_capacity(root.joints.len()),
                positions: Vec::with_capacity(root.vertex_count),
                normals: Vec::with_capacity(root.vertex_count),
                binormals: Vec::with_capacity(root.vertex_count),
                tangents: Vec::with_capacity(root.vertex_count),
                packets,
            });
            let rigid_count = root
                .occurrences
                .iter()
                .filter(|occurrence| {
                    matches!(
                        root.packets[occurrence.packet_index].binding,
                        PreparedBinding::Rigid(_)
                    )
                })
                .count();
            let envelope_count = root.occurrences.len() - rigid_count;
            let corrected_count = root
                .occurrences
                .iter()
                .filter(|occurrence| {
                    occurrence.instance.is_some()
                        && root.packets[occurrence.packet_index].binding
                            == PreparedBinding::Envelope
                })
                .count();
            let instance_joint_count = if root.instances.is_empty() {
                0
            } else {
                root.joints.len()
            };
            scratch.push(DrawRootScratch {
                target_inverses: vec![None; instance_joint_count],
                instance_corrections: vec![None; instance_joint_count],
                model_node_corrections: vec![
                    None;
                    if envelope_count == 0 {
                        0
                    } else {
                        root.joints.len()
                    }
                ],
                rigid_matrices: HashMap::with_capacity(rigid_count),
                palettes: HashMap::with_capacity(envelope_count),
                corrected_palettes: HashMap::with_capacity(corrected_count),
            });
        }
        let pose_by_root = vec![None; roots.len()];
        Ok(Self {
            policy,
            roots,
            output,
            scratch,
            pose_by_root,
        })
    }

    /// Replace runtime DOBJ_HIDDEN admission for one root.
    ///
    /// Melee's ftParts visibility tables set this state independently of JObj
    /// transforms. IDs must belong to the selected root; validation is atomic.
    /// Visible packet ranges and vertex output remain contiguous. Joint matrices
    /// and scene topology are still fully validated, including hidden objects.
    pub fn set_hidden_display_objects(
        &mut self,
        root_index: usize,
        source_ids: &[DObjId],
    ) -> Result<(), HsdDrawWorkError> {
        let root_count = self.roots.len();
        let root =
            self.roots
                .get_mut(root_index)
                .ok_or(HsdDrawWorkError::VisibilityRootOutOfRange {
                    root_index,
                    root_count,
                })?;
        for &source_id in source_ids {
            if !root
                .joints
                .iter()
                .any(|joint| joint.display_objects.iter().any(|&(id, _)| id == source_id))
            {
                return Err(HsdDrawWorkError::MissingDisplayObject {
                    root_index,
                    source_id,
                });
            }
        }
        root.hidden_display_objects.clear();
        root.hidden_display_objects
            .extend(source_ids.iter().copied());
        root.vertex_count = root
            .occurrences
            .iter()
            .filter(|occurrence| root.occurrence_visible(occurrence))
            .map(|occurrence| root.packets[occurrence.packet_index].vertex_count)
            .sum();
        let packets = self
            .prepared_packets()
            .filter_map(|(index, packet)| (index == root_index).then_some(packet))
            .collect();
        self.output.roots[root_index].packets = packets;
        Ok(())
    }

    /// Iterate candidate packet metadata from the prepared scene snapshot.
    ///
    /// Each item contains the root index and the same source path/order/range
    /// metadata produced by a successful evaluation of matching topology.
    /// This borrows prepared occurrences without allocating or deforming geometry;
    /// it does not validate a current scene, prove frame evaluation will succeed,
    /// or infer game-pass admission. INSTANCE expansion reflects the snapshot
    /// used for preparation; explicit runtime DObj visibility is respected.
    pub(crate) fn prepared_packets(
        &self,
    ) -> impl Iterator<Item = (usize, HsdEvaluatedDrawPacket)> + '_ {
        self.roots
            .iter()
            .enumerate()
            .flat_map(|(root_index, root)| {
                root.occurrences
                    .iter()
                    .filter(|occurrence| root.occurrence_visible(occurrence))
                    .scan(0usize, move |first_vertex, occurrence| {
                        let packet = &root.packets[occurrence.packet_index];
                        let metadata = HsdEvaluatedDrawPacket {
                            joint_index: packet.joint_index,
                            display_object_index: packet.display_object_index,
                            polygon_index: packet.polygon_index,
                            polygon_source_id: packet.polygon_source_id,
                            first_vertex: *first_vertex,
                            vertex_count: packet.vertex_count,
                            visible: root.joints[packet.joint_index.0].flags & HIDDEN == 0,
                        };
                        // Preparation already checked this complete per-root sum.
                        *first_vertex += packet.vertex_count;
                        Some((root_index, metadata))
                    })
            })
    }

    /// Evaluate bind pose for every root into retained frame storage.
    pub fn evaluate_bind_pose(
        &mut self,
        scene: &HsdScene,
    ) -> Result<&HsdEvaluatedDrawWork, HsdDrawWorkError> {
        self.evaluate(scene, &[])
    }

    /// Evaluate one frame. Roots absent from `poses` use their serialized bind pose.
    ///
    /// The complete successful frame borrows this evaluator until the next tick.
    /// Errors expose no partial output; the next call recomputes all frame values.
    pub fn evaluate(
        &mut self,
        scene: &HsdScene,
        poses: &[HsdRootPose<'_>],
    ) -> Result<&HsdEvaluatedDrawWork, HsdDrawWorkError> {
        if let Err(error) = self.evaluate_frame(scene, poses) {
            for root in &mut self.output.roots {
                root.joint_world_matrices.clear();
                root.positions.clear();
                root.normals.clear();
                root.binormals.clear();
                root.tangents.clear();
            }
            return Err(error);
        }
        Ok(&self.output)
    }

    fn evaluate_frame(
        &mut self,
        scene: &HsdScene,
        poses: &[HsdRootPose<'_>],
    ) -> Result<(), HsdDrawWorkError> {
        if scene.roots.len() != self.roots.len() {
            return Err(HsdDrawWorkError::SceneTopologyMismatch);
        }

        self.pose_by_root.fill(None);
        for (pose_index, pose) in poses.iter().enumerate() {
            let root_count = scene.roots.len();
            let slot = self.pose_by_root.get_mut(pose.root_index).ok_or(
                HsdDrawWorkError::PoseRootOutOfRange {
                    root_index: pose.root_index,
                    root_count,
                },
            )?;
            if slot.replace(pose_index).is_some() {
                return Err(HsdDrawWorkError::DuplicateRootPose {
                    root_index: pose.root_index,
                });
            }
            let expected = scene.roots[pose.root_index].joints.len();
            if let Some(hidden) = pose.hidden_joints {
                if hidden.len() != expected {
                    return Err(HsdDrawWorkError::PoseVisibilityCountMismatch {
                        root_index: pose.root_index,
                        expected,
                        actual: hidden.len(),
                    });
                }
                let prepared = &self.roots[pose.root_index];
                if let Some(joint_index) =
                    prepared
                        .joints
                        .iter()
                        .zip(hidden)
                        .position(|(joint, &hidden)| {
                            joint.flags & INSTANCE != 0 && (joint.flags & HIDDEN != 0) != hidden
                        })
                {
                    return Err(HsdDrawWorkError::RuntimeInstanceVisibility {
                        root_index: pose.root_index,
                        joint_index: HsdJointIndex(joint_index),
                    });
                }
            }
        }

        for (root_index, ((root, prepared), pose_index)) in scene
            .roots
            .iter()
            .zip(&self.roots)
            .zip(&self.pose_by_root)
            .enumerate()
        {
            let pose = pose_index.map(|index| poses[index].transforms);
            let hidden_joints = pose_index.and_then(|index| poses[index].hidden_joints);
            verify_root_topology(root, prepared)?;
            if let Some(transforms) = pose
                && transforms.len() != root.joints.len()
            {
                return Err(HsdDrawWorkError::PoseJointCountMismatch {
                    root_index,
                    expected: root.joints.len(),
                    actual: transforms.len(),
                });
            }

            let HsdEvaluatedDrawRoot {
                joint_world_matrices,
                positions,
                normals,
                binormals,
                tangents,
                packets,
                ..
            } = &mut self.output.roots[root_index];
            joint_world_matrices.clear();
            positions.clear();
            normals.clear();
            binormals.clear();
            tangents.clear();
            let DrawRootScratch {
                target_inverses,
                instance_corrections,
                model_node_corrections,
                rigid_matrices,
                palettes,
                corrected_palettes,
            } = &mut self.scratch[root_index];
            target_inverses.fill(None);
            instance_corrections.fill(None);
            model_node_corrections.fill(None);
            rigid_matrices.clear();
            palettes.clear();
            corrected_palettes.clear();
            for (joint_index, joint) in root.joints.iter().enumerate() {
                let transform = pose.map_or(joint.local, |transforms| transforms[joint_index]);
                if !transform_is_finite(transform) {
                    return Err(HsdDrawWorkError::NonFinitePoseTransform {
                        root_index,
                        joint_index: HsdJointIndex(joint_index),
                    });
                }
                let local =
                    Mat4::from_srt(transform.scale, transform.rotation, transform.translation);
                let world = match joint.parent {
                    Some(parent) => joint_world_matrices[parent.0].mul(&local),
                    None => local,
                };
                if !matrix_is_finite(world) {
                    return Err(HsdDrawWorkError::NonFiniteWorldMatrix {
                        root_index,
                        joint_index: HsdJointIndex(joint_index),
                    });
                }
                joint_world_matrices.push(world);
            }

            // A repeated occurrence shares the source instance's correction.
            // Nested instances start again from camera/world space (DispAll),
            // rather than concatenating the correction inherited from a caller.
            for &instance in &prepared.instances {
                let target = root.joints[instance.0].children[0];
                let inverse = match target_inverses[target.0] {
                    Some(inverse) => inverse,
                    None => {
                        let inverse = psmtx_inverse_affine(joint_world_matrices[target.0]).ok_or(
                            HsdDrawWorkError::SingularInstanceTarget {
                                root_index,
                                joint_index: instance,
                                target,
                            },
                        )?;
                        target_inverses[target.0] = Some(inverse);
                        inverse
                    }
                };
                let correction = joint_world_matrices[instance.0].mul(&inverse);
                if !matrix_is_finite(inverse) || !matrix_is_finite(correction) {
                    return Err(HsdDrawWorkError::NonFiniteInstanceCorrection {
                        root_index,
                        joint_index: instance,
                        target,
                    });
                }
                instance_corrections[instance.0] = Some(correction);
            }
            let mut output_packets = packets.iter_mut();
            for occurrence in &prepared.occurrences {
                if !prepared.occurrence_visible(occurrence) {
                    continue;
                }
                let packet = &prepared.packets[occurrence.packet_index];
                output_packets
                    .next()
                    .expect("one output packet per visible occurrence")
                    .visible = !hidden_joints.map_or(
                    prepared.joints[packet.joint_index.0].flags & HIDDEN != 0,
                    |hidden| hidden[packet.joint_index.0],
                );
                let correction = occurrence.instance.map(|instance| {
                    instance_corrections[instance.0].expect("prepared visible instance")
                });
                let joint = &root.joints[packet.joint_index.0];
                let polygon = &joint.display_objects[packet.display_object_index].polygons
                    [packet.polygon_index];

                match &polygon.binding {
                    HsdPolygonBinding::Rigid { .. } => {
                        let rigid_joint = packet
                            .rigid_joint_index
                            .expect("prepared rigid packet has a joint");
                        let matrix = match rigid_matrices.entry((occurrence.instance, rigid_joint))
                        {
                            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                            std::collections::hash_map::Entry::Vacant(entry) => {
                                let world = joint_world_matrices[rigid_joint.0];
                                let world =
                                    correction.map_or(world, |correction| correction.mul(&world));
                                entry.insert(HsdEnvelopeMatrix::from_rigid(world).map_err(
                                    |source| HsdDrawWorkError::Evaluation {
                                        root_index,
                                        polygon_source_id: polygon.source_id,
                                        source,
                                    },
                                )?)
                            }
                        };
                        deform_vertices(
                            &polygon.decoded.vertices,
                            |_, position, normal, binormal, tangent| {
                                Ok((
                                    matrix.deform(position, normal)?,
                                    matrix.transform_normal_basis(binormal)?,
                                    matrix.transform_normal_basis(tangent)?,
                                ))
                            },
                            positions,
                            normals,
                            binormals,
                            tangents,
                            root_index,
                            polygon.source_id,
                        )?;
                    }
                    HsdPolygonBinding::Envelope { entries, .. } => {
                        let palette = match palettes.entry(occurrence.packet_index) {
                            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                            std::collections::hash_map::Entry::Vacant(entry) => {
                                let model_node_correction =
                                    match model_node_corrections[packet.joint_index.0] {
                                        Some(correction) => correction,
                                        None => {
                                            // Skeleton discovery and its relative matrix always use
                                            // the original owned hierarchy, never the instance edge.
                                            let correction = envelope_model_node_correction(
                                                root,
                                                packet.joint_index,
                                                joint_world_matrices,
                                                root_index,
                                                polygon.source_id,
                                            )?;
                                            model_node_corrections[packet.joint_index.0] =
                                                Some(correction);
                                            correction
                                        }
                                    };
                                entry.insert(
                                    HsdEnvelopePalette::build(
                                        entries,
                                        self.policy.envelope_single_weight,
                                        model_node_correction,
                                        |source_id| {
                                            prepared.source_to_joint.get(&source_id).map(
                                                |joint_index| HsdEnvelopeJointMatrices {
                                                    current_world: joint_world_matrices
                                                        [joint_index.0],
                                                    inverse_bind: root.joints[joint_index.0]
                                                        .inverse_bind_transform,
                                                },
                                            )
                                        },
                                    )
                                    .map_err(|source| {
                                        HsdDrawWorkError::Evaluation {
                                            root_index,
                                            polygon_source_id: polygon.source_id,
                                            source,
                                        }
                                    })?,
                                )
                            }
                        };
                        // Compose after the source-backed weighted sum and
                        // model-node correction. Moving this multiplication
                        // inside the sum can overflow influences whose weighted
                        // result is finite. HSD accumulates a 3x4 Mtx, so its
                        // implicit homogeneous row is affine, not the weight sum.
                        let corrected_palette = if let (Some(instance), Some(correction)) =
                            (occurrence.instance, correction)
                        {
                            Some(
                                match corrected_palettes.entry((instance, occurrence.packet_index))
                                {
                                    std::collections::hash_map::Entry::Occupied(entry) => {
                                        entry.into_mut()
                                    }
                                    std::collections::hash_map::Entry::Vacant(entry) => {
                                        let mut matrices =
                                            [None; super::envelope::HSD_ENVELOPE_PALETTE_LEN];
                                        for (palette_index, selector) in
                                            (0u16..).step_by(3).take(palette.len()).enumerate()
                                        {
                                            let source = palette
                                                .matrix_for_selector(selector)
                                                .expect("bounded source palette");
                                            let mut source_position = source.position_matrix();
                                            source_position.0[0][3] = 0.0;
                                            source_position.0[1][3] = 0.0;
                                            source_position.0[2][3] = 0.0;
                                            source_position.0[3][3] = 1.0;
                                            let position = correction.mul(&source_position);
                                            let matrix = HsdEnvelopeMatrix::from_rigid(position).map_err(|source| {
                                            let source = match source {
                                                HsdEnvelopeEvaluationError::NonFiniteRigidMatrix =>
                                                    HsdEnvelopeEvaluationError::NonFiniteComposedMatrix { palette_index },
                                                HsdEnvelopeEvaluationError::NonFiniteRigidNormalMatrix =>
                                                    HsdEnvelopeEvaluationError::NonFiniteComposedNormalMatrix { palette_index },
                                                source => source,
                                            };
                                            HsdDrawWorkError::Evaluation {
                                                root_index, polygon_source_id: polygon.source_id, source,
                                            }
                                        })?;
                                            matrices[palette_index] = Some(matrix);
                                        }
                                        entry.insert(matrices)
                                    }
                                },
                            )
                        } else {
                            None
                        };
                        deform_vertices(
                            &polygon.decoded.vertices,
                            |pn_mtx_idx, position, normal, binormal, tangent| {
                                let matrix = palette.matrix_for_selector(pn_mtx_idx)?;
                                let matrix =
                                    corrected_palette.as_ref().map_or(matrix, |matrices| {
                                        matrices[usize::from(pn_mtx_idx / 3)]
                                            .as_ref()
                                            .expect("bounded corrected palette")
                                    });
                                Ok((
                                    matrix.deform(position, normal)?,
                                    matrix.transform_normal_basis(binormal)?,
                                    matrix.transform_normal_basis(tangent)?,
                                ))
                            },
                            positions,
                            normals,
                            binormals,
                            tangents,
                            root_index,
                            polygon.source_id,
                        )?;
                    }
                }
            }

            debug_assert_eq!(positions.len(), prepared.vertex_count);
            debug_assert_eq!(normals.len(), prepared.vertex_count);
            debug_assert_eq!(binormals.len(), prepared.vertex_count);
            debug_assert_eq!(tangents.len(), prepared.vertex_count);
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct DrawCounts {
    joints: usize,
    packets: usize,
    vertices: usize,
}

fn draw_budget(
    current: usize,
    additional: usize,
    limit: usize,
    resource: &'static str,
) -> Result<usize, HsdDrawWorkError> {
    current
        .checked_add(additional)
        .filter(|total| *total <= limit)
        .ok_or(HsdDrawWorkError::LimitExceeded { resource, limit })
}

fn validate_instance_contract(
    root: &HsdSceneRoot,
    root_index: usize,
) -> Result<(), HsdDrawWorkError> {
    let mut owned_edges = vec![0usize; root.joints.len()];
    for (index, joint) in root.joints.iter().enumerate() {
        let invalid = || HsdDrawWorkError::InvalidJointContract {
            root_index,
            joint_index: HsdJointIndex(index),
        };
        if joint.flags & INSTANCE != 0 {
            if joint.children.len() != 1 || joint.children[0].0 >= root.joints.len() {
                return Err(invalid());
            }
        } else {
            for child in &joint.children {
                if root
                    .joints
                    .get(child.0)
                    .is_none_or(|child| child.parent != Some(HsdJointIndex(index)))
                {
                    return Err(invalid());
                }
                owned_edges[child.0] += 1;
                if owned_edges[child.0] != 1 {
                    return Err(invalid());
                }
            }
        }
    }
    for (index, joint) in root.joints.iter().enumerate() {
        if owned_edges[index] != usize::from(joint.parent.is_some()) {
            return Err(HsdDrawWorkError::InvalidJointContract {
                root_index,
                joint_index: HsdJointIndex(index),
            });
        }
    }
    Ok(())
}

/// DFS uses one frame per active path node, not one frame per pending child.
/// Only active-path membership is a cycle: completed targets may be revisited.
fn walk_occurrences(
    root: &HsdSceneRoot,
    root_index: usize,
    counts: &mut DrawCounts,
    limits: HsdDrawWorkLimits,
    mut visit: impl FnMut(HsdJointIndex, Option<HsdJointIndex>),
) -> Result<(), HsdDrawWorkError> {
    let mut active = vec![false; root.joints.len()];
    let mut stack = Vec::new();
    for (index, joint) in root.joints.iter().enumerate() {
        if joint.parent.is_some() {
            continue;
        }
        let mut pending = Some((HsdJointIndex(index), None));
        loop {
            if let Some((index, correction)) = pending.take() {
                if active[index.0] {
                    return Err(HsdDrawWorkError::InstanceCycle {
                        root_index,
                        joint_index: index,
                    });
                }
                counts.joints = draw_budget(
                    counts.joints,
                    1,
                    limits.max_joint_occurrences,
                    "joint occurrences",
                )?;
                let joint = &root.joints[index.0];
                if joint.flags & INSTANCE == 0 {
                    for object in &joint.display_objects {
                        counts.packets = draw_budget(
                            counts.packets,
                            object.polygons.len(),
                            limits.max_packets,
                            "packets",
                        )?;
                        for polygon in &object.polygons {
                            counts.vertices = draw_budget(
                                counts.vertices,
                                polygon.decoded.vertices.len(),
                                limits.max_vertices,
                                "vertices",
                            )?;
                        }
                    }
                }
                visit(index, correction);
                active[index.0] = true;
                stack.push((index, correction, 0usize));
            }
            let Some((index, correction, next_child)) = stack.last_mut() else {
                break;
            };
            let joint = &root.joints[index.0];
            let hidden_instance = joint.flags & (INSTANCE | HIDDEN) == (INSTANCE | HIDDEN);
            if !hidden_instance && *next_child < joint.children.len() {
                let child = joint.children[*next_child];
                *next_child += 1;
                // An instance edge replaces the incoming correction. Ordinary
                // children inherit it; no target sibling is enqueued here.
                let correction = if joint.flags & INSTANCE != 0 {
                    Some(*index)
                } else {
                    *correction
                };
                pending = Some((child, correction));
            } else {
                active[index.0] = false;
                stack.pop();
            }
        }
    }
    Ok(())
}

fn verify_root_topology(
    root: &super::scene::HsdSceneRoot,
    prepared: &PreparedRoot,
) -> Result<(), HsdDrawWorkError> {
    if root.source_id != prepared.source_id
        || root.joints.len() != prepared.joints.len()
        || root
            .joints
            .iter()
            .zip(&prepared.joints)
            .any(|(joint, saved)| {
                joint.source_id != saved.source_id
                    || joint.parent != saved.parent
                    || joint.children != saved.children
                    || joint.flags & (INSTANCE | HIDDEN) != saved.flags
                    || joint.display_objects.len() != saved.display_objects.len()
                    || joint
                        .display_objects
                        .iter()
                        .zip(&saved.display_objects)
                        .any(|(object, (id, count))| {
                            object.source_id != *id || object.polygons.len() != *count
                        })
            })
    {
        return Err(HsdDrawWorkError::SceneTopologyMismatch);
    }

    let mut packet_index = 0usize;
    for (joint_index, joint) in root.joints.iter().enumerate() {
        for (display_object_index, display_object) in joint.display_objects.iter().enumerate() {
            for (polygon_index, polygon) in display_object.polygons.iter().enumerate() {
                if prepared.packets.get(packet_index).is_none_or(|packet| {
                    packet.joint_index.0 != joint_index
                        || packet.display_object_index != display_object_index
                        || packet.polygon_index != polygon_index
                        || packet.polygon_source_id != polygon.source_id
                        || packet.vertex_count != polygon.decoded.vertices.len()
                        || packet.binding != PreparedBinding::new(&polygon.binding)
                }) {
                    return Err(HsdDrawWorkError::SceneTopologyMismatch);
                }
                packet_index += 1;
            }
        }
    }
    if packet_index != prepared.packets.len() {
        return Err(HsdDrawWorkError::SceneTopologyMismatch);
    }

    Ok(())
}

// The four output streams are separate fields of the evaluated root; the
// binormal/tangent (NBT) streams stay for future emboss bump mapping.
#[allow(clippy::too_many_arguments)]
fn deform_vertices<F>(
    vertices: &[crate::gx::vertex::DecodedVertex],
    mut deform: F,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    binormals: &mut Vec<[f32; 3]>,
    tangents: &mut Vec<[f32; 3]>,
    root_index: usize,
    polygon_source_id: PObjId,
) -> Result<(), HsdDrawWorkError>
where
    F: FnMut(
        u16,
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
    ) -> Result<
        (super::envelope::HsdDeformedVertex, [f32; 3], [f32; 3]),
        HsdEnvelopeEvaluationError,
    >,
{
    for vertex in vertices {
        let (output, binormal, tangent) = deform(
            vertex.pn_mtx_idx,
            vertex.position,
            vertex.normal,
            vertex.binormal,
            vertex.tangent,
        )
        .map_err(|source| HsdDrawWorkError::Evaluation {
            root_index,
            polygon_source_id,
            source,
        })?;
        positions.push(output.position);
        normals.push(output.normal);
        binormals.push(binormal);
        tangents.push(tangent);
    }
    Ok(())
}

fn transform_is_finite(transform: HsdTransform) -> bool {
    transform
        .scale
        .into_iter()
        .chain(transform.rotation)
        .chain(transform.translation)
        .all(f32::is_finite)
}

fn matrix_is_finite(matrix: Mat4) -> bool {
    matrix.0.into_iter().flatten().all(f32::is_finite)
}

/// Match `_HSD_mkEnvelopeModelNodeMtx`: express the model node relative to its
/// nearest skeleton while preserving its two distinct inverse implementations.
fn envelope_model_node_correction(
    root: &HsdSceneRoot,
    model_node_index: HsdJointIndex,
    joint_world_matrices: &[Mat4],
    root_index: usize,
    polygon_source_id: PObjId,
) -> Result<Option<Mat4>, HsdDrawWorkError> {
    let model_node = &root.joints[model_node_index.0];
    if model_node.flags & jobj::flags::SKELETON_ROOT != 0 {
        return Ok(None);
    }

    let mut candidate = Some(model_node_index);
    let skeleton_index = loop {
        let Some(index) = candidate else {
            return Err(HsdDrawWorkError::MissingEnvelopeSkeleton {
                root_index,
                polygon_source_id,
                joint_index: model_node_index,
            });
        };
        let joint = &root.joints[index.0];
        if joint.flags & (jobj::flags::SKELETON | jobj::flags::SKELETON_ROOT) != 0 {
            break index;
        }
        candidate = joint.parent;
    };
    let skeleton = &root.joints[skeleton_index.0];

    let correction = if skeleton_index == model_node_index {
        let envelope_matrix =
            skeleton
                .inverse_bind_transform
                .ok_or(HsdDrawWorkError::MissingEnvelopeMatrix {
                    root_index,
                    polygon_source_id,
                    joint_index: skeleton_index,
                })?;
        psmtx_inverse_affine(envelope_matrix).ok_or(HsdDrawWorkError::SingularEnvelopeMatrix {
            root_index,
            polygon_source_id,
            joint_index: skeleton_index,
        })?
    } else if skeleton.flags & jobj::flags::SKELETON_ROOT != 0 {
        hsd_inverse_affine_or_identity(joint_world_matrices[skeleton_index.0])
            .mul(&joint_world_matrices[model_node_index.0])
    } else {
        let envelope_matrix =
            skeleton
                .inverse_bind_transform
                .ok_or(HsdDrawWorkError::MissingEnvelopeMatrix {
                    root_index,
                    polygon_source_id,
                    joint_index: skeleton_index,
                })?;
        let skeleton_envelope = joint_world_matrices[skeleton_index.0].mul(&envelope_matrix);
        hsd_inverse_affine_or_identity(skeleton_envelope)
            .mul(&joint_world_matrices[model_node_index.0])
    };
    Ok(Some(correction))
}

/// SDK `PSMTXInverse` rejects an exactly zero determinant and otherwise writes
/// an inverse. Its caller leaves no defined fallback output, so exact singularity
/// fails closed at this renderer-neutral boundary.
fn psmtx_inverse_affine(matrix: Mat4) -> Option<Mat4> {
    let determinant = affine_determinant(matrix);
    (determinant != 0.0).then(|| inverse_affine_with_determinant(matrix, determinant))
}

/// The ancestor branches use `HSD_MtxInverseConcat`, whose near-singular path
/// copies its source operand. Returning identity here gives the same result when
/// the caller post-multiplies by that source.
fn hsd_inverse_affine_or_identity(matrix: Mat4) -> Mat4 {
    let determinant = affine_determinant(matrix);
    if determinant.abs() < HSD_MATRIX_INVERSE_EPSILON {
        Mat4::identity()
    } else {
        inverse_affine_with_determinant(matrix, determinant)
    }
}

fn affine_determinant(matrix: Mat4) -> f32 {
    let a00 = matrix.0[0][0];
    let a01 = matrix.0[1][0];
    let a02 = matrix.0[2][0];
    let a10 = matrix.0[0][1];
    let a11 = matrix.0[1][1];
    let a12 = matrix.0[2][1];
    let a20 = matrix.0[0][2];
    let a21 = matrix.0[1][2];
    let a22 = matrix.0[2][2];

    a00 * a11 * a22 + a01 * a12 * a20 + a02 * a10 * a21
        - a20 * a11 * a02
        - a10 * a01 * a22
        - a00 * a21 * a12
}

fn inverse_affine_with_determinant(matrix: Mat4, determinant: f32) -> Mat4 {
    let a00 = matrix.0[0][0];
    let a01 = matrix.0[1][0];
    let a02 = matrix.0[2][0];
    let a10 = matrix.0[0][1];
    let a11 = matrix.0[1][1];
    let a12 = matrix.0[2][1];
    let a20 = matrix.0[0][2];
    let a21 = matrix.0[1][2];
    let a22 = matrix.0[2][2];
    let inverse_determinant = determinant.recip();
    let b00 = (a11 * a22 - a21 * a12) * inverse_determinant;
    let b01 = -(a01 * a22 - a21 * a02) * inverse_determinant;
    let b02 = (a01 * a12 - a11 * a02) * inverse_determinant;
    let b10 = -(a10 * a22 - a20 * a12) * inverse_determinant;
    let b11 = (a00 * a22 - a20 * a02) * inverse_determinant;
    let b12 = -(a00 * a12 - a10 * a02) * inverse_determinant;
    let b20 = (a10 * a21 - a20 * a11) * inverse_determinant;
    let b21 = -(a00 * a21 - a20 * a01) * inverse_determinant;
    let b22 = (a00 * a11 - a10 * a01) * inverse_determinant;
    let translation = matrix.0[3];

    Mat4([
        [b00, b10, b20, 0.0],
        [b01, b11, b21, 0.0],
        [b02, b12, b22, 0.0],
        [
            -(b00 * translation[0] + b01 * translation[1] + b02 * translation[2]),
            -(b10 * translation[0] + b11 * translation[1] + b12 * translation[2]),
            -(b20 * translation[0] + b21 * translation[1] + b22 * translation[2]),
            1.0,
        ],
    ])
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HsdDrawWorkError {
    #[error(transparent)]
    InvalidScene(#[from] HsdSceneError),
    #[error("draw expansion exceeds {resource} limit {limit}")]
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },
    #[error("visibility root {root_index} is outside root count {root_count}")]
    VisibilityRootOutOfRange {
        root_index: usize,
        root_count: usize,
    },
    #[error("root {root_index} has no display object {source_id:?}")]
    MissingDisplayObject {
        root_index: usize,
        source_id: DObjId,
    },
    #[error("root {root_index} joint {joint_index:?} has an invalid instance/owned-child contract")]
    InvalidJointContract {
        root_index: usize,
        joint_index: HsdJointIndex,
    },
    #[error("root {root_index} draw expansion cycles through joint {joint_index:?}")]
    InstanceCycle {
        root_index: usize,
        joint_index: HsdJointIndex,
    },
    #[error(
        "root {root_index} instance {joint_index:?} target {target:?} has an exactly singular world matrix"
    )]
    SingularInstanceTarget {
        root_index: usize,
        joint_index: HsdJointIndex,
        target: HsdJointIndex,
    },
    #[error(
        "root {root_index} instance {joint_index:?} target {target:?} produces a non-finite correction"
    )]
    NonFiniteInstanceCorrection {
        root_index: usize,
        joint_index: HsdJointIndex,
        target: HsdJointIndex,
    },
    #[error("root {root_index} contains duplicate joint source identities")]
    DuplicateJointSourceId { root_index: usize },
    #[error(
        "root {root_index} polygon {polygon_source_id:?} references missing rigid joint {source_id:?}"
    )]
    MissingRigidJoint {
        root_index: usize,
        polygon_source_id: PObjId,
        source_id: JObjId,
    },
    #[error(
        "root {root_index} polygon {polygon_source_id:?} model joint {joint_index:?} has no skeleton ancestor"
    )]
    MissingEnvelopeSkeleton {
        root_index: usize,
        polygon_source_id: PObjId,
        joint_index: HsdJointIndex,
    },
    #[error(
        "root {root_index} polygon {polygon_source_id:?} skeleton joint {joint_index:?} has no envelope matrix"
    )]
    MissingEnvelopeMatrix {
        root_index: usize,
        polygon_source_id: PObjId,
        joint_index: HsdJointIndex,
    },
    #[error(
        "root {root_index} polygon {polygon_source_id:?} skeleton joint {joint_index:?} has an exactly singular envelope matrix"
    )]
    SingularEnvelopeMatrix {
        root_index: usize,
        polygon_source_id: PObjId,
        joint_index: HsdJointIndex,
    },
    #[error("prepared draw topology does not match the supplied scene")]
    SceneTopologyMismatch,
    #[error("pose root {root_index} is outside root count {root_count}")]
    PoseRootOutOfRange {
        root_index: usize,
        root_count: usize,
    },
    #[error("root {root_index} has more than one supplied pose")]
    DuplicateRootPose { root_index: usize },
    #[error("root {root_index} pose has {actual} joints; expected {expected}")]
    PoseJointCountMismatch {
        root_index: usize,
        expected: usize,
        actual: usize,
    },
    #[error("root {root_index} visibility pose has {actual} joints; expected {expected}")]
    PoseVisibilityCountMismatch {
        root_index: usize,
        expected: usize,
        actual: usize,
    },
    #[error("root {root_index} INSTANCE joint {joint_index:?} changes visibility at runtime")]
    RuntimeInstanceVisibility {
        root_index: usize,
        joint_index: HsdJointIndex,
    },
    #[error("root {root_index} joint {joint_index:?} has a non-finite pose transform")]
    NonFinitePoseTransform {
        root_index: usize,
        joint_index: HsdJointIndex,
    },
    #[error("root {root_index} joint {joint_index:?} produced a non-finite world matrix")]
    NonFiniteWorldMatrix {
        root_index: usize,
        joint_index: HsdJointIndex,
    },
    #[error("root {root_index} polygon {polygon_source_id:?} evaluation failed: {source}")]
    Evaluation {
        root_index: usize,
        polygon_source_id: PObjId,
        #[source]
        source: HsdEnvelopeEvaluationError,
    },
}

#[cfg(test)]
mod tests;
