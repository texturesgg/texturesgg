//! Skins in tgg-melee: a skin from the library installed into the port as a
//! mod of its one game file (`tgg_mod::skins`), beside installing it into
//! the Slippi ISO. Installing another skin for the same file replaces it;
//! Undo puts back what was there, Remove plays the game's own file again.

use crate::state::{App, NoticeAction};
use melee_dat::MeleeSlot;
use tgg_app::ids::SkinId;
use tgg_mod::Skin;

/// What tgg-melee had for a slot before the last change there: the name and
/// file of the skin it played, or nothing (the game's own file).
pub struct Before {
    pub slot: MeleeSlot,
    pub skin: Option<(String, Vec<u8>)>,
}

/// The skin tgg-melee plays in `slot`, on or off.
pub fn skin_in(app: &App, slot: MeleeSlot) -> Option<Skin> {
    tgg_mod::skin_for(&app.mods.local, &slot.file_name())
}

/// Whether tgg-melee plays the library's skin `id` in `slot`.
pub fn plays(app: &App, id: SkinId, slot: MeleeSlot) -> bool {
    skin_in(app, slot).is_some_and(|skin| skin.enabled && skin.sha256 == id.to_string())
}

/// Install the library's skin `id` into tgg-melee's `slot`.
pub fn install(app: &mut App, id: SkinId, slot: MeleeSlot) {
    let Some(skin) = app.library.skins().iter().find(|skin| skin.id == id) else {
        return;
    };
    let name = skin.name.clone();
    let before = before(app, slot);
    let installed = app
        .library
        .read(id)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            tgg_mod::install_skin(&app.data.mods, &slot.file_name(), &name, bytes)
                .map_err(|error| error.to_string())
        });
    match installed {
        Ok(skin) => {
            let mut text = format!("Installed {name} into {} in tgg-melee.", label(slot));
            if skin.netplay == tgg_mod::Netplay::Files {
                text.push_str(" Players you play online need it too.");
            } else if !skin.enabled {
                text.push_str(" It's turned off on Mods.");
            }
            changed(app, before, text);
        }
        Err(error) => app.tell(
            format!("Couldn't install {name} into tgg-melee: {error}."),
            true,
        ),
    }
}

/// Take the skin out of tgg-melee's `slot`, so it plays the game's own file.
pub fn remove(app: &mut App, slot: MeleeSlot) {
    let before = before(app, slot);
    match tgg_mod::uninstall_skin(&app.data.mods, &slot.file_name()) {
        Ok(()) => {
            let text = format!("tgg-melee plays the original {} again.", label(slot));
            changed(app, before, text);
        }
        Err(error) => app.tell(
            format!("Couldn't remove the skin from tgg-melee: {error}."),
            true,
        ),
    }
}

/// Put back what tgg-melee had before the last change, the toast's Undo.
pub fn undo(app: &mut App) {
    let Some(Before { slot, skin }) = app.port_before.take() else {
        return;
    };
    let result = match skin {
        Some((name, bytes)) => {
            tgg_mod::install_skin(&app.data.mods, &slot.file_name(), &name, bytes).map(|_| ())
        }
        None => tgg_mod::uninstall_skin(&app.data.mods, &slot.file_name()),
    };
    match result {
        Ok(()) => {
            app.tell(
                format!("Put back what {} had in tgg-melee.", label(slot)),
                false,
            );
            crate::mods::actions::read_disk(app);
        }
        Err(error) => app.tell(format!("Couldn't put it back: {error}."), true),
    }
}

/// What tgg-melee has in `slot` now, kept so the change can be undone.
fn before(app: &App, slot: MeleeSlot) -> Before {
    let skin = skin_in(app, slot).and_then(|skin| {
        let folder = tgg_mod::ModsDir::new(app.data.mods.clone()).folder(&skin.id, skin.enabled);
        let bytes = std::fs::read(folder.join("files").join(&skin.path)).ok()?;
        Some((skin.name, bytes))
    });
    Before { slot, skin }
}

/// A change went through: say so, offering Undo, and read the mods again.
fn changed(app: &mut App, before: Before, text: String) {
    app.port_before = Some(before);
    crate::mods::actions::read_disk(app);
    app.tell_with(text, NoticeAction::UndoPort);
}

fn label(slot: MeleeSlot) -> String {
    tgg_app::costumes::slot_label(Some(slot))
}
