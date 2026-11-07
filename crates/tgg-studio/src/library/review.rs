//! Review: the files the player chose, before any joins the library. Each
//! says where it seems to go, and a costume can be moved to another of its
//! fighter's colors; files the app can't use say why. Then the player adds
//! them, or adds and installs them. It's modal: Escape or a click outside
//! cancels.

use super::{Review, actions};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, space, text};
use crate::ui::{ButtonKind, button, rule};
use melee_dat::MeleeSlot;
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_app::costumes::slot_label;

/// The dialog's width, in points.
const WIDTH: f32 = 560.0;

/// The dialog over a scrim, when files are being reviewed.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    let review = app.library_page.review.as_ref()?;
    let dialog = col().key("review").w(WIDTH).max_h(640.0).fill(p.raise).border(1.0, p.line_strong).radius(radius::CONTROL)
        .shadow(24.0, 60.0, rune::Color::BLACK.alpha(0.55))
        .role("dialog").label("Add skins")
        .modal(true, actions::cancel_review)
        .child(col().px(space::LG).pt(space::LG).pb(space::MD).gap(4.0)
            .child(label(title(review)).font(text::H3).color(p.ink))
            .child(label("Check where each goes before they join your library.").font(text::BODY).color(p.muted)))
        .child(rule(p))
        .child(col().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false })
            .child(col().children(items(app, review, p)).children(rejected(review, p))))
        .child(rule(p))
        .child(buttons(app, review, p));
    Some(el().key("review/scrim").absolute(0.0, 0.0).w_full().h_full().fill(rune::Color::BLACK.alpha(0.6)).center().child(dialog))
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
        let goes = col().gap(4.0)
            .child(label(format!("Goes in {}", slot_label(item.slot))).font(text::SMALL).color(p.muted))
            .children(colors(app, index, item.slot, p));
        col().key(format!("review/item/{index}")).px(space::LG).py(space::SM).gap(4.0)
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
        let chosen = slot == Some(to);
        label(costume.color.name()).key(format!("review/item/{index}/{}", to.file_name()))
            .font(text::LABEL).color(if chosen { p.ink } else { p.muted })
            .px(8.0).py(3.0).radius(radius::CONTROL)
            .fill(if chosen { p.selected } else { rune::Color::TRANSPARENT }).border(1.0, if chosen { p.accent_text } else { p.line })
            .hover(p.hover).cursor(rune::Cursor::Pointer).role("radio").label(costume.color.name()).checked(chosen).focusable(true)
            .on_press(move |app: &mut App| actions::set_review_slot(app, index, to))
    });
    Some(row().flex_wrap().gap(6.0).children(chips))
}

/// Files the app can't use, each with why.
fn rejected<'a>(review: &'a Review, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    review.rejected.iter().enumerate().map(move |(index, rejected)| {
        col().key(format!("review/rejected/{index}")).px(space::LG).py(space::SM).gap(2.0)
            .child(label(rejected.file_name.clone()).font(text::MONO_TEXT).color(p.muted).truncate())
            .child(label(format!("Can't be added: {}", rejected.reason)).font(text::SMALL).color(p.danger).wrap())
    })
}

fn buttons(app: &App, review: &Review, p: &Palette) -> El {
    let any = !review.items.is_empty();
    let mut buttons = row().px(space::LG).py(space::MD).gap(space::XS).items_center()
        .child(button("Cancel", ButtonKind::Secondary, p).key("review/cancel").on_press(actions::cancel_review))
        .child(el().grow(1.0));
    if any {
        buttons = buttons.child(button("Add", ButtonKind::Secondary, p).key("review/add")
            .on_press(|app: &mut App| actions::confirm_review(app, false)));
        if app.game.is_some() {
            buttons = buttons.child(button("Add and install", ButtonKind::Primary, p).key("review/install")
                .on_press(|app: &mut App| actions::confirm_review(app, true)));
        }
    }
    buttons
}
