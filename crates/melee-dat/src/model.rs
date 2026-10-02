//! A DAT loaded to show: a fighter playing its animations, a stage playing
//! the ones it loads with, or anything else in its serialized pose.
//!
//! Every host draws a model the same way (evaluate, hand the draw work to
//! the renderer) and drives it the same way (advance, seek, rate), whatever
//! it is. The fighter-only parts, picking a move and its texture places,
//! stay on [`MeleeFighterPlayback`].

use crate::catalog::MeleeReferenceCatalog;
use crate::error::{MeleeError, Result};
use crate::fighter::playback::{FighterAttach, MeleeFighterPlayback};
use crate::references::MeleeReferenceStore;
use crate::stage::playback::MeleeStagePlayback;
use dat_parser::descriptor::map_head::MapHead;
use dat_parser::hsd::draw::{HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork};
use dat_parser::hsd::scene::HsdScene;
use dat_parser::hsd::source::{self, HsdFocus, HsdSource};

/// A loaded DAT: see the module documentation.
pub struct MeleeModel(Model);

enum Model {
    Fighter(Box<MeleeFighterPlayback>),
    Stage(Box<MeleeStagePlayback>),
    Static(Box<HsdSource>),
}
use Model::{Fighter, Stage, Static};

/// What a [`MeleeModel`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeleeModelKind {
    /// A fighter playing its animations.
    Fighter,
    /// A stage playing the animations it loads with.
    Stage,
    /// Anything else, in its serialized pose.
    Static,
}

/// What [`MeleeModel::attach_fighter`] did.
#[derive(Debug)]
#[non_exhaustive]
pub enum FighterAttachOutcome {
    /// The model is a fighter playing its idle.
    Attached,
    /// The model is not a stock fighter's costume the catalog recognizes (or
    /// is a stage, or a fighter already), and stays as it was.
    NotAFighter,
    /// The catalog recognizes the costume and its idle did not attach; the
    /// model stays in its serialized pose.
    Failed(MeleeError),
}

impl MeleeModel {
    /// Load a DAT. One with a `map_head` is a stage; anything else is drawn
    /// under `policy` in its serialized pose until
    /// [`Self::attach_fighter`] gives it a fighter's animations.
    pub fn open(bytes: &[u8], policy: HsdDrawEvaluationPolicy) -> Result<Self> {
        let dat = source::parse(bytes)?;
        Ok(Self(if MapHead::find(&dat).is_some() {
            Stage(Box::new(MeleeStagePlayback::attach(&dat)?))
        } else {
            Static(Box::new(HsdSource::from_parsed(&dat, policy)?))
        }))
    }

    /// Play the fighter's animations on a model still in its serialized
    /// pose. The model always comes back, with what happened.
    pub fn attach_fighter(
        self,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
    ) -> (Self, FighterAttachOutcome) {
        let Static(source) = self.0 else {
            return (self, FighterAttachOutcome::NotAFighter);
        };
        match MeleeFighterPlayback::attach(*source, catalog, store) {
            FighterAttach::Attached(playback) => {
                (Self(Fighter(playback)), FighterAttachOutcome::Attached)
            }
            FighterAttach::Unrecognized(source) => {
                (Self(Static(source)), FighterAttachOutcome::NotAFighter)
            }
            FighterAttach::Failed { source, error } => {
                (Self(Static(source)), FighterAttachOutcome::Failed(error))
            }
        }
    }

    pub fn kind(&self) -> MeleeModelKind {
        match &self.0 {
            Fighter(_) => MeleeModelKind::Fighter,
            Stage(_) => MeleeModelKind::Stage,
            Static(_) => MeleeModelKind::Static,
        }
    }

    /// The evaluation policy a model in its serialized pose was opened
    /// under; `None` for a fighter or a stage, whose playback fixes it.
    pub fn static_policy(&self) -> Option<HsdDrawEvaluationPolicy> {
        match &self.0 {
            Static(source) => Some(source.policy),
            Fighter(_) | Stage(_) => None,
        }
    }

    pub fn scene(&self) -> &HsdScene {
        match &self.0 {
            Fighter(playback) => playback.scene(),
            Stage(playback) => playback.scene(),
            Static(source) => &source.scene,
        }
    }

    /// What a viewer should frame, when that is less than the whole model.
    pub fn focus(&self) -> Option<HsdFocus> {
        match &self.0 {
            Fighter(_) => None,
            Stage(playback) => playback.focus(),
            Static(source) => source.focus,
        }
    }

    /// The fighter's playback, for what only a fighter has: its moves.
    pub fn fighter(&self) -> Option<&MeleeFighterPlayback> {
        match &self.0 {
            Fighter(playback) => Some(playback),
            Stage(_) | Static(_) => None,
        }
    }

    pub fn fighter_mut(&mut self) -> Option<&mut MeleeFighterPlayback> {
        match &mut self.0 {
            Fighter(playback) => Some(playback),
            Stage(_) | Static(_) => None,
        }
    }

    /// Whether advancing changes the pose.
    pub fn is_animated(&self) -> bool {
        match &self.0 {
            Fighter(_) => true,
            Stage(playback) => playback.is_animated(),
            Static(_) => false,
        }
    }

    pub fn frame(&self) -> f32 {
        match &self.0 {
            Fighter(playback) => playback.frame(),
            Stage(playback) => playback.frame(),
            Static(_) => 0.0,
        }
    }

    pub fn end_frame(&self) -> f32 {
        match &self.0 {
            Fighter(playback) => playback.end_frame(),
            Stage(playback) => playback.end_frame(),
            Static(_) => 0.0,
        }
    }

    /// Frames advanced per tick: 1 plays at the game's speed.
    pub fn rate(&self) -> f32 {
        match &self.0 {
            Fighter(playback) => playback.rate(),
            Stage(playback) => playback.rate(),
            Static(_) => 1.0,
        }
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        match &mut self.0 {
            Fighter(playback) => playback.set_rate(rate),
            Stage(playback) => playback.set_rate(rate),
            Static(_) => Ok(()),
        }
    }

    /// Show `frame` of what is playing.
    pub fn seek(&mut self, frame: f32) -> Result<()> {
        match &mut self.0 {
            Fighter(playback) => playback.seek(frame),
            Stage(playback) => playback.seek(frame),
            Static(_) => Ok(()),
        }
    }

    /// Advance one 60 Hz tick.
    pub fn advance(&mut self) -> Result<()> {
        match &mut self.0 {
            Fighter(playback) => playback.advance(),
            Stage(playback) => playback.advance(),
            Static(_) => Ok(()),
        }
    }

    /// Evaluate the current pose; returns the scene with its draw work.
    pub fn evaluate(&mut self) -> Result<(&HsdScene, &HsdEvaluatedDrawWork)> {
        match &mut self.0 {
            Fighter(playback) => playback.evaluate(),
            Stage(playback) => playback.evaluate(),
            Static(source) => Ok(source.evaluate_bind_pose()?),
        }
    }
}
