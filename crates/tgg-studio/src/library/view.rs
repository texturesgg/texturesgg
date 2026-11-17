//! The player's skins as a plate each: its picture, its name, where it goes,
//! and a yellow mark while tgg-melee plays it. Skins groups them by what
//! they change; You shows the ones the player made.

use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, space};
use crate::ui::{card as ui_card, card_plate, status};
use melee_dat::MeleeSlot;
use rune::{El, row};
use tgg_app::costumes::fighters;
use tgg_app::library::{Skin, SkinSource};

/// A skin's plate: its picture, its name, where it goes, and a gold dot with
/// "Using" while tgg-melee plays it. The caller keys it and makes pressing
/// it open it.
pub fn card(app: &App, skin: &Skin, p: &Palette) -> El {
    let mut picture = card_plate(p);
    if let Some(pixels) = app.renders.get(RenderKey::Skin(skin.id)) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    let using = in_game(app, skin);
    let goes = goes(skin);
    let words = match (using, goes.is_empty()) {
        (true, true) => "Using".to_owned(),
        (true, false) => format!("Using · {goes}"),
        (false, _) => goes,
    };
    ui_card(picture, &name(skin), status(words, using, p), p)
}

/// A skin's name as a surface says it: its own, unless that's only the name
/// of the game file it came from, which reads as its fighter (`PlFc` is
/// "Falco").
pub fn name(skin: &Skin) -> String {
    shown_name(&skin.name, skin.slot)
}

/// [`name`] for a file before it joins the library, under the slot it would
/// go in.
pub fn shown_name(name: &str, slot: Option<MeleeSlot>) -> String {
    let Some(slot) = slot else {
        return name.to_owned();
    };
    let file = slot.file_name();
    let stem = file.trim_end_matches(".dat");
    let only_the_file = [file.as_str(), stem]
        .iter()
        .any(|file| name.eq_ignore_ascii_case(file));
    if !only_the_file {
        return name.to_owned();
    }
    slot_place(Some(slot))
        .split(" · ")
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Where a skin goes, without repeating its name: a skin called "Falco"
/// reads "all costumes".
pub fn goes(skin: &Skin) -> String {
    let place = place(skin);
    place
        .strip_prefix(&name(skin))
        .map_or(place.clone(), |rest| {
            rest.trim_start_matches(" · ").to_owned()
        })
}

/// Where it goes, in words: "Fox · Orange", "Battlefield".
pub fn place(skin: &Skin) -> String {
    slot_place(skin.slot)
}

/// Where a slot goes, in words, never the file name it lives in.
pub fn slot_place(slot: Option<MeleeSlot>) -> String {
    match slot {
        Some(MeleeSlot::Costume { character, color }) => {
            format!("{} · {}", character.name(), color.name())
        }
        Some(MeleeSlot::FighterData(character)) => format!("{} · all costumes", character.name()),
        Some(MeleeSlot::Effects(effects)) => format!("{} · effects", fighters(effects)),
        Some(MeleeSlot::Stage(stage)) => stage.name().to_owned(),
        None => "No slot to go in".to_owned(),
    }
}

/// Who made it: the player for one they saved from the editor, else the
/// file it was added from.
pub fn maker(skin: &Skin) -> &'static str {
    if made(skin) {
        "by You"
    } else {
        "Added from a file"
    }
}

/// Whether tgg-melee plays `skin` now.
pub fn in_game(app: &App, skin: &Skin) -> bool {
    skin.slot
        .is_some_and(|slot| super::port::plays(app, skin.id, slot))
}

/// Whether the player made `skin`: saved it from the editor.
pub fn made(skin: &Skin) -> bool {
    matches!(skin.source, SkinSource::Edited { .. })
}

/// What a skin changes.
pub fn kind(skin: &Skin) -> Option<crate::melee::Kind> {
    use crate::melee::Kind;
    match skin.slot? {
        MeleeSlot::Costume { .. } | MeleeSlot::FighterData(_) => Some(Kind::Fighters),
        MeleeSlot::Effects(_) => Some(Kind::Effects),
        MeleeSlot::Stage(_) => Some(Kind::Stages),
    }
}

/// Whether the player's Melee disc holds `skin` in its slot now: for "Also
/// in Slippi" and its undo.
pub fn in_iso(app: &App, skin: &Skin) -> bool {
    use tgg_app::install::SlotState;
    let states = app.melee.states.as_ref();
    skin.slot.is_some_and(|slot| {
        matches!(
            states.and_then(|states| states.get(&slot)),
            Some(SlotState::Skin(there)) if there.id == skin.id
        )
    })
}

/// The frame a sheet's picture sits in: square, rounded.
pub fn sheet_plate(p: &Palette, width: f32) -> El {
    card_plate(p).w(width).aspect_ratio(1.0)
}

/// A row of skins, wrapping to the page.
pub fn grid(cards: Vec<El>) -> El {
    row().flex_wrap().gap(space::MD).children(cards)
}
