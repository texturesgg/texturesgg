//! The player's skins, under Skins' "Your skins" tab, as plates grouped by
//! the fighter each was made for, newest first; stages and skins without a
//! slot last.

use super::{Filter, actions};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    self, ButtonKind, TEXT_ACTION_INSET, button, card_plate, chip, spacer, text_action,
};
use melee_dat::MeleeSlot;
use rune::{El, col, row, text as label};
use tgg_app::costumes::{EFFECTS_FILE, FIGHTER_FILE, fighters};
use tgg_app::install::SlotState;
use tgg_app::library::{Skin, SkinSource};

/// Skins' "Your skins" tab: which of them show, adding more, and the skins
/// grouped by fighter.
pub fn body(app: &App, p: &Palette) -> Vec<El> {
    let skins = app.library.skins();
    let in_game = skins.iter().filter(|skin| installed(app, skin)).count();
    let made_here = skins.iter().filter(|skin| made(skin)).count();
    let filter = app.library_page.filter;
    let choice = |name: String, which: Filter, key: &str| {
        chip(&name, filter == which, p)
            .key(key.to_owned())
            .role("radio")
            .on_press(move |app: &mut App| actions::show_filter(app, which))
    };
    let top = row()
        .gap(space::SNUG)
        .items_center()
        .child(choice(
            format!("All {}", skins.len()),
            Filter::All,
            "library/tab/all",
        ))
        .child(choice(
            format!("Installed {in_game}"),
            Filter::InGame,
            "library/tab/in-game",
        ))
        .child(choice(
            format!("Not installed {}", skins.len() - in_game),
            Filter::NotInstalled,
            "library/tab/not-installed",
        ))
        .child(choice(
            format!("Made by you {made_here}"),
            Filter::Made,
            "library/tab/made",
        ))
        .child(spacer())
        .child(
            button("Add a file…", ButtonKind::Secondary, p)
                .key("library/add-file")
                .on_press_tx(actions::choose_file),
        )
        .child(
            button("Add a folder…", ButtonKind::Secondary, p)
                .key("library/add-folder")
                .on_press_tx(actions::choose_folder),
        );
    let mut body = vec![top];
    if skins.is_empty() {
        body.push(
            label("You have no skins yet. Add them from a file, a zip, or a folder of them.")
                .key("library/empty")
                .font(text::BODY)
                .color(p.muted),
        );
    } else {
        body.extend(groups(app).into_iter().map(|(name, skins)| {
            col()
                .gap(space::SM)
                .child(label(name).font(text::LABEL).color(p.faint))
                .child(
                    row()
                        .flex_wrap()
                        .gap(space::MD)
                        .children(skins.into_iter().map(|skin| card(app, skin, p))),
                )
        }));
    }
    body
}

/// How many skins the player has, for the tab's count.
pub fn count(app: &App) -> usize {
    app.library.skins().len()
}

/// Whether `skin` is in either game now: the Slippi ISO or tgg-melee.
fn installed(app: &App, skin: &Skin) -> bool {
    in_slippi(app, skin) || in_tgg_melee(app, skin)
}

/// Whether `skin` is in the Slippi ISO now.
fn in_slippi(app: &App, skin: &Skin) -> bool {
    let states = app.melee.states.as_ref();
    skin.slot.is_some_and(|slot| matches!(states.and_then(|states| states.get(&slot)), Some(SlotState::Skin(there)) if there.id == skin.id))
}

/// Whether tgg-melee plays `skin` now.
fn in_tgg_melee(app: &App, skin: &Skin) -> bool {
    skin.slot
        .is_some_and(|slot| super::port::plays(app, skin.id, slot))
}

/// The skins the tab shows, grouped by the fighter they were made for (newest
/// first), stages and skins without a slot last.
fn groups(app: &App) -> Vec<(String, Vec<&Skin>)> {
    let mut groups: Vec<(String, Vec<&Skin>)> = Vec::new();
    for skin in app.library.skins().iter().rev() {
        let shown = match app.library_page.filter {
            Filter::All => true,
            Filter::InGame => installed(app, skin),
            Filter::NotInstalled => !installed(app, skin),
            Filter::Made => made(skin),
        };
        if !shown {
            continue;
        }
        let group = match skin.slot {
            Some(MeleeSlot::Costume { character, .. } | MeleeSlot::FighterData(character)) => {
                character.name().to_owned()
            }
            Some(MeleeSlot::Effects(effects)) => fighters(effects),
            Some(MeleeSlot::Stage(_)) => "Stages".to_owned(),
            None => "Without a slot".to_owned(),
        };
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, skins)) => skins.push(skin),
            None => groups.push((group, vec![skin])),
        }
    }
    groups.sort_by_key(|(name, _)| matches!(name.as_str(), "Stages" | "Without a slot"));
    groups
}

/// A skin: its picture, its name, where it goes, whether it's in the game,
/// and installing or removing it.
pub fn card(app: &App, skin: &Skin, p: &Palette) -> El {
    let mut picture = card_plate(p);
    if let Some(pixels) = app.renders.get(RenderKey::Skin(skin.id)) {
        picture = picture.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    // The fighter is the group's heading: say the color, the file, or the stage.
    let goes = match skin.slot {
        Some(MeleeSlot::Costume { color, .. }) => color.name().to_owned(),
        Some(MeleeSlot::FighterData(_)) => FIGHTER_FILE.to_owned(),
        Some(MeleeSlot::Effects(_)) => EFFECTS_FILE.to_owned(),
        Some(MeleeSlot::Stage(stage)) => stage.name().to_owned(),
        None => "No slot".to_owned(),
    };
    let (slippi, tgg_melee) = (in_slippi(app, skin), in_tgg_melee(app, skin));
    let id = skin.id;
    let status = ui::status(
        match (slippi, tgg_melee) {
            (true, true) => format!("{goes} · in Slippi and tgg-melee"),
            (true, false) => format!("{goes} · in Slippi"),
            (false, true) => format!("{goes} · in tgg-melee"),
            (false, false) => goes,
        },
        slippi || tgg_melee,
        p,
    );
    // Installing it, into whichever game doesn't have it yet.
    let into_slippi = skin
        .slot
        .filter(|_| !slippi && app.game.is_some())
        .map(|slot| {
            text_action("Into Slippi", false, p)
                .key(format!("library/install/{}", skin.id))
                .help("Install it into your Slippi ISO")
                .on_press(move |app: &mut App| actions::install(app, id, slot))
        });
    let into_tgg_melee = skin.slot.filter(|_| !tgg_melee).map(|slot| {
        text_action("Into tgg-melee", false, p)
            .key(format!("library/install-port/{}", skin.id))
            .help("Install it into tgg-melee")
            .on_press(move |app: &mut App| super::port::install(app, id, slot))
    });
    let remove = text_action("Remove", true, p)
        .key(format!("library/remove/{}", skin.id))
        .on_press(move |app: &mut App| actions::remove(app, id));
    // The actions pull back their own padding, so their words line up with the name's.
    ui::card(picture, &skin.name, status, p)
        .key(format!("library/skin/{}", skin.id))
        .child(
            row()
                .gap(space::XXS)
                .ml(-TEXT_ACTION_INSET)
                .children(into_slippi)
                .children(into_tgg_melee)
                .child(remove),
        )
}

/// Whether the player made `skin`: saved it from the editor.
pub fn made(skin: &Skin) -> bool {
    matches!(skin.source, SkinSource::Edited { .. })
}
