//! Renderer-neutral HSD animation contracts and evaluation.
//!
//! `fobj` owns packed scalar stream interpretation, `aobj` owns source-ordered
//! playback lifecycle, and `channel` maps joint scalar receivers.

mod anim_joint;
mod aobj;
mod channel;
mod fobj;
mod joint_pose;

pub use anim_joint::attach_anim_joints;
pub use aobj::{HsdAObjError, HsdAObjEvaluator, HsdAObjFObj, HsdAObjTick, aobj_flags};
pub use channel::{HsdAnimationChannel, HsdJointChannel};
pub use fobj::{
    FObjEvaluationError, FObjEvaluator, FObjStream, FObjStreamF32, sample_fobj_integer_frames,
};
pub use joint_pose::{HsdJointPoseError, HsdJointPoseEvaluator, HsdJointPoseLimits};
