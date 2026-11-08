//! Review: the files the player chose, before any joins the library. Each
//! says where it seems to go, and a costume can be moved to another of its
//! fighter's colors; files the app can't use say why. Then the player adds
//! them, or adds and installs them. It's modal: Escape or a click outside
//! cancels.

use super::{Review, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{ButtonKind, Dialog, DialogWidth, button, chip, dialog, spacer};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_app::costumes::slot_label;

/// The file list's tallest, in points; longer lists scroll.
const LIST_HEIGHT: f32 = 440.0;

/// The dialog over a scrim, when files are being reviewed.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    let review = app.library_page.review.as_ref()?;
    // A most height, not grow: the dialog has none of its own to grow into.
    let list = col().max_h(LIST_HEIGHT).scroll(ScrollSpec { stick_bottom: false })
        .child(col().children(items(app, review, p)).children(rejected(review, p)));
    let parts = Dialog {
        title: title(review),
        line: "Check where each goes before they join your library.".into(),
        body: Some(list),
        buttons: buttons(app, review, p),
    };
    Some(dialog("review", "dialog", DialogWidth::Wide, parts, actions::cancel_review, p))
}

fn title(review: &Review) -> String {
    match review.items.len() {
        0 => "Nothing to add".into(),
        1 => "Add 1 skin".into(),
        count => format!("Add {count} skins"),
    }
}

/// Each file: its name, its file name, and where it goes, with the other
/// colors of its fighter to move it to.
fn items<'a>(app: &'a App, review: &'a Review, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    review.items.iter().enumerate().map(move |(index, item)| {
        let goes = col().gap(space::XXS)
            .child(label(format!("Goes in {}", slot_label(item.slot))).font(text::SMALL).color(p.muted))
            .children(colors(app, index, item.slot, p));
        col().key(format!("review/item/{index}")).px(space::LG).py(space::SM).gap(space::XXS)
            .child(row().gap(space::XS).items_end()
                .child(label(item.candidate.name.clone()).font(text::STRONG).color(p.ink).truncate())
                .child(label(item.candidate.file_name.clone()).font(text::MONO_SMALL).color(p.faint).truncate()))
            .child(goes)
    })
}

/// The colors of the fighter a costume is for, to move it to another.
fn colors(app: &App, index: usize, slot: Option<MeleeSlot>, p: &Palette) -> Option<El> {
    let Some(MeleeSlot::Costume { character, .. }) = slot else { return None };
    let fighter = app.melee.fighters.iter().find(|fighter| fighter.character == character)?;
    let chips = fighter.costumes.iter().map(|costume| {
        let to = costume.slot(fighter);
        chip(costume.color.name(), slot == Some(to), p).key(format!("review/item/{index}/{}", to.file_name())).role("radio")
            .on_press(move |app: &mut App| actions::set_review_slot(app, index, to))
    });
    Some(row().flex_wrap().gap(space::SNUG).children(chips))
}

/// Files the app can't use, each with why.
fn rejected<'a>(review: &'a Review, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    review.rejected.iter().enumerate().map(move |(index, rejected)| {
        col().key(format!("review/rejected/{index}")).px(space::LG).py(space::SM).gap(space::TIGHT)
            .child(label(rejected.file_name.clone()).font(text::MONO_TEXT).color(p.muted).truncate())
            .child(label(format!("Can't be added: {}", rejected.reason)).font(text::SMALL).color(p.danger).wrap())
    })
}

fn buttons(app: &App, review: &Review, p: &Palette) -> Vec<El> {
    let mut buttons = vec![button("Cancel", ButtonKind::Secondary, p).key("review/cancel").on_press(actions::cancel_review), spacer()];
    if !review.items.is_empty() {
        buttons.push(button("Add", ButtonKind::Secondary, p).key("review/add").on_press(|app: &mut App| actions::confirm_review(app, false)));
        if app.game.is_some() {
            buttons.push(button("Add and install", ButtonKind::Primary, p).key("review/install")
                .on_press(|app: &mut App| actions::confirm_review(app, true)));
        }
    }
    buttons
}
