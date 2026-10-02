//! A DAT loaded to show: a fighter playing its animations, a stage playing
//! the ones it loads with, or anything else in its serialized pose.
//!
//! Every host draws a model the same way (evaluate, hand the draw work to
//! the renderer) and drives it the same way (advance, seek, rate), whatever
//! it is. The fighter-only parts, picking a move and its texture places,
//! stay on [`MeleeFighterPlayback`].

use crate::catalog::MeleeReferenceCatalog;
use crate::error::{MeleeError, Result};
use crate::fighter::playback::MeleeFighterPlayback;
use crate::references::MeleeReferenceStore;
use crate::stage::playback::MeleeStagePlayback;
use dat_parser::descriptor::map_head::MapHead;
use dat_parser::hsd::draw::{HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork};
use dat_parser::hsd::scene::HsdScene;
use dat_parser::hsd::source::{self, HsdFocus, HsdSource};

pub enum MeleeModel {
    Fighter(Box<MeleeFighterPlayback>),
    Stage(Box<MeleeStagePlayback>),
    Static(Box<HsdSource>),
}

impl MeleeModel {
    /// Load a DAT. One with a `map_head` is a stage; anything else is drawn
    /// under `policy` in its serialized pose until
    /// [`Self::attach_fighter`] gives it a fighter's animations.
    pub fn open(bytes: &[u8], policy: HsdDrawEvaluationPolicy) -> Result<Self> {
        let dat = source::parse(bytes)?;
        Ok(if MapHead::find(&dat).is_some() {
            Self::Stage(Box::new(MeleeStagePlayback::attach(&dat)?))
        } else {
            Self::Static(Box::new(HsdSource::from_parsed(&dat, policy)?))
        })
    }

    /// Play the fighter's animations on a model still in its serialized
    /// pose. One the catalog does not admit stays as it is; so does one that
    /// fails to attach, with the reason.
    pub fn attach_fighter(
        self,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
    ) -> (Self, Option<MeleeError>) {
        let Self::Static(source) = self else {
            return (self, None);
        };
        match MeleeFighterPlayback::attach(*source, catalog, store) {
            Ok(playback) => (Self::Fighter(Box::new(playback)), None),
            Err(not_attached) => (
                Self::Static(Box::new(not_attached.source)),
                not_attached.error,
            ),
        }
    }

    pub fn scene(&self) -> &HsdScene {
        match self {
            Self::Fighter(playback) => playback.scene(),
            Self::Stage(playback) => playback.scene(),
            Self::Static(source) => &source.scene,
        }
    }

    /// What a viewer should frame, when that is less than the whole model.
    pub fn focus(&self) -> Option<HsdFocus> {
        match self {
            Self::Fighter(_) => None,
            Self::Stage(playback) => playback.focus(),
            Self::Static(source) => source.focus,
        }
    }

    /// The fighter's playback, for what only a fighter has: its moves.
    pub fn fighter(&self) -> Option<&MeleeFighterPlayback> {
        match self {
            Self::Fighter(playback) => Some(playback),
            Self::Stage(_) | Self::Static(_) => None,
        }
    }

    pub fn fighter_mut(&mut self) -> Option<&mut MeleeFighterPlayback> {
        match self {
            Self::Fighter(playback) => Some(playback),
            Self::Stage(_) | Self::Static(_) => None,
        }
    }

    /// Whether advancing changes the pose.
    pub fn is_animated(&self) -> bool {
        match self {
            Self::Fighter(_) => true,
            Self::Stage(playback) => playback.is_animated(),
            Self::Static(_) => false,
        }
    }

    pub fn frame(&self) -> f32 {
        match self {
            Self::Fighter(playback) => playback.frame(),
            Self::Stage(playback) => playback.frame(),
            Self::Static(_) => 0.0,
        }
    }

    pub fn end_frame(&self) -> f32 {
        match self {
            Self::Fighter(playback) => playback.end_frame(),
            Self::Stage(playback) => playback.end_frame(),
            Self::Static(_) => 0.0,
        }
    }

    /// Frames advanced per tick: 1 plays at the game's speed.
    pub fn rate(&self) -> f32 {
        match self {
            Self::Fighter(playback) => playback.rate(),
            Self::Stage(playback) => playback.rate(),
            Self::Static(_) => 1.0,
        }
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        match self {
            Self::Fighter(playback) => playback.set_rate(rate),
            Self::Stage(playback) => playback.set_rate(rate),
            Self::Static(_) => Ok(()),
        }
    }

    /// Show `frame` of what is playing.
    pub fn seek(&mut self, frame: f32) -> Result<()> {
        match self {
            Self::Fighter(playback) => playback.seek(frame),
            Self::Stage(playback) => playback.seek(frame),
            Self::Static(_) => Ok(()),
        }
    }

    /// Advance one 60 Hz tick.
    pub fn advance(&mut self) -> Result<()> {
        match self {
            Self::Fighter(playback) => playback.advance(),
            Self::Stage(playback) => playback.advance(),
            Self::Static(_) => Ok(()),
        }
    }

    /// Evaluate the current pose; returns the scene with its draw work.
    pub fn evaluate(&mut self) -> Result<(&HsdScene, &HsdEvaluatedDrawWork)> {
        match self {
            Self::Fighter(playback) => playback.evaluate(),
            Self::Stage(playback) => playback.evaluate(),
            Self::Static(source) => Ok(source.evaluate_bind_pose()?),
        }
    }
}
