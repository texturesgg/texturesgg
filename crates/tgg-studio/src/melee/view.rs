//! Skins' page: the roster of the player's ISO, fighters or stages, as
//! plates in a grid, and the skins they have; the costumes and stages
//! edited lately sit under the head.

use super::{Melee, Subject, Tab, actions};
use crate::jobs::renders::RenderKey;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{self, TEXT_ACTION_INSET, card_plate, page_head, rule, tab, text_action};
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
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, melee.tab == which, p)
            .key(key.to_owned())
            .on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    let tabs = row()
        .gap(space::LG)
        .child(tab_for(
            "Fighters",
            melee.fighters.len(),
            Tab::Fighters,
            "melee/tab/fighters",
        ))
        .child(tab_for(
            "Stages",
            melee.stages.len(),
            Tab::Stages,
            "melee/tab/stages",
        ))
        .child(tab_for(
            "Your skins",
            crate::library::view::count(app),
            Tab::Mine,
            "melee/tab/mine",
        ));
    let body: Vec<El> = match melee.tab {
        Tab::Fighters => vec![grid(
            melee
                .fighters
                .iter()
                .map(|fighter| {
                    let picture = Melee::shown_slot(fighter);
                    let changed = melee.changed_count(fighter);
                    let character = fighter.character;
                    card(
                        app,
                        Subject::Fighter(character),
                        picture,
                        status(melee, changed),
                        p,
                    )
                    .key(format!("melee/fighter/{}", character.name()))
                    .on_press(move |app: &mut App| actions::open_fighter(app, character))
                })
                .collect(),
        )],
        Tab::Stages => vec![grid(
            melee
                .stages
                .iter()
                .map(|stage| {
                    let slot = MeleeSlot::Stage(*stage);
                    let changed = usize::from(melee.changed(slot));
                    let stage = *stage;
                    card(
                        app,
                        Subject::Stage(stage),
                        Some(slot),
                        status(melee, changed),
                        p,
                    )
                    .key(format!("melee/stage/{}", stage.file_name()))
                    .on_press(move |app: &mut App| actions::open_stage(app, stage))
                })
                .collect(),
        )],
        Tab::Mine => crate::library::view::body(app, p),
    };
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::MD)
        .child(
            row()
                .items_end()
                .gap(space::LG)
                .child(col().grow(1.0).min_w(0.0).child(page_head(
                    crate::nav::Place::Skins.label(),
                    "Your skins, for Slippi and tgg-melee.",
                    p,
                )))
                .children(game(app, p)),
        )
        .children(recent(app, p))
        .children(melee.problem.as_ref().map(|problem| {
            label(problem.clone())
                .key("melee/problem")
                .font(text::BODY)
                .color(p.danger)
        }))
        .child(col().child(tabs).child(rule(p)))
        .children(body);
    col()
        .key("melee/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// Plates, wrapping to the page.
fn grid(cards: Vec<El>) -> El {
    row().flex_wrap().gap(space::MD).children(cards)
}

/// The costumes and stages edited lately, the latest first: pressing one
/// opens it for editing again (through the unsaved-changes prompt).
fn recent(app: &App, p: &Palette) -> Option<El> {
    let slots = &app.settings.recent;
    if slots.is_empty() {
        return None;
    }
    Some(
        row()
            .key("skins/recent")
            .flex_wrap()
            .items_center()
            .gap(space::XXS)
            .child(
                label("Recently edited")
                    .font(text::LABEL)
                    .color(p.faint)
                    .pl(TEXT_ACTION_INSET)
                    .mr(space::XXS),
            )
            .children(slots.iter().copied().map(|slot| {
                text_action(&tgg_app::costumes::slot_label(Some(slot)), false, p)
                    .key(format!("skins/recent/{}", slot.file_name()))
                    .on_press_tx(move |app: &mut App, tx| {
                        crate::editor::actions::edit_recent(app, slot, tx)
                    })
            }))
            .ml(-TEXT_ACTION_INSET),
    )
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
fn card(
    app: &App,
    subject: Subject,
    picture: Option<MeleeSlot>,
    status: Option<(String, bool)>,
    p: &Palette,
) -> El {
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
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .label(name)
        .focusable(true)
}

/// The head's side: the game skins go into, its file, and choosing another.
fn game(app: &App, p: &Palette) -> Option<El> {
    let references = app.game.as_ref()?;
    let file = references
        .game()
        .path()
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    Some(
        col()
            .key("melee/game")
            .items_end()
            .gap(space::TIGHT)
            .max_w(GAME_SIDE)
            .min_w(0.0)
            .child(label("Melee NTSC 1.02").font(text::STRONG).color(p.ink))
            .child(
                label(file)
                    .font(text::MONO_SMALL)
                    .color(p.muted)
                    .truncate_middle()
                    .min_w(0.0),
            )
            .child(
                row().mr(-ui::TEXT_ACTION_INSET).child(
                    ui::text_action("Choose another ISO…", false, p)
                        .key("melee/choose-game")
                        .on_press_tx(crate::preferences::actions::choose_game),
                ),
            ),
    )
}

/// The head's side, at its widest.
const GAME_SIDE: f32 = 420.0;
