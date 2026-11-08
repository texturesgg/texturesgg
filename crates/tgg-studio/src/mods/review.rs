//! Review: what an install or update will do, before it does it. Each mod
//! says whether it's new, an update from which version, or already there;
//! the download's size; and what won't work, with why. It's modal: Escape
//! or a click outside cancels.

use super::{actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{ButtonKind, Dialog, DialogWidth, Tone, badge, button, dialog, spacer};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::{Action, Plan};

/// The list's tallest, in points; longer lists scroll.
const LIST_HEIGHT: f32 = 400.0;

/// The dialog over a scrim, while a plan is reviewed.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    let review = app.mods.review.as_ref()?;
    let plan = &review.plan;
    let list = col().max_h(LIST_HEIGHT).scroll(ScrollSpec { stick_bottom: false })
        .child(col().children(steps(plan, p)).children(problems(plan, p)));
    let parts = Dialog { title: review.title.clone(), line: line(plan), body: Some(list), buttons: buttons(plan, p) };
    Some(dialog("mods/review", "dialog", DialogWidth::Wide, parts, actions::cancel_review, p))
}

/// What the plan comes to, in a line.
fn line(plan: &Plan) -> String {
    let new = plan.steps.iter().filter(|step| step.action == Action::Install).count();
    let updated = plan.steps.iter().filter(|step| matches!(step.action, Action::Update { .. })).count();
    let mut parts = Vec::new();
    if new > 0 {
        parts.push(format!("{new} new"));
    }
    if updated > 0 {
        parts.push(format!("{updated} updated"));
    }
    if parts.is_empty() {
        return "Everything here is installed and up to date.".into();
    }
    format!("{}, {} to download.", parts.join(", "), size_text(plan.size()))
}

/// Each mod the plan touches: its name and version, and what happens to it.
fn steps<'a>(plan: &'a Plan, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    plan.steps.iter().map(move |step| {
        let (words, tone) = match &step.action {
            Action::Install => ("New".to_owned(), Tone::News),
            Action::Update { from } => (format!("Update from {from}"), Tone::News),
            Action::Keep => ("Installed".to_owned(), Tone::Plain),
        };
        let size = step.download.as_ref().map(|package| label(size_text(package.size)).font(text::MONO_SMALL).color(p.faint));
        row().key(format!("mods/review/{}", step.id)).px(space::LG).py(space::XS).gap(space::SM).items_center()
            .child(col().gap(space::TIGHT).grow(1.0).min_w(0.0)
                .child(row().gap(space::XS).items_end()
                    .child(label(step.manifest.name.clone()).font(text::STRONG).color(p.ink).truncate())
                    .child(label(step.manifest.version.to_string()).font(text::MONO_SMALL).color(p.faint)))
                .children(step.manifest.description.clone().map(|line| label(line).font(text::SMALL).color(p.muted).truncate())))
            .children(size)
            .child(badge(words, tone, p))
    })
}

/// What won't work, each with why.
fn problems<'a>(plan: &'a Plan, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    plan.problems().iter().enumerate().map(move |(index, problem)| {
        let mut words = problem.to_string();
        if let Some(first) = words.get(..1) {
            words = first.to_uppercase() + &words[1..];
        }
        label(words).key(format!("mods/review/problem/{index}")).px(space::LG).py(space::XS).font(text::SMALL).color(p.danger).wrap()
    })
}

fn buttons(plan: &Plan, p: &Palette) -> Vec<El> {
    let mut buttons = vec![button("Cancel", ButtonKind::Secondary, p).key("mods/review/cancel").on_press(actions::cancel_review), spacer()];
    // A plan that downloads nothing still records the set it installs.
    if !plan.steps.is_empty() {
        buttons.push(button("Install", ButtonKind::Primary, p).key("mods/review/confirm").on_press(actions::confirm_review));
    }
    buttons
}
