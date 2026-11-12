//! Opening the costume showing as a document, choosing a texture, applying
//! and undoing edits, saving, and leaving (asking first when edits are
//! unsaved).

use super::{Document, Pending};
use crate::nav::{Place, actions as nav};
use crate::stage::actions as stage;
use crate::state::App;
use dat_edit::{TextureEdit, TextureIndex, Undone, UseIndex};
use dat_parser::hsd::scene::HsdTextureSourceId;
use melee_dat::MeleeReferenceCatalog;
use rune::{Pixels, Tx};
use tgg_app::costumes::slot_label;
use tgg_app::install::{self, SlotState};
use tgg_app::library::SkinSource;
use tgg_app::textures::{Listing, texture_size};

/// Open what the stage shows as a document, from the player's game.
pub fn open(app: &mut App) {
    let Some(references) = &app.game else { return };
    let Some(shown) = &app.stage.shown else {
        return;
    };
    let slot = shown.slot;
    let bytes = match references.game().read_slot(slot) {
        Ok(bytes) => bytes,
        Err(error) => return app.tell(format!("Couldn't open it: {error}"), true),
    };
    let listing = Listing::read(bytes, Some(references), shown.store.as_deref());
    if let Err(error) = &listing.document {
        return app.tell(format!("Its textures couldn't be read: {error}"), true);
    }
    let thumbnails = pictures(&listing);
    let mut document = Document {
        slot,
        listing,
        thumbnails,
        selected: None,
        pending: None,
        external: Default::default(),
        colors: Vec::new(),
        color_pick: None,
    };
    super::colors::read(&mut document);
    app.editor = Some(document);
    // The sidebar lists it among the recent edits.
    app.change_settings(move |settings| settings.remember_edit(slot));
    select(
        app,
        (app.editor
            .as_ref()
            .is_some_and(|document| document.texture_count() > 0))
        .then_some(0),
    );
}

/// A picture of each texture, through its first use.
fn pictures(listing: &Listing) -> Vec<Pixels> {
    let Ok(document) = &listing.document else {
        return Vec::new();
    };
    (0..document.textures().len())
        .map(|index| {
            let pixels = Pixels::new();
            let (width, height) = texture_size(&document.textures()[index]);
            if let Ok(rgba) = document.pixels(TextureIndex(index), UseIndex(0)) {
                pixels.set(width, height, rgba);
            }
            pixels
        })
        .collect()
}

/// Inspect `texture` (or nothing): the one way the selection changes. Its
/// surfaces tint on the stage.
pub fn select(app: &mut App, texture: Option<usize>) {
    let Some(document) = &mut app.editor else {
        return;
    };
    document.selected = texture;
    let uses: Vec<HsdTextureSourceId> = match (&document.listing.document, texture) {
        (Ok(document), Some(texture)) => document
            .textures()
            .get(texture)
            .map(|texture| texture.drawn_as())
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    stage::set_highlight(app, &uses);
}

/// Select the texture a click on the model landed on (`textures`, each the
/// scene's ids for one texture the surface draws, most defining first). A
/// surface can blend several: clicking one whose texture is selected moves
/// to the next, and past the last unselects. The list scrolls to the one
/// chosen.
pub fn select_picked(app: &mut App, textures: &[Vec<HsdTextureSourceId>], tx: &mut Tx) {
    let Some(document) = &app.editor else { return };
    let Ok(open) = &document.listing.document else {
        return;
    };
    let mut candidates: Vec<usize> = Vec::new();
    for ids in textures {
        let found = open
            .textures()
            .iter()
            .position(|texture| ids.iter().any(|id| texture.uses.contains(id)));
        if let Some(index) = found
            && !candidates.contains(&index)
        {
            candidates.push(index);
        }
    }
    if candidates.is_empty() {
        if !textures.is_empty() {
            app.tell("That surface's texture isn't in the list.", false);
        }
        return;
    }
    let at = document
        .selected
        .and_then(|selected| candidates.iter().position(|&index| index == selected));
    let place = match at {
        Some(at) if at + 1 == candidates.len() => {
            select(app, None);
            app.tell("Selection cleared", false);
            return;
        }
        Some(at) => at + 1,
        None => 0,
    };
    let next = candidates[place];
    let name = document.listing.name(next);
    let text = if candidates.len() > 1 {
        format!(
            "{name}: {} of {} on this surface; click again for the next",
            place + 1,
            candidates.len()
        )
    } else {
        name
    };
    select(app, Some(next));
    tx.scroll_into_view(format!("editor/texture/{next}"));
    app.tell(text, false);
}

/// Replace `texture`'s pixels with the RGBA8 image `rgba`, its size: one undo
/// step, shown on the model and in the panes. A CI texture that owns its
/// palette gets one rebuilt for the new pixels; otherwise they map to the
/// colors it has.
pub fn import(app: &mut App, texture: usize, rgba: &[u8]) -> Result<TextureEdit, String> {
    let Some(document) = &mut app.editor else {
        return Err("no costume is open".into());
    };
    let Ok(open) = &mut document.listing.document else {
        return Err("its textures couldn't be read".into());
    };
    let edit = open
        .import(TextureIndex(texture), rgba)
        .map_err(|error| error.to_string())?;
    show_pixels(app, texture, &edit.decoded);
    super::draft::keep(app);
    Ok(edit)
}

/// Show `texture`'s pixels after an edit: every decoded use on the model,
/// and its picture.
fn show_pixels(app: &mut App, texture: usize, decoded: &[(HsdTextureSourceId, Vec<u8>)]) {
    let Some(document) = &app.editor else { return };
    let Ok(open) = &document.listing.document else {
        return;
    };
    let size = texture_size(&open.textures()[texture]);
    if let (Some((_, pixels)), Some(picture)) = (decoded.first(), document.thumbnails.get(texture))
    {
        picture.set(size.0, size.1, pixels.clone());
    }
    stage::update_textures(
        app,
        decoded
            .iter()
            .map(|(id, pixels)| (*id, size, pixels.as_slice())),
    );
}

/// Step back through the edits.
pub fn undo(app: &mut App) {
    step(app, true);
}

/// Step forward again.
pub fn redo(app: &mut App) {
    step(app, false);
}

fn step(app: &mut App, back: bool) {
    let Some(document) = &mut app.editor else {
        return;
    };
    let Ok(open) = &mut document.listing.document else {
        return;
    };
    let stepped = if back { open.undo() } else { open.redo() };
    match stepped {
        None => {}
        Some(Err(error)) => app.tell(
            format!("Couldn't {}: {error}", if back { "undo" } else { "redo" }),
            true,
        ),
        Some(Ok(Undone::Texture(restored))) => {
            show_pixels(app, restored.texture.0, &restored.decoded);
            select(app, Some(restored.texture.0));
            super::external::resync(app, restored.texture.0);
        }
        // Colors change the model itself: show it afresh from the document.
        Some(Ok(Undone::Colors)) => {
            let bytes = open.bytes().to_vec();
            if let Some(document) = &mut app.editor {
                super::colors::read(document);
            }
            if let Err(problem) = stage::replace(app, &bytes) {
                app.tell(problem, true);
            }
        }
    }
    super::draft::keep(app);
}

/// Keep the costume in the library as an edited skin and, with `install_it`,
/// write it into the slot it came from; then do what was waiting on it.
pub fn save(app: &mut App, install_it: bool, then: Option<Pending>, _: &mut Tx) {
    let Some(document) = &app.editor else { return };
    let Some(references) = &app.game else { return };
    let Ok(open) = &document.listing.document else {
        return;
    };
    let (slot, bytes) = (document.slot, open.bytes().to_vec());
    let game = references.game();
    let saved = (|| {
        let before = game.read_slot(slot)?;
        // An edit of a skin is that skin, edited; of the game's own costume,
        // the slot, edited.
        let (name, from) = match SlotState::of(slot, &before, &app.library) {
            SlotState::Skin(skin) => (edited_name(&skin.name), Some(skin.id)),
            _ => (edited_name(&slot_label(Some(slot))), None),
        };
        let mut candidate = app
            .library
            .inspect(
                &slot.file_name(),
                bytes.clone(),
                MeleeReferenceCatalog::checked_in(),
            )
            .map_err(|rejected| tgg_app::Error::Rejected(rejected.reason))?;
        candidate.name = name.clone();
        app.library
            .store(&candidate, Some(slot), SkinSource::Edited { from })?;
        if install_it {
            install::install(game, slot, &bytes, &app.library, &app.data.history())?;
        }
        Ok::<_, tgg_app::Error>(name)
    })();
    match saved {
        Ok(name) => {
            if let Some(Ok(open)) = app
                .editor
                .as_mut()
                .map(|document| &mut document.listing.document)
            {
                open.mark_saved();
            }
            super::draft::keep(app);
            let text = if install_it {
                format!("Saved {name} to your library and installed it.")
            } else {
                format!("Saved {name} to your library.")
            };
            app.tell(text, false);
            if install_it {
                crate::library::actions::refresh_after_install(app, slot);
            }
            if let Some(then) = then {
                go_on(app, then);
            }
        }
        Err(error) => app.tell(format!("Couldn't save: {error}"), true),
    }
}

/// Ask to leave for `then`: straight away when nothing is unsaved, else
/// the prompt asks first. Whether it left.
pub fn leave_for(app: &mut App, then: Pending, _: &mut Tx) -> bool {
    let Some(document) = &mut app.editor else {
        return true;
    };
    if document.modified() {
        document.pending = Some(then);
        return false;
    }
    go_on(app, then);
    true
}

/// Back to the fighter, asking first when edits are unsaved.
pub fn leave(app: &mut App, tx: &mut Tx) {
    leave_for(app, Pending::Leave, tx);
}

/// The prompt's answers.
pub fn answer_save(app: &mut App, tx: &mut Tx) {
    let then = app
        .editor
        .as_mut()
        .and_then(|document| document.pending.take());
    save(app, false, then, tx);
}

pub fn answer_discard(app: &mut App, _: &mut Tx) {
    if let Some(then) = app
        .editor
        .as_mut()
        .and_then(|document| document.pending.take())
    {
        go_on(app, then);
    }
}

pub fn answer_cancel(app: &mut App) {
    if let Some(document) = &mut app.editor {
        document.pending = None;
    }
}

/// Do what waited: close the document (the stage shows the slot from the
/// game again, edits gone), then leave, close, or go.
fn go_on(app: &mut App, then: Pending) {
    close(app);
    match then {
        Pending::Leave => {}
        Pending::Go(place) => nav::go(app, place),
        Pending::Edit(slot) => open_slot(app, slot),
    }
}

/// Edit `slot`, a recent edit: through the prompt when edits are unsaved.
pub fn edit_recent(app: &mut App, slot: melee_dat::MeleeSlot, tx: &mut Tx) {
    if app
        .editor
        .as_ref()
        .is_some_and(|document| document.slot == slot)
    {
        return;
    }
    if app.editor.is_some() {
        leave_for(app, Pending::Edit(slot), tx);
    } else {
        open_slot(app, slot);
    }
}

/// Show `slot` on the stage, under its fighter or stage in Melee, and open
/// it for editing.
pub fn open_slot(app: &mut App, slot: melee_dat::MeleeSlot) {
    nav::go(app, Place::Skins);
    app.melee.tab = match slot {
        melee_dat::MeleeSlot::Stage(_) => crate::melee::Tab::Stages,
        _ => crate::melee::Tab::Fighters,
    };
    match slot {
        melee_dat::MeleeSlot::Stage(stage) => crate::melee::actions::open_stage(app, stage),
        slot => {
            let Some(character) = slot.character() else {
                return;
            };
            crate::melee::actions::open_fighter(app, character);
            crate::melee::actions::choose_costume(app, slot);
        }
    }
    if app
        .stage
        .shown
        .as_ref()
        .is_some_and(|shown| shown.slot == slot)
    {
        open(app);
    }
}

/// Close the document, and show its slot as the game holds it.
fn close(app: &mut App) {
    let Some(mut document) = app.editor.take() else {
        return;
    };
    super::external::forget(&mut document);
    // Closed, its edits are saved or discarded: the draft goes.
    super::draft::keep(app);
    if app.stage.shown.is_some()
        && let Err(problem) = stage::show(app, document.slot)
    {
        app.tell(problem, true);
    }
}

/// Where a place change goes while a document is open: through the prompt.
pub fn go(app: &mut App, place: Place, tx: &mut Tx) {
    if app.editor.is_some() {
        leave_for(app, Pending::Go(place), tx);
    } else {
        nav::go(app, place);
    }
}

/// A skin's name once edited: "Waffle Falco (edited)", not edited twice.
fn edited_name(name: &str) -> String {
    if name.ends_with(" (edited)") {
        name.to_owned()
    } else {
        format!("{name} (edited)")
    }
}

/// Put `bytes`, a draft of `slot`'s costume, in place of the document just
/// opened for it: on the model and in the panes, and unsaved.
pub fn restore(app: &mut App, slot: melee_dat::MeleeSlot, bytes: Vec<u8>) -> Result<(), String> {
    let Some(document) = &app.editor else {
        return Err("its costume couldn't be opened".into());
    };
    if document.slot != slot {
        return Err("its costume couldn't be opened".into());
    }
    let store = app
        .stage
        .shown
        .as_ref()
        .and_then(|shown| shown.store.clone());
    let mut listing = Listing::read(bytes.clone(), app.game.as_ref(), store.as_deref());
    match &mut listing.document {
        Ok(open) => open.mark_unsaved(),
        Err(error) => return Err(error.to_string()),
    }
    let thumbnails = pictures(&listing);
    if let Some(document) = &mut app.editor {
        document.listing = listing;
        document.thumbnails = thumbnails;
        super::colors::read(document);
    }
    stage::replace(app, &bytes)?;
    let first = app
        .editor
        .as_ref()
        .is_some_and(|document| document.texture_count() > 0)
        .then_some(0);
    select(app, first);
    Ok(())
}
