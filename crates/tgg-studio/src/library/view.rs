//! The Library's page: the player's skins as plates, grouped by the fighter
//! each was made for, newest first; stages and skins without a slot last.

use super::{Filter, actions};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    self, ButtonKind, TEXT_ACTION_INSET, button, card_plate, page_head, rule, tab, text_action,
};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_app::costumes::{EFFECTS_FILE, FIGHTER_FILE, fighters};
use tgg_app::install::SlotState;
use tgg_app::library::Skin;

pub fn view(app: &App, p: &Palette) -> El {
    let skins = app.library.skins();
    let in_game = skins.iter().filter(|skin| installed(app, skin)).count();
    let head = row()
        .items_end()
        .gap(space::SM)
        .child(col().grow(1.0).child(page_head(
            "Library",
            &format!("{} skins, {in_game} in your game.", skins.len()),
            p,
        )))
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
    let filter = app.library_page.filter;
    let tabs = row()
        .gap(space::LG)
        .child(
            tab("All", skins.len(), filter == Filter::All, p)
                .key("library/tab/all")
                .on_press(|app: &mut App| actions::show_filter(app, Filter::All)),
        )
        .child(
            tab("In your game", in_game, filter == Filter::InGame, p)
                .key("library/tab/in-game")
                .on_press(|app: &mut App| actions::show_filter(app, Filter::InGame)),
        )
        .child(
            tab(
                "Not installed",
                skins.len() - in_game,
                filter == Filter::NotInstalled,
                p,
            )
            .key("library/tab/not-installed")
            .on_press(|app: &mut App| actions::show_filter(app, Filter::NotInstalled)),
        );
    let groups = groups(app);
    let body = if skins.is_empty() {
        vec![
            label("Your library is empty. Add skins from a file, a zip, or a folder of them.")
                .key("library/empty")
                .font(text::BODY)
                .color(p.muted),
        ]
    } else {
        groups
            .into_iter()
            .map(|(name, skins)| {
                col()
                    .gap(space::SM)
                    .child(label(name).font(text::LABEL).color(p.faint))
                    .child(
                        row()
                            .flex_wrap()
                            .gap(space::MD)
                            .children(skins.into_iter().map(|skin| card(app, skin, p))),
                    )
            })
            .collect()
    };
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::MD)
        .child(head)
        .child(col().child(tabs).child(rule(p)))
        .children(body);
    col()
        .key("library/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// Whether `skin` is in the player's game now.
fn installed(app: &App, skin: &Skin) -> bool {
    let states = app.melee.states.as_ref();
    skin.slot.is_some_and(|slot| matches!(states.and_then(|states| states.get(&slot)), Some(SlotState::Skin(there)) if there.id == skin.id))
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
fn card(app: &App, skin: &Skin, p: &Palette) -> El {
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
    let in_game = installed(app, skin);
    let id = skin.id;
    let status = ui::status(
        if in_game {
            format!("{goes} · in your game")
        } else {
            goes
        },
        in_game,
        p,
    );
    let install = skin
        .slot
        .filter(|_| !in_game && app.game.is_some())
        .map(|slot| {
            text_action("Install", false, p)
                .key(format!("library/install/{}", skin.id))
                .on_press(move |app: &mut App| actions::install(app, id, slot))
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
                .children(install)
                .child(remove),
        )
}
