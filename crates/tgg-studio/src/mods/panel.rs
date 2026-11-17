//! The set the page shows, on the right: its name and Use, its mods in the
//! groups its plan gives (from each set it builds on, added, left out,
//! skipped with the reason), and what can be done to it. Installed mods are
//! the mods folder, so its rows carry the switches instead. A game running
//! from the set makes it read-only.

use super::{actions, current_set};
use crate::sets::Planned;
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Tone, badge, button_small, icon_tile, rule, split_empty, switch, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::sets::{Group, SetMod};

/// A mod's icon in a row here: smaller than a thumbnail, as the panel's
/// rows are close together.
const ICON: f32 = 22.0;

/// The set showing, as a panel on the page's right.
pub fn view(app: &App, p: &Palette) -> El {
    let Some(set) = current_set(app) else {
        return crate::ui::split_details(
            split_empty("Choose a set to see its mods.", p).key("mods/set/none"),
            p,
        )
        .key("mods/set");
    };
    let running = app.sets.running(&set.key);
    let using = app.sets.active == set.key;
    let head = row()
        .gap(space::SM)
        .items_center()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(
                    label(set.name.clone())
                        .key("mods/set/name")
                        .font(text::H3)
                        .color(p.ink)
                        .wrap(),
                )
                .child(label(kind(set)).font(text::SMALL).color(p.muted).wrap()),
        )
        .child(if using {
            badge("Using", Tone::Mine, p)
        } else {
            let key = set.key.clone();
            button_small("Use", ButtonKind::Primary, p)
                .key("mods/set/use")
                .on_press(move |app: &mut App| actions::use_set(app, &key))
        });
    let mut body = col()
        .key("mods/set/body")
        .px(space::MD)
        .py(space::SM)
        .gap(space::MD)
        .child(head)
        .children(running.then(|| {
            label(format!(
                "{} is running. Changes apply next time you play.",
                set.name
            ))
            .key("mods/set/running")
            .font(text::SMALL)
            .color(p.muted)
            .wrap()
        }))
        .children(groups(app, &set.key, running, p));
    if let Some(footer) = footer(app, &set.key, running, p) {
        body = body.child(footer);
    }
    col()
        .key("mods/set")
        .grow(1.0)
        .min_w(size::DETAILS_MIN)
        .max_w(size::DETAILS_MAX)
        .min_h(0.0)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
        .clip_rounded(radius::CARD)
        .child(
            col()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

/// What the set is, under its name.
fn kind(set: &tgg_mod::sets::MySet) -> String {
    if set.is_installed() {
        "Yours. The mods in your mods folder.".to_owned()
    } else if set.locked {
        "By textures.gg, and kept up to date. To change it, make your own.".to_owned()
    } else {
        "Yours. Add mods from the list.".to_owned()
    }
}

/// Its mods, group by group.
fn groups(app: &App, key: &str, running: bool, p: &Palette) -> Vec<El> {
    let Some(set) = app.sets.get(key) else {
        return Vec::new();
    };
    if set.is_installed() {
        return installed(app, key, running, p);
    }
    match app.sets.plans.get(key) {
        Some(Planned::Ready(plan)) => {
            let mut blocks: Vec<(Group, Vec<&SetMod>)> = Vec::new();
            for m in plan.mods.iter().filter(|m| m.skipped.is_none()) {
                match blocks.last_mut() {
                    Some((group, rows)) if *group == m.group => rows.push(m),
                    _ => blocks.push((m.group.clone(), vec![m])),
                }
            }
            let mut out: Vec<El> = blocks
                .into_iter()
                .flat_map(|(group, rows)| {
                    let title = format!("{} ({})", title(&group), rows.len());
                    std::iter::once(section(&title, p))
                        .chain(rows.into_iter().map(|m| row_for(app, m, running, p)))
                })
                .collect();
            let skipped: Vec<&SetMod> = plan.skipped().collect();
            if !skipped.is_empty() {
                out.push(section(&format!("Skipped ({})", skipped.len()), p));
                out.extend(skipped.into_iter().map(|m| skipped_row(app, m, p)));
            }
            out
        }
        Some(Planned::Failed(why)) => vec![
            label(why.clone())
                .key("mods/set/failed")
                .font(text::SMALL)
                .color(p.danger)
                .wrap(),
        ],
        Some(Planned::Working) | None => vec![
            label("Working out its mods…")
                .key("mods/set/working")
                .font(text::SMALL)
                .color(p.faint),
        ],
    }
}

/// A group's words, as the panel's rows head it.
fn title(group: &Group) -> String {
    match group {
        Group::FromSet { title, .. } => format!("From {title}"),
        Group::Added => "Added".to_owned(),
        Group::LeftOut => "Left out".to_owned(),
        Group::Skins => "Skins".to_owned(),
        Group::Needed => "Needed".to_owned(),
    }
}

/// A section's label, tight over its rows.
fn section(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .font(text::LABEL)
        .color(p.faint)
        .pt(space::XS)
}

/// A mod of a set, and what the player can do with its line.
fn row_for(app: &App, m: &SetMod, running: bool, p: &Palette) -> El {
    let id = &m.id;
    let left_out = m.group == Group::LeftOut;
    let end = if running {
        None
    } else {
        match &m.group {
            Group::LeftOut => Some(
                text_action("Put back", false, p)
                    .key(format!("mods/set/{id}/put-back"))
                    .on_press({
                        let id = id.clone();
                        move |app: &mut App| actions::leave_out(app, &id, false)
                    }),
            ),
            Group::Added => Some(
                text_action("Take out", false, p)
                    .key(format!("mods/set/{id}/take-out"))
                    .on_press({
                        let id = id.clone();
                        move |app: &mut App| actions::take_out(app, &id)
                    }),
            ),
            Group::FromSet { .. } => Some(
                text_action("Leave out", false, p)
                    .key(format!("mods/set/{id}/leave-out"))
                    .on_press({
                        let id = id.clone();
                        move |app: &mut App| actions::leave_out(app, &id, true)
                    }),
            ),
            Group::Skins | Group::Needed => None,
        }
    };
    row()
        .key(format!("mods/set/{id}"))
        .gap(space::XS)
        .items_center()
        .child(icon_tile(app.mods.icon(id), ICON, p))
        .child(
            label(app.mods.name_of(id))
                .font(text::BODY)
                .color(if left_out { p.faint } else { p.ink })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
        .children(end)
}

/// Installed mods: the mods folder, each with the switch that turns it on
/// or off and the update it has.
fn installed(app: &App, key: &str, running: bool, p: &Palette) -> Vec<El> {
    let Some(set) = app.sets.get(key) else {
        return Vec::new();
    };
    let mut out = vec![section(
        &format!("In your mods folder ({})", set.lines.len()),
        p,
    )];
    for line in &set.lines {
        let tgg_mod::sets::Line::Mod(line) = line else {
            continue;
        };
        let id = &line.id;
        let on = !line.left_out;
        let update = app.mods.local.get(id).and_then(|m| m.update.clone());
        let end: Vec<El> = if running {
            vec![
                label(if on { "On" } else { "Off" })
                    .font(text::SMALL)
                    .color(p.muted),
            ]
        } else {
            let update = update.filter(|_| app.mods.work.is_none()).map(|version| {
                let id = id.clone();
                text_action(&format!("Update to {version}"), false, p)
                    .key(format!("mods/set/{id}/update"))
                    .on_press(move |app: &mut App| {
                        crate::mods::actions::review_mod_update(app, &id);
                    })
            });
            let toggled = id.clone();
            vec![
                switch(&format!("mods/set/{id}/switch"), on, p)
                    .label(app.mods.name_of(id))
                    .on_press(move |app: &mut App| actions::set_on(app, &toggled, !on)),
            ]
            .into_iter()
            .chain(update)
            .collect()
        };
        out.push(
            row()
                .key(format!("mods/set/{id}"))
                .gap(space::XS)
                .items_center()
                .child(icon_tile(app.mods.icon(id), ICON, p))
                .child(
                    label(app.mods.name_of(id))
                        .font(text::BODY)
                        .color(if on { p.ink } else { p.faint })
                        .truncate()
                        .grow(1.0)
                        .min_w(0.0),
                )
                .children(end),
        );
    }
    out
}

/// A mod the set skips, and why.
fn skipped_row(app: &App, m: &SetMod, p: &Palette) -> El {
    let id = &m.id;
    row()
        .key(format!("mods/set/{id}/skipped"))
        .gap(space::XS)
        .items_center()
        .child(icon_tile(app.mods.icon(id), ICON, p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(
                    label(app.mods.name_of(id))
                        .font(text::BODY)
                        .color(p.muted)
                        .truncate(),
                )
                .child(
                    label(m.skipped.clone().unwrap_or_default())
                        .font(text::SMALL)
                        .color(p.danger)
                        .wrap(),
                ),
        )
}

/// What can be done to the set: making one's own from a locked, textures.gg
/// one, or renaming, deleting and building on another for the player's.
fn footer(app: &App, key: &str, running: bool, p: &Palette) -> Option<El> {
    let set = app.sets.get(key)?;
    let items: Vec<El> = if running {
        return None;
    } else if set.locked {
        let key = set.key.clone();
        vec![
            button_small("Make your own", ButtonKind::Secondary, p)
                .key("mods/set/make-own")
                .on_press(move |app: &mut App| actions::make_own(app, &key)),
        ]
    } else if set.is_installed() {
        return None;
    } else {
        let key = set.key.clone();
        vec![
            text_action("Rename", false, p)
                .key("mods/set/rename")
                .on_press(actions::open_rename),
            text_action("Delete", true, p)
                .key("mods/set/delete")
                .on_press(move |app: &mut App| actions::delete_set(app, &key)),
            text_action("Build on another set", false, p)
                .key("mods/set/build-on")
                .on_press(actions::open_build_on),
        ]
    };
    Some(
        col().key("mods/set/actions").child(rule(p)).child(
            row()
                .px(space::MD)
                .py(space::XS)
                .gap(space::XS)
                .flex_wrap()
                .children(items),
        ),
    )
}
