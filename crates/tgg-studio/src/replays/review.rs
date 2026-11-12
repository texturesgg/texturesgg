//! The review before a replay plays with mods of its own: the tgg-melee it
//! needs, the mods it gets (who publishes each, and which ask the player's
//! yes), and what can't be had, with whether that likely matters. Nothing
//! is installed into the player's own mods. It's modal: Escape or a click
//! outside cancels.

use super::{Status, actions};
use crate::mods::size_text;
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, Tone, badge, button, dialog, rule, spacer, switch,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::GameNeed;

/// The list's tallest, in points; longer lists scroll.
const LIST_HEIGHT: f32 = 360.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    let review = app.replays.review.as_ref()?;
    let plan = &review.plan;
    let mut sections: Vec<El> = Vec::new();
    match &plan.game {
        GameNeed::Installed(port) if !port.current => sections.push(note(
            format!(
                "Plays on tgg-melee {}, installed beside the one you use.",
                port.version
            ),
            p,
        )),
        GameNeed::Available { version, size } => sections.push(note(
            match size {
                Some(size) => format!(
                    "Plays on tgg-melee {version}, installed beside the one you use ({}).",
                    size_text(*size)
                ),
                None => format!("Plays on tgg-melee {version}, installed beside the one you use."),
            },
            p,
        )),
        _ => {}
    }
    if !plan.mods.is_empty() {
        let rows = plan.mods.iter().map(|m| {
            let size = (!m.cached).then(|| {
                label(size_text(m.size))
                    .font(text::MONO_SMALL)
                    .color(p.faint)
            });
            let who = if m.publisher.official {
                badge("Official", Tone::News, p)
            } else {
                badge(format!("by {}", m.publisher.name), Tone::Plain, p)
            };
            row()
                .key(format!("replays/review/mod/{}", m.id))
                .px(space::LG)
                .py(space::XS)
                .gap(space::SM)
                .items_center()
                .child(
                    row()
                        .gap(space::XS)
                        .items_end()
                        .grow(1.0)
                        .min_w(0.0)
                        .child(
                            label(m.name.clone())
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate(),
                        )
                        .child(
                            label(m.version.to_string())
                                .font(text::MONO_SMALL)
                                .color(p.faint),
                        ),
                )
                .children(size)
                .child(who)
        });
        sections.push(section("Mods it plays with".into(), rows.collect(), p));
    }
    let untrusted = plan.untrusted();
    if !untrusted.is_empty() {
        let remember = review.remember;
        sections.push(
            col()
                .key("replays/review/trust")
                .px(space::LG)
                .py(space::SM)
                .gap(space::XS)
                .child(
                    label(format!(
                        "Some of its mods aren't official: by {}. They run only while the replay plays.",
                        untrusted.join(", ")
                    ))
                    .font(text::SMALL)
                    .color(p.muted)
                    .wrap(),
                )
                .child(
                    row()
                        .gap(space::XS)
                        .items_center()
                        .child(
                            switch("replays/review/remember", remember, p)
                                .label("Trust them from now on")
                                .on_press(move |app: &mut App| actions::set_remember(app, !remember)),
                        )
                        .child(label("Trust them from now on").font(text::SMALL).color(p.muted)),
                ),
        );
    }
    if let Some(minor_only) = Status::of_missing(plan) {
        let lead = if minor_only {
            "A few of the codes its console ran have no mod, but they rarely change a replay: it likely plays as it was."
        } else {
            "Some of what its game ran can't be had, so it may not play as it was:"
        };
        let rows = plan
            .missing
            .iter()
            .filter(|missing| !minor_only || !missing.is_minor())
            .map(|missing| {
                label(capitalized(&missing.to_string()))
                    .font(text::SMALL)
                    .color(p.danger)
                    .px(space::LG)
                    .wrap()
            });
        sections.push(
            col()
                .key("replays/review/missing")
                .py(space::SM)
                .gap(space::XXS)
                .child(
                    label(lead)
                        .font(text::SMALL)
                        .color(p.muted)
                        .px(space::LG)
                        .wrap(),
                )
                .children(rows),
        );
    }
    let list = col()
        .max_h(LIST_HEIGHT)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col().children(
                sections
                    .into_iter()
                    .enumerate()
                    .flat_map(|(index, section)| {
                        (index > 0).then(|| rule(p)).into_iter().chain([section])
                    }),
            ),
        );
    let fetch =
        plan.mods.iter().any(|m| !m.cached) || matches!(plan.game, GameNeed::Available { .. });
    let line = if fetch {
        let size = plan.size()
            + if let GameNeed::Available {
                size: Some(size), ..
            } = plan.game
            {
                size
            } else {
                0
            };
        format!(
            "It plays with the mods its game had, in a setup of its own: your mods stay as they are. {} to download.",
            size_text(size)
        )
    } else {
        "It plays with the mods its game had, in a setup of its own: your mods stay as they are."
            .to_owned()
    };
    let parts = Dialog {
        title: "Watch this replay".into(),
        line,
        body: Some(list),
        buttons: vec![
            button("Cancel", ButtonKind::Secondary, p)
                .key("replays/review/cancel")
                .on_press(actions::cancel_review),
            spacer(),
            button(
                if fetch { "Get and watch" } else { "Watch" },
                ButtonKind::Primary,
                p,
            )
            .key("replays/review/play")
            .on_press(actions::confirm),
        ],
    };
    Some(dialog(
        "replays/review",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::cancel_review,
        p,
    ))
}

/// A line about the plan.
fn note(words: String, p: &Palette) -> El {
    label(words)
        .font(text::SMALL)
        .color(p.muted)
        .px(space::LG)
        .py(space::SM)
        .wrap()
}

/// A part of the list: its label over its rows.
fn section(name: String, rows: Vec<El>, p: &Palette) -> El {
    col()
        .pt(space::XS)
        .pb(space::XS)
        .child(
            label(name)
                .font(text::LABEL)
                .color(p.faint)
                .px(space::LG)
                .py(space::XXS),
        )
        .children(rows)
}

fn capitalized(line: &str) -> String {
    let mut characters = line.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}
