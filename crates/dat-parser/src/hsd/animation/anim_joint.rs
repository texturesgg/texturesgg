//! Attach an AnimJoint tree to a model root, as `HSD_JObjAddAnimAll` does.
//!
//! The source walks the JObj tree and the AnimJoint tree side by side: a
//! joint's children pair with the AnimJoint's children in sibling order, and
//! the walk does not enter an INSTANCE joint's children. A joint with no
//! AnimJoint beside it, or whose AnimJoint has no AObj, keeps its pose. The
//! root's own siblings are not visited.
//!
//! Only the transform and NODE/BRANCH tracks are attached. PATH tracks (the
//! joint follows a spline), the SETBYTE/SETFLOAT user tracks, and RObj
//! animations are left out, so a joint driven by one of those holds still.

use super::{
    FObjStreamF32, HsdAObjEvaluator, HsdAObjFObj, HsdJointChannel, HsdJointPoseError,
    HsdJointPoseEvaluator, HsdJointPoseLimits,
};
use crate::descriptor::generic_animation::RawAnimJointGraph;
use crate::descriptor::jobj::flags::INSTANCE;
use crate::hsd::scene::{HsdJointIndex, HsdScene, HsdSceneRoot};

/// A pose for `scene`'s root `root_index` driven by `graph`, not yet requested.
pub fn attach_anim_joints<'a>(
    scene: &HsdScene,
    root_index: usize,
    graph: &RawAnimJointGraph<'a>,
    limits: HsdJointPoseLimits,
) -> Result<HsdJointPoseEvaluator<'a>, HsdJointPoseError> {
    let mut pose = HsdJointPoseEvaluator::unanimated(scene, root_index, limits)?;
    let root = &scene.roots[root_index];
    for anim_joint in &graph.joints {
        let (Some(aobj), Some(joint_index)) =
            (&anim_joint.aobj, joint_at(root, &anim_joint.position))
        else {
            continue;
        };
        let tracks: Vec<_> = aobj
            .fobjs
            .iter()
            .filter_map(|fobj| {
                Some(HsdAObjFObj {
                    metadata: HsdJointChannel::from_joint_object_type(fobj.object_type)?,
                    stream: FObjStreamF32 {
                        start_frame: f32::from_bits(fobj.start_frame_bits),
                        frac_value: fobj.frac_value,
                        frac_slope: fobj.frac_slope,
                        packed_data: fobj.packed_data,
                    },
                })
            })
            .collect();
        if tracks.is_empty() {
            continue;
        }
        let lifecycle = HsdAObjEvaluator::new(
            aobj.raw_flags,
            f32::from_bits(aobj.end_frame_bits),
            tracks,
            limits.max_tracks,
        )
        .map_err(|source| HsdJointPoseError::Playback {
            joint_index,
            source,
        })?;
        pose.attach_joint_animation(joint_index, lifecycle)?;
    }
    Ok(pose)
}

/// The joint at `position`: child/sibling steps down from the root at `[0]`,
/// never through an INSTANCE joint's children. Following the path costs its
/// length, which the graph's parse already budgets; a map of every joint's
/// path would cost the square of a long chain.
fn joint_at(root: &HsdSceneRoot, position: &[usize]) -> Option<HsdJointIndex> {
    let (&0, steps) = position.split_first()? else {
        return None;
    };
    let mut index = HsdJointIndex(0);
    for &sibling in steps {
        let joint = root.joints.get(index.0)?;
        if joint.flags & INSTANCE != 0 {
            return None;
        }
        index = *joint.children.get(sibling)?;
    }
    root.joints.get(index.0).map(|_| index)
}
