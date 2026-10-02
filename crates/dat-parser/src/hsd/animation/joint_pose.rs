//! A model root's joint pose, driven by one scalar AObj per animated joint.
//!
//! The controllers come from a Figa tree bound to explicit archive-local
//! JObj identities ([`HsdJointPoseEvaluator::from_figatree`]) or are attached
//! joint by joint, as an AnimJoint tree is (`anim_joint.rs`). Either way this
//! is a fresh attachment, not fighter part selection or animation blending.
//! Unanimated components start at the serialized local pose and retain their
//! values across requests. NODE/BRANCH controls update runtime `JOBJ_HIDDEN`
//! state from the serialized flags. Callers own game-driven root updates, other
//! receivers, and matrix policy (including Figa's CLASSICAL_SCALE selection).

use std::collections::HashMap;

use thiserror::Error;

use super::{FObjStreamF32, HsdAObjError, HsdAObjEvaluator, HsdAObjFObj, HsdJointChannel};
use crate::descriptor::animation::{MAX_FIGA_COUNT_LIST_ENTRIES, MAX_FIGA_TRACKS, RawFigaTree};
use crate::descriptor::jobj::flags::{HIDDEN, INSTANCE};
use crate::hsd::draw::HsdRootPose;
use crate::hsd::scene::{HsdJointIndex, HsdScene, HsdTransform, JObjId};

#[derive(Clone, Copy, Debug)]
pub struct HsdJointPoseLimits {
    pub max_joints: usize,
    pub max_tracks: usize,
    /// Aggregate retained bytes, counting duplicate ranges once per track.
    pub max_packed_bytes: usize,
}

impl Default for HsdJointPoseLimits {
    fn default() -> Self {
        Self {
            max_joints: MAX_FIGA_COUNT_LIST_ENTRIES,
            max_tracks: MAX_FIGA_TRACKS,
            max_packed_bytes: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum HsdJointPoseError {
    #[error("joint pose exceeds the {resource} budget of {limit}")]
    ResourceLimit {
        resource: &'static str,
        limit: usize,
    },
    #[error("joint pose root {root_index} is out of range")]
    RootOutOfRange { root_index: usize },
    #[error("Figa has {expected} count-list entries but {actual} receiver identities")]
    ReceiverCountMismatch { expected: usize, actual: usize },
    #[error("JObj identity {source_id:?} is ambiguous in the model root")]
    AmbiguousJoint { source_id: JObjId },
    #[error("Figa receiver {source_id:?} is absent from the model root")]
    MissingReceiver { source_id: JObjId },
    #[error("Figa receiver {source_id:?} occurs more than once")]
    DuplicateReceiver { source_id: JObjId },
    #[error("Figa count-list entry {ordinal} disagrees with its track descriptors")]
    InvalidTrackLayout { ordinal: usize },
    #[error("Figa track {descriptor:#x} declares {declared} bytes but retains {actual}")]
    PackedLengthMismatch {
        descriptor: u32,
        declared: u16,
        actual: usize,
    },
    #[error("Figa track {descriptor:#x} has unsupported scalar receiver type {object_type}")]
    UnsupportedChannel { descriptor: u32, object_type: u8 },
    #[error("Figa end frame is not finite")]
    NonFiniteEndFrame,
    #[error("Figa request frame is not finite")]
    NonFiniteRequestFrame,
    #[error("Figa rate is not finite")]
    NonFiniteRate,
    #[error("Figa local transform for joint {joint_index:?} is not finite")]
    NonFiniteTransform { joint_index: HsdJointIndex },
    #[error("Figa local joint {joint_index:?} is out of range")]
    JointOutOfRange { joint_index: HsdJointIndex },
    #[error("Figa playback for joint {joint_index:?} failed: {source}")]
    Playback {
        joint_index: HsdJointIndex,
        source: HsdAObjError,
    },
}

#[derive(Debug)]
struct JointAnimation<'a> {
    joint_index: HsdJointIndex,
    lifecycle: HsdAObjEvaluator<'a, HsdJointChannel>,
}

/// Reusable scalar pose with one source AObj lifecycle per attached joint.
///
/// A failed tick permanently poisons this attachment: earlier updates may have
/// occurred, and the partial pose must not be exposed as a successful frame.
#[derive(Debug)]
pub struct HsdJointPoseEvaluator<'a> {
    root_index: usize,
    transforms: Vec<HsdTransform>,
    /// Runtime `JOBJ_HIDDEN`, seeded from the serialized flags.
    hidden: Vec<bool>,
    /// Owned children for recursive BRANCH updates; empty below an INSTANCE.
    branch_children: Vec<Vec<HsdJointIndex>>,
    joints: Vec<JointAnimation<'a>>,
    max_tracks: usize,
    max_packed_bytes: usize,
    failed: Option<HsdJointPoseError>,
}

impl<'a> HsdJointPoseEvaluator<'a> {
    /// Bind each count-list entry to an explicit identity in one model root.
    ///
    /// `receivers` must come from the caller's source-established attachment
    /// relation, not a guess that count-list ordinal equals dense joint index.
    /// Only scalar and NODE/BRANCH JObj channels are admitted; others are errors.
    pub fn from_figatree(
        scene: &HsdScene,
        root_index: usize,
        tree: &RawFigaTree<'a>,
        receivers: &[JObjId],
        limits: HsdJointPoseLimits,
    ) -> Result<Self, HsdJointPoseError> {
        let root = scene
            .roots
            .get(root_index)
            .ok_or(HsdJointPoseError::RootOutOfRange { root_index })?;
        if root.joints.len() > limits.max_joints || receivers.len() > limits.max_joints {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "joints",
                limit: limits.max_joints,
            });
        }
        if tree.tracks.len() > limits.max_tracks {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "tracks",
                limit: limits.max_tracks,
            });
        }
        let packed_bytes = tree.tracks.iter().try_fold(0usize, |total, track| {
            total.checked_add(track.packed_data.len())
        });
        if packed_bytes.is_none_or(|total| total > limits.max_packed_bytes) {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "packed bytes",
                limit: limits.max_packed_bytes,
            });
        }
        if receivers.len() != tree.track_counts.len() {
            return Err(HsdJointPoseError::ReceiverCountMismatch {
                expected: tree.track_counts.len(),
                actual: receivers.len(),
            });
        }
        if !tree.end_frame.is_finite() {
            return Err(HsdJointPoseError::NonFiniteEndFrame);
        }

        let mut identities = HashMap::with_capacity(root.joints.len());
        for (index, joint) in root.joints.iter().enumerate() {
            let joint_index = HsdJointIndex(index);
            validate_transform(joint_index, joint.local)?;
            if identities
                .insert(joint.source_id, (joint_index, false))
                .is_some()
            {
                return Err(HsdJointPoseError::AmbiguousJoint {
                    source_id: joint.source_id,
                });
            }
        }
        let mut bound = Vec::with_capacity(receivers.len());
        let mut cursor = 0usize;
        for (ordinal, (&source_id, &count)) in receivers.iter().zip(&tree.track_counts).enumerate()
        {
            let (joint_index, used) = identities
                .get_mut(&source_id)
                .ok_or(HsdJointPoseError::MissingReceiver { source_id })?;
            if *used {
                return Err(HsdJointPoseError::DuplicateReceiver { source_id });
            }
            *used = true;
            let count = usize::try_from(count)
                .map_err(|_| HsdJointPoseError::InvalidTrackLayout { ordinal })?;
            let end = cursor
                .checked_add(count)
                .ok_or(HsdJointPoseError::InvalidTrackLayout { ordinal })?;
            let tracks = tree
                .tracks
                .get(cursor..end)
                .ok_or(HsdJointPoseError::InvalidTrackLayout { ordinal })?;
            for track in tracks {
                if track.count_list_ordinal != ordinal {
                    return Err(HsdJointPoseError::InvalidTrackLayout { ordinal });
                }
                if usize::from(track.length) != track.packed_data.len() {
                    return Err(HsdJointPoseError::PackedLengthMismatch {
                        descriptor: track.descriptor_offset,
                        declared: track.length,
                        actual: track.packed_data.len(),
                    });
                }
                if HsdJointChannel::from_joint_object_type(track.object_type).is_none() {
                    return Err(HsdJointPoseError::UnsupportedChannel {
                        descriptor: track.descriptor_offset,
                        object_type: track.object_type,
                    });
                }
            }
            if count != 0 {
                bound.push((*joint_index, cursor..end));
            }
            cursor = end;
        }
        if cursor != tree.tracks.len() {
            return Err(HsdJointPoseError::InvalidTrackLayout {
                ordinal: receivers.len(),
            });
        }

        // JObjAnimAll visits the model in depth-first order. Within each joint,
        // lbAnim_InitFrames preserves descriptor order, then lbAnim_JObjSortAnim
        // moves only the first BRANCH FObj to the head of the list.
        bound.sort_unstable_by_key(|(joint_index, _)| joint_index.0);
        let mut joints = Vec::with_capacity(bound.len());
        for (joint_index, range) in bound {
            let mut tracks = tree.tracks[range].iter().collect::<Vec<_>>();
            if let Some(branch) = tracks.iter().position(|track| {
                HsdJointChannel::from_joint_object_type(track.object_type)
                    == Some(HsdJointChannel::Branch)
            }) {
                tracks[..=branch].rotate_right(1);
            }
            let lifecycle = HsdAObjEvaluator::new(
                tree.flags,
                tree.end_frame,
                tracks.into_iter().map(|track| HsdAObjFObj {
                    // The complete immutable descriptor list was preflighted above.
                    metadata: HsdJointChannel::from_joint_object_type(track.object_type)
                        .expect("preflighted joint channel"),
                    stream: FObjStreamF32 {
                        start_frame: f32::from(track.start_frame as i16),
                        frac_value: track.frac_value,
                        frac_slope: track.frac_slope,
                        packed_data: track.packed_data,
                    },
                }),
                limits.max_tracks,
            )
            .map_err(|source| HsdJointPoseError::Playback {
                joint_index,
                source,
            })?;
            joints.push(JointAnimation {
                joint_index,
                lifecycle,
            });
        }
        Ok(Self {
            root_index,
            transforms: root.joints.iter().map(|joint| joint.local).collect(),
            hidden: root
                .joints
                .iter()
                .map(|joint| joint.flags & HIDDEN != 0)
                .collect(),
            branch_children: root
                .joints
                .iter()
                .map(|joint| {
                    if joint.flags & INSTANCE != 0 {
                        Vec::new()
                    } else {
                        joint.children.clone()
                    }
                })
                .collect(),
            joints,
            max_tracks: limits.max_tracks,
            max_packed_bytes: limits.max_packed_bytes,
            failed: None,
        })
    }

    /// A model root at its serialized pose, with no controllers yet.
    ///
    /// For animations that arrive one joint at a time rather than as a Figa
    /// tree: an AnimJoint tree attaches each joint's AObj with
    /// [`Self::attach_joint_animation`].
    pub fn unanimated(
        scene: &HsdScene,
        root_index: usize,
        limits: HsdJointPoseLimits,
    ) -> Result<Self, HsdJointPoseError> {
        let root = scene
            .roots
            .get(root_index)
            .ok_or(HsdJointPoseError::RootOutOfRange { root_index })?;
        if root.joints.len() > limits.max_joints {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "joints",
                limit: limits.max_joints,
            });
        }
        for (index, joint) in root.joints.iter().enumerate() {
            validate_transform(HsdJointIndex(index), joint.local)?;
        }
        Ok(Self {
            root_index,
            transforms: root.joints.iter().map(|joint| joint.local).collect(),
            hidden: root
                .joints
                .iter()
                .map(|joint| joint.flags & HIDDEN != 0)
                .collect(),
            branch_children: root
                .joints
                .iter()
                .map(|joint| {
                    if joint.flags & INSTANCE != 0 {
                        Vec::new()
                    } else {
                        joint.children.clone()
                    }
                })
                .collect(),
            joints: Vec::new(),
            max_tracks: limits.max_tracks,
            max_packed_bytes: limits.max_packed_bytes,
            failed: None,
        })
    }

    /// Give one joint a controller over transform and NODE/BRANCH channels,
    /// replacing any it had. The budgets count every controller but the one
    /// replaced, and a refused controller changes nothing.
    pub fn attach_joint_animation(
        &mut self,
        joint_index: HsdJointIndex,
        lifecycle: HsdAObjEvaluator<'a, HsdJointChannel>,
    ) -> Result<(), HsdJointPoseError> {
        self.validate_joint_animation_replacement(joint_index, &lifecycle)?;
        let attached = JointAnimation {
            joint_index,
            lifecycle,
        };
        match self
            .joints
            .binary_search_by_key(&joint_index.0, |joint| joint.joint_index.0)
        {
            Ok(index) => self.joints[index] = attached,
            Err(index) => self.joints.insert(index, attached),
        }
        Ok(())
    }

    /// Whether any joint has a controller.
    pub fn is_animated(&self) -> bool {
        !self.joints.is_empty()
    }

    /// The longest controller's end frame.
    pub fn end_frame(&self) -> f32 {
        self.joints
            .iter()
            .map(|joint| joint.lifecycle.end_frame())
            .fold(0.0, f32::max)
    }

    /// Loop every controller, as `HSD_AObjSetFlags(AOBJ_LOOP)` over the tree.
    pub fn set_looping(&mut self, looping: bool) {
        for joint in &mut self.joints {
            joint.lifecycle.set_looping(looping);
        }
    }

    /// Own packed streams while preserving local pose, lifecycle and poison state.
    /// Construction and replacements have already checked the aggregate budget.
    pub fn into_owned(self) -> HsdJointPoseEvaluator<'static> {
        HsdJointPoseEvaluator {
            root_index: self.root_index,
            transforms: self.transforms,
            hidden: self.hidden,
            branch_children: self.branch_children,
            joints: self
                .joints
                .into_iter()
                .map(|joint| JointAnimation {
                    joint_index: joint.joint_index,
                    lifecycle: joint.lifecycle.into_owned(),
                })
                .collect(),
            max_tracks: self.max_tracks,
            max_packed_bytes: self.max_packed_bytes,
            failed: self.failed,
        }
    }

    /// Preflight a borrowed controller before copying its packed bytes.
    ///
    /// Both aggregate budgets exclude the controller being replaced. Duplicate
    /// byte ranges count per track; arithmetic overflow is a budget failure.
    /// This does not mutate either controller and also rejects poisoned poses.
    pub fn validate_joint_animation_replacement<M>(
        &self,
        joint_index: HsdJointIndex,
        lifecycle: &HsdAObjEvaluator<'_, M>,
    ) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        if joint_index.0 >= self.transforms.len() {
            return Err(HsdJointPoseError::JointOutOfRange { joint_index });
        }
        let total = self
            .joints
            .iter()
            .try_fold(lifecycle.len(), |total, joint| {
                if joint.joint_index == joint_index {
                    Some(total)
                } else {
                    total.checked_add(joint.lifecycle.len())
                }
            });
        if total.is_none_or(|total| total > self.max_tracks) {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "tracks",
                limit: self.max_tracks,
            });
        }
        let packed_bytes = lifecycle.checked_packed_byte_len().and_then(|incoming| {
            self.joints.iter().try_fold(incoming, |total, joint| {
                if joint.joint_index == joint_index {
                    Some(total)
                } else {
                    total.checked_add(joint.lifecycle.checked_packed_byte_len()?)
                }
            })
        });
        if packed_bytes.is_none_or(|total| total > self.max_packed_bytes) {
            return Err(HsdJointPoseError::ResourceLimit {
                resource: "packed bytes",
                limit: self.max_packed_bytes,
            });
        }
        Ok(())
    }

    pub fn request(&mut self, frame: f32) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        if !frame.is_finite() {
            return Err(HsdJointPoseError::NonFiniteRequestFrame);
        }
        // Compact starts are bounded s16 values; adding one to a finite f32
        // request cannot overflow. Each AObj still performs its own validation.
        for joint in &mut self.joints {
            joint
                .lifecycle
                .request(frame)
                .map_err(|source| HsdJointPoseError::Playback {
                    joint_index: joint.joint_index,
                    source,
                })?;
        }
        Ok(())
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        if !rate.is_finite() {
            return Err(HsdJointPoseError::NonFiniteRate);
        }
        for joint in &mut self.joints {
            joint
                .lifecycle
                .set_rate(rate)
                .map_err(|source| HsdJointPoseError::Playback {
                    joint_index: joint.joint_index,
                    source,
                })?;
        }
        Ok(())
    }

    pub fn set_updates_suppressed(&mut self, suppressed: bool) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        for joint in &mut self.joints {
            joint.lifecycle.set_updates_suppressed(suppressed);
        }
        Ok(())
    }

    /// Apply caller-owned local state, such as game root placement, before drawing.
    pub fn set_local_transform(
        &mut self,
        joint_index: HsdJointIndex,
        transform: HsdTransform,
    ) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        let target = self
            .transforms
            .get_mut(joint_index.0)
            .ok_or(HsdJointPoseError::JointOutOfRange { joint_index })?;
        validate_transform(joint_index, transform)?;
        *target = transform;
        Ok(())
    }

    pub fn is_stopped(&self) -> bool {
        self.joints.iter().all(|joint| joint.lifecycle.is_stopped())
    }

    /// Advance every joint once without allocating or resetting unanimated state.
    pub fn advance(&mut self) -> Result<HsdRootPose<'_>, HsdJointPoseError> {
        self.ensure_usable()?;
        for joint in &mut self.joints {
            let mut state = JointState {
                joint_index: joint.joint_index,
                transforms: &mut self.transforms,
                hidden: &mut self.hidden,
                branch_children: &self.branch_children,
            };
            if let Err(source) = joint
                .lifecycle
                .advance(|channel, value| state.apply(*channel, value))
            {
                let error = HsdJointPoseError::Playback {
                    joint_index: joint.joint_index,
                    source,
                };
                self.failed = Some(error);
                return Err(error);
            }
        }
        self.pose()
    }

    /// Flush each joint's pending KEY updates and persistently stop all AObjs.
    pub fn stop(&mut self) -> Result<(), HsdJointPoseError> {
        self.ensure_usable()?;
        for joint in &mut self.joints {
            let mut state = JointState {
                joint_index: joint.joint_index,
                transforms: &mut self.transforms,
                hidden: &mut self.hidden,
                branch_children: &self.branch_children,
            };
            if let Err(source) = joint
                .lifecycle
                .stop(|channel, value| state.apply(*channel, value))
            {
                self.failed.get_or_insert(HsdJointPoseError::Playback {
                    joint_index: joint.joint_index,
                    source,
                });
            }
        }
        self.ensure_usable()
    }

    pub fn pose(&self) -> Result<HsdRootPose<'_>, HsdJointPoseError> {
        self.ensure_usable()?;
        Ok(HsdRootPose {
            root_index: self.root_index,
            transforms: &self.transforms,
            hidden_joints: Some(&self.hidden),
        })
    }

    fn ensure_usable(&self) -> Result<(), HsdJointPoseError> {
        self.failed.map_or(Ok(()), Err)
    }
}

/// One joint's mutable view of the pose while its AObj delivers updates.
struct JointState<'s> {
    joint_index: HsdJointIndex,
    transforms: &'s mut [HsdTransform],
    hidden: &'s mut [bool],
    branch_children: &'s [Vec<HsdJointIndex>],
}

impl JointState<'_> {
    fn apply(&mut self, channel: HsdJointChannel, value: f32) {
        let index = self.joint_index.0;
        match channel {
            HsdJointChannel::Transform(channel) => {
                channel.apply_to_transform(value, &mut self.transforms[index]);
            }
            HsdJointChannel::Node => self.hidden[index] = HsdJointChannel::hides(value),
            HsdJointChannel::Branch => set_hidden_all(
                self.hidden,
                self.branch_children,
                self.joint_index,
                HsdJointChannel::hides(value),
            ),
        }
    }
}

/// HSD_JObjSetFlagsAll/ClearFlagsAll: flag the JObj and everything under it
/// through owned children, stopping at an INSTANCE. The source recurses; a
/// stage may chain as many joints as its budget allows, so this keeps its own
/// stack.
fn set_hidden_all(
    hidden: &mut [bool],
    branch_children: &[Vec<HsdJointIndex>],
    joint: HsdJointIndex,
    value: bool,
) {
    let mut pending = vec![joint];
    while let Some(joint) = pending.pop() {
        hidden[joint.0] = value;
        pending.extend(&branch_children[joint.0]);
    }
}

fn validate_transform(
    joint_index: HsdJointIndex,
    transform: HsdTransform,
) -> Result<(), HsdJointPoseError> {
    if transform
        .scale
        .into_iter()
        .chain(transform.rotation)
        .chain(transform.translation)
        .all(f32::is_finite)
    {
        Ok(())
    } else {
        Err(HsdJointPoseError::NonFiniteTransform { joint_index })
    }
}

#[cfg(test)]
mod tests {
    use super::{HsdJointIndex, set_hidden_all};

    #[test]
    fn hiding_a_branch_reaches_the_end_of_a_chain_too_deep_to_recurse() {
        // One child each, 200,000 deep: recursion would run out of stack.
        const JOINTS: usize = 200_000;
        let children: Vec<Vec<HsdJointIndex>> = (0..JOINTS)
            .map(|joint| {
                if joint + 1 < JOINTS {
                    vec![HsdJointIndex(joint + 1)]
                } else {
                    Vec::new()
                }
            })
            .collect();
        let mut hidden = vec![false; JOINTS];
        set_hidden_all(&mut hidden, &children, HsdJointIndex(1), true);
        assert!(!hidden[0], "the branch's parent keeps its own flag");
        assert!(hidden[1..].iter().all(|&hidden| hidden));
    }
}
