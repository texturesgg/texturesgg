//! Skins in tgg-melee: a skin from the library put in a costume slot of
//! `skins.json`, which the tgg.skins mod reads when the game starts
//! (`tgg_mod::skins`). Putting another skin in the slot replaces it; Undo
//! puts back what was there, Remove plays the game's own costume again.
//!
//! The game decides whether a costume changes only looks
//! ([`tgg_mod::costume_verdict`]): one that does plays online too, one that
//! changes more shows offline only.

use crate::jobs::Done;
use crate::state::{App, NoticeAction};
use melee_dat::MeleeSlot;
use tgg_app::ids::SkinId;
use tgg_mod::{SkinSlot, Verdict};

/// What tgg-melee had in a slot before the last change there: its skin,
/// or nothing (the game's own costume).
pub struct Before {
    pub slot: MeleeSlot,
    pub skin: Option<SkinSlot>,
}

/// Whether skins for `slot` can go into tgg-melee: costumes, which is what
/// tgg.skins loads.
pub fn takes(slot: MeleeSlot) -> bool {
    matches!(slot, MeleeSlot::Costume { .. })
}

/// The skin tgg-melee plays in `slot`.
pub fn skin_in(app: &App, slot: MeleeSlot) -> Option<&SkinSlot> {
    let name = slot.file_name();
    app.mods
        .skins
        .slots
        .iter()
        .find(|(have, _)| have.eq_ignore_ascii_case(&name))
        .map(|(_, skin)| skin)
}

/// Whether tgg-melee plays the library's skin `id` in `slot`.
pub fn plays(app: &App, id: SkinId, slot: MeleeSlot) -> bool {
    skin_in(app, slot).is_some_and(|skin| skin.sha256 == id.to_string())
}

/// What the game said of the skin `id` in `slot`, once it's been asked.
pub fn verdict(app: &App, id: SkinId, slot: MeleeSlot) -> Option<&Verdict> {
    app.mods.verdicts.get(&(id, slot)).and_then(Option::as_ref)
}

/// Put the library's skin `id` into tgg-melee's `slot`, installing tgg.skins
/// first if it isn't yet.
pub fn install(app: &mut App, id: SkinId, slot: MeleeSlot) {
    let Some(skin) = app.library.skins().iter().find(|skin| skin.id == id) else {
        return;
    };
    let name = skin.name.clone();
    let before = before(app, slot);
    let path = app.library.blob_path(id);
    match tgg_mod::set_skin(&app.data.skins, &slot.file_name(), &path, &name) {
        Ok(_) => {
            let mut text = format!("Installed {name} in tgg-melee.");
            if let Some(why) = loader_missing(app) {
                text = why;
            }
            changed(app, before, text);
            ask(app, vec![(id, slot)]);
        }
        Err(error) => app.tell(
            format!("Couldn't install {name} in tgg-melee: {error}."),
            true,
        ),
    }
}

/// Take the skin out of tgg-melee's `slot`, so it plays the game's own
/// costume.
pub fn remove(app: &mut App, slot: MeleeSlot) {
    let before = before(app, slot);
    match tgg_mod::clear_skin(&app.data.skins, &slot.file_name()) {
        Ok(_) => changed(
            app,
            before,
            "tgg-melee plays the original again.".to_owned(),
        ),
        Err(error) => app.tell(format!("Couldn't remove it from tgg-melee: {error}."), true),
    }
}

/// Put back what tgg-melee had before the last change, the toast's Undo.
pub fn undo(app: &mut App) {
    let Some(Before { slot, skin }) = app.port_before.take() else {
        return;
    };
    let result = match skin {
        Some(skin) => tgg_mod::set_skin(&app.data.skins, &slot.file_name(), &skin.path, &skin.name)
            .map(|_| ()),
        None => tgg_mod::clear_skin(&app.data.skins, &slot.file_name()).map(|_| ()),
    };
    match result {
        Ok(()) => {
            app.tell("Put back what tgg-melee had.", false);
            crate::mods::actions::read_disk(app);
        }
        Err(error) => app.tell(format!("Couldn't put it back: {error}."), true),
    }
}

/// Ask the game, off the window's thread, what it makes of each skin in its
/// slot that it hasn't been asked about. Needs tgg-melee and a disc; a
/// tgg-melee without the check leaves them unsaid.
pub fn ask(app: &mut App, skins: Vec<(SkinId, MeleeSlot)>) {
    let Some(port) = app.mods.port().cloned() else {
        return;
    };
    let iso = match &app.mods.disc {
        crate::mods::Disc::Found(disc) => disc.clone(),
        _ => match &app.game {
            Some(references) => references.game().path().to_owned(),
            None => return,
        },
    };
    let asking: Vec<(SkinId, MeleeSlot, std::path::PathBuf)> = skins
        .into_iter()
        .filter(|&(id, slot)| takes(slot) && !app.mods.verdicts.contains_key(&(id, slot)))
        .map(|(id, slot)| (id, slot, app.library.blob_path(id)))
        .collect();
    if asking.is_empty() {
        return;
    }
    for &(id, slot, _) in &asking {
        app.mods.verdicts.insert((id, slot), None);
    }
    app.jobs.spawn("costume check", move || {
        let verdicts = asking
            .into_iter()
            .map(|(id, slot, path)| {
                let verdict = tgg_mod::costume_verdict(&port, &iso, &slot.file_name(), &path).ok();
                (id, slot, verdict)
            })
            .collect();
        Done::Verdicts(verdicts)
    });
}

/// The game's answers arrived.
pub fn answered(app: &mut App, verdicts: Vec<(SkinId, MeleeSlot, Option<Verdict>)>) -> bool {
    for (id, slot, verdict) in verdicts {
        app.mods.verdicts.insert((id, slot), verdict);
    }
    true
}

/// Ask about every skin in the library that could go into tgg-melee.
pub fn ask_all(app: &mut App) {
    let skins = app
        .library
        .skins()
        .iter()
        .filter_map(|skin| {
            skin.slot
                .filter(|&slot| takes(slot))
                .map(|slot| (skin.id, slot))
        })
        .collect();
    ask(app, skins);
}

/// When tgg.skins isn't installed: install it now if textures.gg has it, and
/// say what's happening; else say skins wait for it.
fn loader_missing(app: &mut App) -> Option<String> {
    let loader: tgg_mod::ModId = tgg_mod::skins::LOADER.parse().ok()?;
    if app.mods.local.get(&loader).is_some() {
        return None;
    }
    if app.mods.port().is_some() && app.mods.newest(&loader).is_some() && app.mods.work.is_none() {
        crate::mods::actions::review_mod(app, &loader);
        crate::mods::actions::confirm_review(app);
        return Some("Installed in tgg-melee, with tgg.skins.".to_owned());
    }
    Some("Installed in tgg-melee; it needs tgg.skins.".to_owned())
}

/// What tgg-melee has in `slot` now, kept so the change can be undone.
fn before(app: &App, slot: MeleeSlot) -> Before {
    Before {
        slot,
        skin: skin_in(app, slot).cloned(),
    }
}

/// A change went through: say so, offering Undo, and read skins.json again.
fn changed(app: &mut App, before: Before, text: String) {
    app.port_before = Some(before);
    crate::mods::actions::read_disk(app);
    app.tell_with(text, NoticeAction::UndoPort);
}
