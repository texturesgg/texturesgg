//! A fighter or stage opened: the stage, with the timeline under it, and the
//! pane column beside it: Costumes (a fighter's, to choose between), Shared
//! (the files its costumes share, and the models in them) and Moves.

use super::{Melee, Pane, Subject, actions};
use crate::stage::{actions as stage, view as stage_view};
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{pane, pane_header, pane_row};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_app::costumes::{EFFECTS_FILE, FIGHTER_FILE};
use tgg_app::install::SlotState;
use tgg_app::moves::MoveRow;

pub fn view(app: &App, subject: Subject, p: &Palette) -> El {
    let stage = col().grow(1.0).min_w(0.0).h_full().gap(space::SM)
        .child(stage_view::plate(app, &subject.plate_id(), p))
        .child(stage_view::timeline(app, p));
    let panes: Vec<El> = [app.melee.fighter().map(|_| costumes(app, p)), shared(app, p), moves(app, p)].into_iter().flatten().collect();
    // A stage has no panes yet; then it takes the whole width.
    let column = (!panes.is_empty()).then(|| {
        col().key("melee/panes").w(size::PANE_COLUMN).h_full().gap(space::SM)
            .scroll(ScrollSpec { stick_bottom: false })
            .children(panes)
    });
    row().key("melee/subject").grow(1.0).min_h(0.0).w_full().px(space::MD).pb(space::MD).gap(space::MD)
        .child(stage)
        .children(column)
}

/// The title row's breadcrumbs while something's open: "Melee / Fox", where
/// "Melee" goes back.
pub fn crumbs(subject: Subject, p: &Palette) -> El {
    row().gap(space::XS).items_center()
        .child(label("Melee").key("melee/crumb").font(text::STRONG).color(p.muted).role("button").focusable(true)
            .on_press(|app: &mut App| {
                actions::back(app);
            }))
        .child(label("/").font(text::BODY).color(p.faint))
        .child(label(subject.name()).font(text::STRONG).color(p.ink))
}

fn costumes(app: &App, p: &Palette) -> El {
    let melee = &app.melee;
    let Some(fighter) = melee.fighter() else { return col() };
    let folded = melee.panes.folded(Pane::Costumes);
    let header = pane_header("Costumes", Some(fighter.costumes.len()), folded, p)
        .key("melee/pane/costumes").on_press(|app: &mut App| actions::toggle_pane(app, Pane::Costumes));
    let showing = app.stage.shown.as_ref().map(|shown| shown.slot);
    let body = (!folded).then(|| {
        col().children(fighter.costumes.iter().map(|costume| {
            let slot = costume.slot(fighter);
            pane_row(label(costume.color.name()).font(text::STRONG).color(p.ink), Some(state_word(melee, slot, p)), showing == Some(slot), p)
                .key(format!("melee/costume/{}", slot.file_name()))
                .role("button").label(costume.color.name()).focusable(true)
                .on_press(move |app: &mut App| actions::choose_costume(app, slot))
        }))
    });
    pane(header, body, p)
}

/// The files every costume shares, each with the models in it.
fn shared(app: &App, p: &Palette) -> Option<El> {
    let melee = &app.melee;
    let fighter = melee.fighter()?;
    if fighter.shared.is_empty() {
        return None;
    }
    let folded = melee.panes.folded(Pane::Shared);
    let header = pane_header("Shared", Some(fighter.shared.len()), folded, p)
        .key("melee/pane/shared").on_press(|app: &mut App| actions::toggle_pane(app, Pane::Shared));
    let body = (!folded).then(|| {
        col().children(fighter.shared.iter().map(|&file| {
            let name = match file {
                MeleeSlot::FighterData(_) => FIGHTER_FILE,
                _ => EFFECTS_FILE,
            };
            let models: Vec<&str> = fighter.models(file).map(|model| model.name()).collect();
            let left = col()
                .child(label(name).font(text::STRONG).color(p.ink))
                .children((!models.is_empty()).then(|| label(models.join(", ")).font(text::SMALL).color(p.muted).truncate()));
            pane_row(left, Some(state_word(melee, file, p)), false, p).h(48.0)
        }))
    });
    Some(pane(header, body, p))
}

/// The fighter's moves, grouped; choosing one plays it.
fn moves(app: &App, p: &Palette) -> Option<El> {
    let shown = app.stage.shown.as_ref()?;
    if shown.moves.is_empty() {
        return None;
    }
    let playable = shown.moves.iter().filter(|row| matches!(row, MoveRow::Move { refused: None, .. })).count();
    let folded = app.melee.panes.folded(Pane::Moves);
    let header = pane_header("Moves", Some(playable), folded, p)
        .key("melee/pane/moves").on_press(|app: &mut App| actions::toggle_pane(app, Pane::Moves));
    let body = (!folded).then(|| {
        col().children(shown.moves.iter().map(|row| match row {
            MoveRow::Heading(group) => label(group.clone()).font(text::LABEL).color(p.faint).px(space::SM).pt(space::SM).pb(space::XXS),
            MoveRow::Move { animation, title, detail, refused } => {
                let animation = *animation;
                let ink = if refused.is_some() { p.faint } else { p.ink };
                let right = (!detail.is_empty()).then(|| label(detail.clone()).font(text::MONO_SMALL).color(p.faint));
                let mut entry = pane_row(label(title.clone()).font(text::BODY).color(ink).truncate(), right, shown.animation == Some(animation), p)
                    .key(format!("melee/move/{animation}"));
                if let Some(reason) = refused {
                    entry = entry.help(reason.clone());
                } else {
                    entry = entry.role("button").label(title.clone()).focusable(true)
                        .on_press(move |app: &mut App| stage::play(app, animation));
                }
                entry
            }
        }))
    });
    Some(pane(header, body, p))
}

/// What a slot holds, in a word: the player's skin, a file the app doesn't
/// know, or the game's own.
fn state_word(melee: &Melee, slot: MeleeSlot, p: &Palette) -> El {
    let (word, mine) = match melee.states.as_ref().and_then(|states| states.get(&slot)) {
        Some(SlotState::Skin(_)) => ("Yours", true),
        Some(SlotState::Custom) => ("Custom", true),
        Some(SlotState::Vanilla) => ("Original", false),
        None => ("", false),
    };
    label(word).font(text::SMALL).color(if mine { p.accent_text } else { p.faint })
}
