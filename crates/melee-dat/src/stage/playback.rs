//! A stage playing the animations it loads with.
//!
//! Each of `map_head`'s model groups carries banks of joint, material, and
//! shape animations. When the game creates a group's object it nearly always
//! starts animation 0 of each bank (`grAnime_801C8138(gobj, map_id, 0)` in
//! 145 of the stage modules' calls), loops it when the group's flag byte for
//! that animation is set, and ticks it once before the first frame.
//!
//! This plays joint animation 0 of every group, which is the stage as it
//! first appears. It does not run what a stage's own code does afterwards:
//! Pokemon Stadium's transformations, Corneria's Arwings, or the choice
//! among a group's other animations. Material and shape animations are not
//! played. A group whose animation does not read keeps its serialized pose.

use crate::error::{MeleeError, Result};
use dat_parser::DatFile;
use dat_parser::descriptor::generic_animation::RawAnimJointGraph;
use dat_parser::descriptor::map_head::MapHead;
use dat_parser::hsd::animation::{HsdJointPoseEvaluator, HsdJointPoseLimits, attach_anim_joints};
use dat_parser::hsd::draw::{HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork};
use dat_parser::hsd::scene::HsdScene;
use dat_parser::hsd::source::{HsdFocus, HsdSource};
use dat_parser::math::Mat4;

const ANIMATION: u32 = 0;
/// The furthest a seek plays to: ten minutes of frames. Seeking replays from
/// the load pose, so a file claiming a longer animation cannot stall it.
const MAX_SEEK_FRAMES: f32 = 36_000.0;

pub struct MeleeStagePlayback {
    source: HsdSource,
    /// One pose per model root that has a joint animation.
    poses: Vec<HsdJointPoseEvaluator<'static>>,
    end_frame: f32,
    frame: f32,
    rate: f32,
}

impl MeleeStagePlayback {
    /// Load a stage DAT and start its groups' animations.
    pub fn attach(dat: &DatFile) -> Result<Self> {
        let mut source = HsdSource::from_parsed(dat, HsdDrawEvaluationPolicy::GENERIC_HSD)?;
        // A stage's backdrop makes the whole scene many times larger than
        // the part fighters play on.
        source.focus = super::points::camera_focus(dat);
        let poses: Vec<_> = group_animations(dat)
            .into_iter()
            .filter_map(|group| {
                let root_index = source
                    .scene
                    .roots
                    .iter()
                    .position(|root| root.source_id.0 == group.root)?;
                let graph = RawAnimJointGraph::parse(dat, group.anim_joint).ok()?;
                let limits = HsdJointPoseLimits {
                    max_joints: source.scene.roots[root_index].joints.len(),
                    ..HsdJointPoseLimits::default()
                };
                let mut pose =
                    attach_anim_joints(&source.scene, root_index, &graph, limits).ok()?;
                if !pose.is_animated() {
                    return None;
                }
                if group.loops {
                    pose.set_looping(true);
                }
                Some(pose.into_owned())
            })
            .collect();
        let end_frame = poses
            .iter()
            .map(HsdJointPoseEvaluator::end_frame)
            .fold(0.0, f32::max);
        let mut playback = Self {
            source,
            poses,
            end_frame,
            frame: 0.0,
            rate: 1.0,
        };
        playback.reset()?;
        Ok(playback)
    }

    /// Face the model's billboarded joints toward a camera with this `view`
    /// in later evaluations; `None` leaves them as posed.
    pub fn set_view(&mut self, view: Option<Mat4>) {
        self.source.evaluator.set_view(view);
    }

    pub fn scene(&self) -> &HsdScene {
        &self.source.scene
    }

    /// The stage's camera range.
    pub fn focus(&self) -> Option<HsdFocus> {
        self.source.focus
    }

    /// Whether any group has a joint animation.
    pub fn is_animated(&self) -> bool {
        !self.poses.is_empty()
    }

    /// Frames since the stage loaded.
    pub fn frame(&self) -> f32 {
        self.frame
    }

    /// The longest animation's length. Looping groups each repeat at their
    /// own length, so the stage as a whole need not repeat at this one.
    pub fn end_frame(&self) -> f32 {
        self.end_frame
    }

    /// Frames advanced per tick: 1 plays at the game's speed.
    pub fn rate(&self) -> f32 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        if !(rate.is_finite() && rate > 0.0) {
            return Err(MeleeError::InvalidRate(rate));
        }
        self.rate = rate;
        for pose in &mut self.poses {
            pose.set_rate(rate)?;
        }
        Ok(())
    }

    /// Advance one 60 Hz tick.
    pub fn advance(&mut self) -> Result<()> {
        for pose in &mut self.poses {
            pose.advance()?;
        }
        self.frame += self.rate;
        Ok(())
    }

    /// The stage as it loads: every animation requested at frame 0 and
    /// ticked once (`HSD_JObjReqAnimAll`, then `HSD_JObjAnimAll`).
    pub fn reset(&mut self) -> Result<()> {
        for pose in &mut self.poses {
            pose.set_rate(self.rate)?;
            pose.request(0.0)?;
            pose.advance()?;
        }
        self.frame = 0.0;
        Ok(())
    }

    /// Show the stage `frame` frames after it loads, by playing up to it at
    /// the game's speed: groups loop at their own lengths, so no single
    /// request reaches a later frame.
    pub fn seek(&mut self, frame: f32) -> Result<()> {
        if frame.is_nan() {
            return Err(MeleeError::InvalidFrame);
        }
        let rate = self.rate;
        self.rate = 1.0;
        self.reset()?;
        let frame = frame
            .clamp(0.0, self.end_frame.max(0.0))
            .min(MAX_SEEK_FRAMES);
        for _ in 0..frame.round() as u32 {
            self.advance()?;
        }
        self.set_rate(rate)
    }

    /// Evaluate the current pose; returns the scene with its draw work.
    pub fn evaluate(&mut self) -> Result<(&HsdScene, &HsdEvaluatedDrawWork)> {
        let poses = self
            .poses
            .iter()
            .map(HsdJointPoseEvaluator::pose)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let work = self.source.evaluator.evaluate(&self.source.scene, &poses)?;
        Ok((&self.source.scene, work))
    }
}

/// A model group's root joint with the joint animation it starts on.
struct GroupAnimation {
    root: u32,
    anim_joint: u32,
    loops: bool,
}

/// Every model group that has a joint animation 0. A group that does not
/// read is left out rather than failing the stage.
fn group_animations(dat: &DatFile) -> Vec<GroupAnimation> {
    MapHead::find(dat)
        .and_then(|map_head| map_head.model_groups().ok())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|group| {
            Some(GroupAnimation {
                root: group.root().ok()??,
                anim_joint: group.anim_joint(ANIMATION).ok()??,
                loops: group.loops(ANIMATION).unwrap_or(false),
            })
        })
        .collect()
}
