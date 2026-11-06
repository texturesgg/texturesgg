//! A fighter's costume read from the player's disc image and attached to its
//! animations, as hsd-render's CLI and tgg-editor do it.

use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use dat_parser::hsd::source::HsdSource;
use melee_dat::{FighterAttach, MeleeFighterPlayback, MeleeReferenceCatalog, MeleeReferenceStore};
use std::path::Path;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// The costume `file` (such as `PlFxNr.dat`) from the disc at `iso`, playing
/// its idle animation.
pub fn load(iso: &Path, file: &str) -> Result<MeleeFighterPlayback, Error> {
    let mut disc = gc_iso::Disc::open(iso).map_err(|error| format!("{}: {error}", iso.display()))?;
    let bytes = disc.read(file).map_err(|error| format!("{file}: {error}"))?;
    let source = HsdSource::from_dat(&bytes, HsdDrawEvaluationPolicy::MELEE_FIGHTER)?;
    let catalog = MeleeReferenceCatalog::checked_in();
    let store = MeleeReferenceStore::for_costume(catalog, &source.scene, |asset| disc.read(&asset.file_name).ok())
        .ok_or("the catalog doesn't know this costume's fighter files")?;
    match MeleeFighterPlayback::attach(source, catalog, &store) {
        FighterAttach::Attached(playback) => Ok(*playback),
        FighterAttach::Unrecognized(_) => Err("no idle profile admits this costume".into()),
        FighterAttach::Failed { error, .. } => Err(error.into()),
    }
}
