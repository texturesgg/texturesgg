//! Melee's page: the roster, fighters or stages, as plates in a grid.

use super::{Melee, Subject, Tab, actions};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{self, card_plate, page_head, rule, tab};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, row, text as label};

pub fn view(app: &App, p: &Palette) -> El {
    let melee = &app.melee;
    if let Some(document) = &app.editor {
        return crate::editor::view::view(app, document, p);
    }
    if let Some(subject) = melee.subject {
        return super::subject::view(app, subject, p);
    }
    let tabs = row().gap(space::LG)
        .child(tab("Fighters", melee.fighters.len(), melee.tab == Tab::Fighters, p)
            .key("melee/tab/fighters").on_press(|app: &mut App| actions::show_tab(app, Tab::Fighters)))
        .child(tab("Stages", melee.stages.len(), melee.tab == Tab::Stages, p)
            .key("melee/tab/stages").on_press(|app: &mut App| actions::show_tab(app, Tab::Stages)));
    let cards: Vec<El> = match melee.tab {
        Tab::Fighters => melee.fighters.iter().map(|fighter| {
            let picture = Melee::shown_slot(fighter);
            let changed = melee.changed_count(fighter);
            let character = fighter.character;
            card(app, Subject::Fighter(character), picture, status(melee, changed), p)
                .key(format!("melee/fighter/{}", character.name()))
                .on_press(move |app: &mut App| actions::open_fighter(app, character))
        }).collect(),
        Tab::Stages => melee.stages.iter().map(|stage| {
            let slot = MeleeSlot::Stage(*stage);
            let changed = usize::from(melee.changed(slot));
            let stage = *stage;
            card(app, Subject::Stage(stage), Some(slot), status(melee, changed), p)
                .key(format!("melee/stage/{}", stage.file_name()))
                .on_press(move |app: &mut App| actions::open_stage(app, stage))
        }).collect(),
    };
    let page = col().px(space::LG + 4.0).pt(space::XS).pb(space::XL).gap(space::MD)
        .child(page_head("Melee", "What's in your game, and what you've changed.", p))
        .children(melee.problem.as_ref().map(|problem| label(problem.clone()).key("melee/problem").font(text::BODY).color(p.danger)))
        .child(col().child(tabs).child(rule(p)))
        .child(row().flex_wrap().gap(space::MD).children(cards));
    col().key("melee/scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page)
}

/// What a plate's line says: nothing until the slots are read, then
/// "Vanilla" or how many files are the player's.
fn status(melee: &Melee, changed: usize) -> Option<(String, bool)> {
    melee.states.as_ref()?;
    Some(match changed {
        0 => ("Vanilla".into(), false),
        changed => (format!("{changed} custom"), true),
    })
}

/// A fighter or stage: its picture on a plate, its name, and its status
/// line, with a gold dot when something in it is the player's.
fn card(app: &App, subject: Subject, picture: Option<MeleeSlot>, status: Option<(String, bool)>, p: &Palette) -> El {
    let name = subject.name();
    let pixels = picture.and_then(|slot| app.renders.get(RenderKey::Slot(slot)));
    let mut plate = card_plate(p).matched(subject.plate_id());
    if let Some(pixels) = pixels {
        // Square renders fill the 4:5 plate, trimming the empty sides.
        plate = plate.pixels_fit(pixels, rune::SurfaceFit::Fill);
    }
    // Without a status, a blank line keeps the cards' names level.
    let (words, mine) = status.unwrap_or_else(|| (" ".to_owned(), false));
    ui::card(plate, name, ui::status(words, mine, p), p)
        .cursor(rune::Cursor::Pointer).role("button").label(name).focusable(true)
}
