//! Unsaved edits kept on disk as they're made, so closing the window (which
//! can't stop to ask) or a crash doesn't lose them. The costume open is
//! written to a draft after every edit, named for its slot, and the draft
//! goes once the edits are saved, discarded, or undone. The next launch
//! offers to reopen it: the costume opens with those edits, unsaved.

use super::actions;
use crate::state::{App, NoticeAction};
use melee_dat::MeleeSlot;
use std::path::{Path, PathBuf};

/// The folder the draft is kept in, beside the library.
fn folder(app: &App) -> PathBuf {
    app.data.library.with_file_name("draft")
}

/// Keep the open costume's edits in the draft, or remove the draft when
/// there are none. Called after every change to the document.
pub fn keep(app: &mut App) {
    // A snapshot's scenes edit too, but they aren't the player's.
    if crate::snapshot() {
        return;
    }
    let unsaved = app.editor.as_ref().and_then(|document| {
        let open = document.listing.document.as_ref().ok()?;
        open.is_modified()
            .then(|| (document.slot, open.bytes().to_vec()))
    });
    let folder = folder(app);
    let written = match unsaved {
        Some((slot, bytes)) => write(&folder, slot, &bytes),
        None => clear(&folder),
    };
    if let Err(error) = written {
        tgg_app::log::log(&format!("couldn't keep the draft: {error}"));
    }
}

/// Write `bytes` as the draft of `slot`, in place of any other.
fn write(folder: &Path, slot: MeleeSlot, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let name = slot.file_name();
    // Written aside and moved in, so a crash mid-write leaves the last one.
    let partial = folder.join(".partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, folder.join(&name))?;
    for entry in std::fs::read_dir(folder)?.flatten() {
        if entry.file_name() != name.as_str() {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn clear(folder: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The draft kept, and its slot.
fn found(app: &App) -> Option<(MeleeSlot, Vec<u8>)> {
    std::fs::read_dir(folder(app))
        .ok()?
        .flatten()
        .find_map(|entry| {
            let slot = MeleeSlot::from_file_name(entry.file_name().to_str()?)?;
            Some((slot, std::fs::read(entry.path()).ok()?))
        })
}

/// Offer to reopen the draft an earlier session left, when there is one and
/// nothing is open.
pub fn offer(app: &mut App) {
    if app.editor.is_some() || app.game.is_none() {
        return;
    }
    if let Some((slot, _)) = found(app) {
        app.tell_with(
            format!(
                "Unsaved edits to {}",
                tgg_app::costumes::slot_label(Some(slot))
            ),
            NoticeAction::Reopen(slot),
        );
    }
}

/// Open the draft's costume with its edits, still unsaved.
pub fn reopen(app: &mut App) {
    let Some((slot, bytes)) = found(app) else {
        return app.tell("That draft is gone.", true);
    };
    app.notice = None;
    actions::open_slot(app, slot);
    if let Err(error) = actions::restore(app, slot, bytes) {
        app.tell(format!("Couldn't reopen your edits: {error}"), true);
    }
}

/// Throw the draft away.
pub fn discard(app: &mut App) {
    app.notice = None;
    if let Err(error) = clear(&folder(app)) {
        app.tell(format!("Couldn't remove the draft: {error}"), true);
    }
}
