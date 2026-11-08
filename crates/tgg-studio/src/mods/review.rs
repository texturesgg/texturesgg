//! Review: what an install or update will do, before it does it. What
//! changes comes first: each new mod, and each update with the version it
//! goes from and to; then the mods an update drops (removed with it unless
//! the player keeps them); then what's already up to date; then what won't
//! work, with why. It's modal: Escape or a click outside cancels.

use super::{Mods, Review, actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, Tone, badge, button, dialog, rule, spacer, switch,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::{Action, Plan, Step};

/// The list's tallest, in points; longer lists scroll.
const LIST_HEIGHT: f32 = 400.0;

/// The dialog over a scrim, while a plan is reviewed.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    let mods = &app.mods;
    let review = mods.review.as_ref()?;
    let plan = &review.plan;
    let (changing, kept): (Vec<&Step>, Vec<&Step>) = plan
        .steps
        .iter()
        .partition(|step| step.action != Action::Keep);
    let mut sections: Vec<El> = Vec::new();
    if !changing.is_empty() {
        sections.push(section(
            None,
            changing.into_iter().map(|step| step_row(step, p)).collect(),
            p,
        ));
    }
    if !review.dropped.is_empty() {
        sections.push(dropped(mods, review, p));
    }
    if !kept.is_empty() {
        let label = format!("Already up to date · {}", kept.len());
        sections.push(section(
            Some(label),
            kept.into_iter().map(|step| step_row(step, p)).collect(),
            p,
        ));
    }
    let list = col()
        .max_h(LIST_HEIGHT)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .pb(space::XS)
                .children(
                    sections
                        .into_iter()
                        .enumerate()
                        .flat_map(|(index, section)| {
                            (index > 0).then(|| rule(p)).into_iter().chain([section])
                        }),
                )
                .children(problems(plan, p)),
        );
    let parts = Dialog {
        title: review.title.clone(),
        line: line(review),
        body: Some(list),
        buttons: buttons(review, p),
    };
    Some(dialog(
        "mods/review",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::cancel_review,
        p,
    ))
}

/// What the plan comes to, in a line.
fn line(review: &Review) -> String {
    let plan = &review.plan;
    let new = plan
        .steps
        .iter()
        .filter(|step| step.action == Action::Install)
        .count();
    let updated = plan
        .steps
        .iter()
        .filter(|step| matches!(step.action, Action::Update { .. }))
        .count();
    let removed = if review.remove_dropped {
        review.dropped.len()
    } else {
        0
    };
    let mut parts = Vec::new();
    if new > 0 {
        parts.push(format!("{new} new"));
    }
    if updated > 0 {
        parts.push(format!("{updated} updated"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    if parts.is_empty() {
        return "Everything here is installed and up to date.".into();
    }
    if plan.size() == 0 {
        return format!("{}.", parts.join(", "));
    }
    format!(
        "{}, {} to download.",
        parts.join(", "),
        size_text(plan.size())
    )
}

/// A part of the list: its label, when it has one, over its rows.
fn section(label_text: Option<String>, rows: Vec<El>, p: &Palette) -> El {
    col()
        .pt(space::XS)
        .children(label_text.map(|words| {
            label(words)
                .font(text::LABEL)
                .color(p.faint)
                .px(space::LG)
                .py(space::XXS)
        }))
        .children(rows)
}

/// A mod in the plan: its name and line, the download, and what happens to
/// it: new, from one version to another, or nothing (already up to date).
fn step_row(step: &Step, p: &Palette) -> El {
    let version = step.manifest.version.to_string();
    let change = match &step.action {
        Action::Install => row()
            .gap(space::XS)
            .items_center()
            .child(label(version).font(text::MONO_SMALL).color(p.ink))
            .child(badge("New", Tone::News, p)),
        Action::Update { from } => row()
            .gap(space::XS)
            .items_center()
            .child(
                label(format!("{from} → {version}"))
                    .font(text::MONO_SMALL)
                    .color(p.ink),
            )
            .child(badge("Update", Tone::News, p)),
        Action::Keep => row().child(label(version).font(text::MONO_SMALL).color(p.faint)),
    };
    let size = step.download.as_ref().map(|package| {
        label(size_text(package.size))
            .font(text::MONO_SMALL)
            .color(p.faint)
    });
    let quiet = step.action == Action::Keep;
    row()
        .key(format!("mods/review/{}", step.id))
        .px(space::LG)
        .py(space::XS)
        .gap(space::SM)
        .items_center()
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    label(step.manifest.name.clone())
                        .font(text::STRONG)
                        .color(if quiet { p.muted } else { p.ink })
                        .truncate(),
                )
                .children(
                    step.manifest
                        .description
                        .clone()
                        .filter(|_| !quiet)
                        .map(|line| label(line).font(text::SMALL).color(p.muted).truncate()),
                ),
        )
        .children(size)
        .child(change)
}

/// The mods an update drops, and whether they go with it.
fn dropped(mods: &Mods, review: &Review, p: &Palette) -> El {
    let remove = review.remove_dropped;
    let rows = review.dropped.iter().map(|id| {
        let installed = mods
            .local
            .get(id)
            .map(|m| m.manifest.version.to_string())
            .unwrap_or_default();
        row()
            .key(format!("mods/review/dropped/{id}"))
            .px(space::LG)
            .py(space::XS)
            .gap(space::SM)
            .items_center()
            .child(
                label(mods.name_of(id))
                    .font(text::STRONG)
                    .color(p.ink)
                    .truncate()
                    .grow(1.0)
                    .min_w(0.0),
            )
            .child(label(installed).font(text::MONO_SMALL).color(p.faint))
            .child(badge(
                if remove { "Remove" } else { "Keep" },
                if remove { Tone::Danger } else { Tone::Plain },
                p,
            ))
    });
    let choice = row()
        .px(space::LG)
        .py(space::XS)
        .gap(space::XS)
        .items_center()
        .child(
            switch("mods/review/remove-dropped", remove, p)
                .label("Remove them")
                .on_press(move |app: &mut App| actions::set_remove_dropped(app, !remove)),
        )
        .child(
            label("Remove them with the update")
                .font(text::SMALL)
                .color(p.muted),
        );
    section(
        Some(format!(
            "No longer in the collection · {}",
            review.dropped.len()
        )),
        rows.chain([choice]).collect(),
        p,
    )
    .key("mods/review/dropped")
}

/// What won't work, each with why.
fn problems<'a>(plan: &'a Plan, p: &'a Palette) -> impl Iterator<Item = El> + 'a {
    plan.problems()
        .iter()
        .enumerate()
        .map(move |(index, problem)| {
            let mut words = problem.to_string();
            if let Some(first) = words.get(..1) {
                words = first.to_uppercase() + &words[1..];
            }
            label(words)
                .key(format!("mods/review/problem/{index}"))
                .px(space::LG)
                .py(space::XS)
                .font(text::SMALL)
                .color(p.danger)
                .wrap()
        })
}

fn buttons(review: &Review, p: &Palette) -> Vec<El> {
    let mut buttons = vec![
        button("Cancel", ButtonKind::Secondary, p)
            .key("mods/review/cancel")
            .on_press(actions::cancel_review),
        spacer(),
    ];
    // A plan that downloads nothing still records the collection it installs.
    if !review.plan.steps.is_empty() || !review.dropped.is_empty() {
        let words = if review.title.starts_with("Update") {
            "Update"
        } else {
            "Install"
        };
        buttons.push(
            button(words, ButtonKind::Primary, p)
                .key("mods/review/confirm")
                .on_press(actions::confirm_review),
        );
    }
    buttons
}
