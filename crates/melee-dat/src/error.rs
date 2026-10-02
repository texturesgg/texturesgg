//! What can go wrong loading and playing Melee models.

use crate::fighter::animation::{FighterAnimationAttachError, FighterAnimationBindingError};
use crate::fighter::parts::ModelPartsError;
use dat_parser::DatParseError;
use dat_parser::hsd::animation::HsdJointPoseError;
use dat_parser::hsd::draw::HsdDrawWorkError;
use dat_parser::hsd::source::HsdSourceError;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MeleeError {
    /// The model itself did not load.
    #[error(transparent)]
    Source(#[from] HsdSourceError),
    /// A file the catalog names was not among those supplied.
    #[error("reference {file_name} ({sha256}) is not available")]
    MissingReference { file_name: String, sha256: String },
    /// A supplied reference matched its catalog hash and still did not parse.
    #[error("reference {file_name} does not parse: {source}")]
    InvalidReference {
        file_name: String,
        source: DatParseError,
    },
    #[error("fighter playback requires the MeleeFighter evaluation policy")]
    WrongPolicy,
    /// The animation can't play; `reason` reads as plain words, for a move
    /// list to show.
    #[error("{reason}")]
    Unplayable {
        reason: String,
        source: FighterAnimationBindingError,
    },
    #[error("the fighter's animation table does not read: {0}")]
    AnimationTable(#[source] FighterAnimationBindingError),
    #[error("the fighter's parts do not read: {0}")]
    FighterParts(#[source] FighterAnimationBindingError),
    #[error("the animation does not attach: {0}")]
    Attach(#[from] FighterAnimationAttachError),
    #[error("model-part visibility: {0}")]
    ModelParts(#[from] ModelPartsError),
    /// The costume or the fighter's files disagree with what the catalog
    /// verified for the fighter.
    #[error("{fighter} differs from its verified reference: {what}")]
    ReferenceMismatch { fighter: String, what: &'static str },
    #[error("playback rate {0} must be positive")]
    InvalidRate(f32),
    #[error("the frame to seek to is not a number")]
    InvalidFrame,
    #[error("{label} stopped at its reset frame")]
    StoppedAtReset { label: String },
    #[error("joint pose: {0}")]
    Pose(#[from] HsdJointPoseError),
    #[error("draw work: {0}")]
    DrawWork(#[from] HsdDrawWorkError),
}

pub type Result<T> = std::result::Result<T, MeleeError>;
