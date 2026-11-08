//! Adding skins (look first, then keep), taking them out, and changing what
//! a slot of the player's game holds: installing a skin, undoing the last
//! install, or putting the game's own file back. Melee's slot actions use
//! the same ones.

use super::{Filter, Review, ReviewItem};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use melee_dat::{MeleeReferenceCatalog, MeleeSlot};
use rune::{FileDialogOptions, Tx};
use std::path::{Path, PathBuf};
use tgg_app::costumes::slot_label;
use tgg_app::ids::SkinId;
use tgg_app::install;
use tgg_app::library::SkinSource;

/// The files a skin comes in: a DAT (or a stage's USD), or a zip of them.
const SKIN_FILES: [&str; 3] = ["dat", "usd", "zip"];

pub fn show_filter(app: &mut App, filter: Filter) {
    app.library_page.filter = filter;
}

/// Ask for pictures of the library's skins that don't have one yet. A skin
/// for a file costumes share draws as the first model it changes.
pub fn request_renders(app: &mut App) {
    let Some(references) = &app.game else { return };
    let iso = references.game().path().to_owned();
    for skin in app.library.skins() {
        if let Some(slot) = skin.slot {
            app.renders
                .request_skin(&iso, skin.id, app.library.blob_path(skin.id), slot);
        }
    }
}

/// Ask for a skin's file with the system's file dialog, then look at it.
pub fn choose_file(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Add a skin".into(),
        ..Default::default()
    }
    .filter("Skin (DAT, USD, or a zip of them)", &SKIN_FILES);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => add(app, &[path]),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Ask for a folder, then look at every skin file in it.
pub fn choose_folder(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Add a folder of skins".into(),
        ..Default::default()
    };
    tx.open_folder(options, |app: &mut App, answer, _| match answer {
        Ok(Some(folder)) => add(app, &skin_files(&folder)),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// The skin files directly in `folder`, by name.
fn skin_files(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    SKIN_FILES
                        .iter()
                        .any(|kind| kind.eq_ignore_ascii_case(extension))
                })
        })
        .collect();
    files.sort();
    files
}

/// Look at `paths` (DATs, or zips of them) and open Review, where the
/// player says where each goes before any joins the library.
pub fn add(app: &mut App, paths: &[PathBuf]) {
    let (items, rejected): (Vec<_>, Vec<_>) = app
        .library
        .inspect_files(paths, MeleeReferenceCatalog::checked_in())
        .into_iter()
        .partition(Result::is_ok);
    let items: Vec<ReviewItem> = items
        .into_iter()
        .flatten()
        .map(|candidate| ReviewItem {
            slot: candidate.slot,
            candidate,
        })
        .collect();
    let rejected = rejected
        .into_iter()
        .filter_map(Result::err)
        .collect::<Vec<_>>();
    if items.is_empty() && rejected.is_empty() {
        app.tell("There were no skins there.", false);
        return;
    }
    app.library_page.review = Some(Review { items, rejected });
}

/// Put the reviewed file `index` in `slot` instead.
pub fn set_review_slot(app: &mut App, index: usize, slot: MeleeSlot) {
    if let Some(item) = app
        .library_page
        .review
        .as_mut()
        .and_then(|review| review.items.get_mut(index))
    {
        item.slot = Some(slot);
    }
}

/// Close Review without adding anything.
pub fn cancel_review(app: &mut App) {
    app.library_page.review = None;
}

/// Keep the reviewed files in the library for the slots the player chose
/// and, with `install_them`, install each into its slot; then say what
/// happened.
pub fn confirm_review(app: &mut App, install_them: bool) {
    let Some(review) = app.library_page.review.take() else {
        return;
    };
    let history = app.data.history();
    let (mut done, mut failed, mut changed) = (Vec::new(), Vec::new(), Vec::new());
    for item in review.items {
        let skin = match app
            .library
            .store(&item.candidate, item.slot, SkinSource::Imported)
        {
            Ok(skin) => skin,
            Err(error) => {
                failed.push(format!("{}: {error}", item.candidate.name));
                continue;
            }
        };
        let place = slot_label(skin.slot);
        match (install_them, &app.game, skin.slot) {
            (true, Some(references), Some(slot)) => {
                match install::install(
                    references.game(),
                    slot,
                    &item.candidate.bytes,
                    &app.library,
                    &history,
                ) {
                    Ok(()) => {
                        changed.push(slot);
                        done.push(format!("installed {} into {place}", skin.name));
                    }
                    Err(error) => failed.push(format!("{}: {error}", skin.name)),
                }
            }
            (true, _, None) => failed.push(format!(
                "{} was added but has no slot to install into",
                skin.name
            )),
            _ => done.push(format!("added {} ({place})", skin.name)),
        }
    }
    let text = done
        .iter()
        .chain(&failed)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    if !text.is_empty() {
        app.tell(capitalized(&text), done.is_empty() && !failed.is_empty());
    }
    slots_changed(app, &changed);
    request_renders(app);
}

/// Take the skin `id` out of the library. Its file stays, for the install
/// history.
pub fn remove(app: &mut App, id: SkinId) {
    match app.library.remove(id) {
        Ok(skin) => app.tell(format!("Removed {} from your library.", skin.name), false),
        Err(error) => app.tell(error.to_string(), true),
    }
}

/// Install the library's skin `id` into `slot` of the player's game.
pub fn install(app: &mut App, id: SkinId, slot: MeleeSlot) {
    let Some(references) = &app.game else { return };
    let name = app
        .library
        .skins()
        .iter()
        .find(|skin| skin.id == id)
        .map_or("the skin".to_owned(), |skin| skin.name.clone());
    let result = app
        .library
        .read(id)
        .and_then(|bytes| {
            install::install(
                references.game(),
                slot,
                &bytes,
                &app.library,
                &app.data.history(),
            )
        })
        .map(|()| format!("Installed {name} into {}.", slot_label(Some(slot))));
    finish(app, slot, result);
}

/// Put back what `slot` held before its last install.
pub fn undo(app: &mut App, slot: MeleeSlot) {
    let Some(references) = &app.game else { return };
    let result = install::undo(references.game(), slot, &app.library, &app.data.history())
        .map(|()| format!("Put back what {} had before.", slot_label(Some(slot))));
    finish(app, slot, result);
}

/// Put the game's own file back into `slot`, from the install history or
/// another vanilla ISO in the player's game folders.
pub fn restore(app: &mut App, slot: MeleeSlot) {
    let Some(references) = &app.game else { return };
    let others: Vec<PathBuf> = tgg_app::discover_games()
        .into_iter()
        .map(|choice| choice.path)
        .collect();
    let result = install::restore_vanilla(
        references.game(),
        slot,
        &others,
        &app.library,
        &app.data.history(),
    )
    .map(|()| format!("{} is vanilla again.", slot_label(Some(slot))));
    finish(app, slot, result);
}

/// Say how a change to `slot` went and, when it went through, show it.
fn finish(app: &mut App, slot: MeleeSlot, result: Result<String, tgg_app::Error>) {
    match result {
        Ok(text) => {
            app.tell(text, false);
            slots_changed(app, &[slot]);
        }
        Err(error) => app.tell(error.to_string(), true),
    }
}

/// What `slot` holds changed under the editor, which shows it already:
/// draw it again and read what every slot holds again, leaving the stage.
pub fn refresh_after_install(app: &mut App, slot: MeleeSlot) {
    app.renders.forget(RenderKey::Slot(slot));
    crate::melee::actions::refresh(app);
}

/// What `slots` hold changed: draw them again, read what every slot holds
/// again, and show the new file on the stage if it's showing one of them.
fn slots_changed(app: &mut App, slots: &[MeleeSlot]) {
    if slots.is_empty() {
        return;
    }
    for &slot in slots {
        app.renders.forget(RenderKey::Slot(slot));
    }
    crate::melee::actions::refresh(app);
    let showing = app
        .stage
        .shown
        .as_ref()
        .map(|shown| shown.slot)
        .filter(|slot| slots.contains(slot));
    if let Some(slot) = showing
        && let Err(problem) = crate::stage::actions::show(app, slot)
    {
        app.tell(problem, true);
    }
}

/// `text` with its first letter capitalized.
fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}
