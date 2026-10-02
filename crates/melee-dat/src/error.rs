//! What can go wrong loading and playing Melee models.

use dat_parser::hsd::source::HsdSourceError;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MeleeError {
    #[error("reference catalog: {0}")]
    Catalog(String),
    #[error("playback: {0}")]
    Playback(String),
    /// The animation can't play; the reason reads as plain words.
    #[error("{0}")]
    Unplayable(String),
    #[error("texture places: {0}")]
    Places(String),
    #[error(transparent)]
    Source(#[from] HsdSourceError),
}

pub type Result<T> = std::result::Result<T, MeleeError>;

pub(crate) fn playback_error(message: impl Into<String>) -> MeleeError {
    MeleeError::Playback(message.into())
}
