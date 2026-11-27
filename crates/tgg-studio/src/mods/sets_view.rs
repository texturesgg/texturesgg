//! The Sets tab inside Mods: every set the player has (clicking one shows
//! its mods in the panel on the right, Use makes it active) and every curated
//! set textures.gg publishes (with its description, a preview of its mods,
//! and Add set).

use super::{Registry, actions, current_set, mod_count};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{ButtonKind, Tone, badge, button, button_small, list_row, skeleton};
use rune::{El, col, row, text as label};
use tgg_mod::Set;
use tgg_mod::sets::{Group, Line, MySet};

/// Width of the action column on the right of each set row (Add set / Added / Use / Using).
const ACTION_WIDTH: f32 = 92.0;
/// How many mod names to preview on an official set row before "+N more".
const PREVIEW_MODS: usize = 5;
/// Placeholder rows while textures.gg first answers.
const PLACEHOLDERS: usize = 3;

/// Number shown on the Sets tab: the player's visible sets plus official
/// textures.gg sets not yet added.
pub fn tab_count(app: &App) -> usize {
    let yours = visible_sets(app).count();
    let extra_official = app
        .mods
        .sets
        .iter()
        .filter(|set| local_for_official(app, &set.name).is_none())
        .count();
    yours + extra_official
}

/// The scrollable body of the Sets tab: Your sets, then From textures.gg.
pub fn view(app: &App, p: &Palette) -> El {
    let yours: Vec<&MySet> = visible_sets(app).collect();
    let shown = current_set(app).map(|set| set.key.as_str());

    let yours_section = col()
        .key("mods/sets/yours")
        .gap(space::XXS)
        .child(section("Your sets", p).px(space::SM))
        .child(if yours.is_empty() {
            label("No sets yet. Add one from textures.gg below, or make your own.")
                .key("mods/sets/yours/empty")
                .font(text::BODY)
                .color(p.muted)
                .wrap()
                .px(space::SM)
                .py(space::XS)
        } else {
            col().children(
                yours
                    .into_iter()
                    .map(|set| my_set_row(app, set, shown == Some(set.key.as_str()), p)),
            )
        });

    let loading = matches!(app.mods.registry, Registry::NotAsked | Registry::Asking)
        && app.mods.sets.is_empty();

    let official_section = col()
        .key("mods/sets/official")
        .pt(space::SM)
        .gap(space::XXS)
        .child(section("From textures.gg", p).px(space::SM))
        .child(if loading {
            col().children((0..PLACEHOLDERS).map(|_| {
                col()
                    .padding(space::SM)
                    .gap(space::XS)
                    .child(skeleton(0.3, p))
                    .child(skeleton(0.6, p))
            }))
        } else if app.mods.sets.is_empty() {
            label("textures.gg has no sets yet.")
                .key("mods/sets/official/empty")
                .font(text::BODY)
                .color(p.muted)
                .px(space::SM)
                .py(space::XS)
        } else {
            col().children(
                app.mods
                    .sets
                    .iter()
                    .map(|set| official_set_row(app, set, p)),
            )
        });

    col()
        .gap(space::SM)
        .child(yours_section)
        .child(official_section)
}

/// The player's sets that show on the page (hiding an empty Installed mods).
fn visible_sets(app: &App) -> impl Iterator<Item = &MySet> {
    app.sets
        .list
        .iter()
        .filter(|set| !set.is_installed() || !set.lines.is_empty())
}

/// The local locked set that follows textures.gg set `link`, if added.
fn local_for_official<'a>(app: &'a App, link: &str) -> Option<&'a MySet> {
    app.sets.list.iter().find(|have| {
        have.locked
            && have.lines.len() == 1
            && matches!(&have.lines[0], Line::Set { set } if set == link)
    })
}

/// One of the player's local sets: clicking selects it in the side panel,
/// Use activates it (or Using when active).
fn my_set_row(app: &App, set: &MySet, chosen: bool, p: &Palette) -> El {
    let key = set.key.clone();
    let pick_key = set.key.clone();
    let active = app.sets.active == set.key;
    let count = local_mod_count(app, set);
    let end = if active {
        row()
            .key(format!("mods/sets/mine/{key}/using"))
            .w(ACTION_WIDTH)
            .justify_end()
            .child(badge("Using", Tone::Mine, p))
    } else {
        let use_key = set.key.clone();
        row().w(ACTION_WIDTH).justify_end().child(
            button_small("Use", ButtonKind::Secondary, p)
                .key(format!("mods/sets/mine/{key}/use"))
                .on_press(move |app: &mut App| actions::use_set(app, &use_key)),
        )
    };
    let mut item = list_row(p)
        .key(format!("mods/sets/mine/{key}"))
        .label(set.name.clone())
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::choose_set(app, &pick_key));
    if chosen {
        item = item.fill(p.current);
    }
    item.child(
        col()
            .gap(space::TIGHT)
            .grow(1.0)
            .min_w(0.0)
            .child(
                row()
                    .gap(space::XS)
                    .items_center()
                    .child(
                        label(set.name.clone())
                            .font(text::STRONG)
                            .color(p.ink)
                            .truncate()
                            .min_w(0.0),
                    )
                    .children(count.map(|n| label(mod_count(n)).font(text::SMALL).color(p.faint))),
            )
            .child(
                label(super::panel::kind(app, set))
                    .font(text::SMALL)
                    .color(p.muted)
                    .truncate(),
            ),
    )
    .child(end)
}

/// An official set from textures.gg: title, mod count, description, mod
/// preview, and Add set (or Added when already in Your sets).
fn official_set_row(app: &App, set: &Set, p: &Palette) -> El {
    let added = local_for_official(app, &set.name);
    let preview = preview_mods(app, set);
    let end = if added.is_some() {
        row()
            .key(format!("mods/sets/official/{}/added", set.name))
            .w(ACTION_WIDTH)
            .justify_center()
            .child(label("Added").font(text::STRONG).color(p.faint))
    } else if app.mods.work.is_some() {
        row().w(ACTION_WIDTH)
    } else {
        let link = set.name.clone();
        button("Add set", ButtonKind::Secondary, p)
            .key(format!("mods/sets/official/{}/add", set.name))
            .w(ACTION_WIDTH)
            .on_press(move |app: &mut App| actions::add_set(app, &link))
    };
    let mut item = list_row(p)
        .key(format!("mods/sets/official/{}", set.name))
        .label(set.title.clone());
    if let Some(local) = added {
        let pick_key = local.key.clone();
        item = item
            .cursor(rune::Cursor::Pointer)
            .role("button")
            .focusable(true)
            .on_press(move |app: &mut App| actions::choose_set(app, &pick_key));
    }
    item.child(
        col()
            .gap(space::TIGHT)
            .grow(1.0)
            .min_w(0.0)
            .child(
                row()
                    .gap(space::XS)
                    .items_center()
                    .child(
                        label(set.title.clone())
                            .font(text::STRONG)
                            .color(p.ink)
                            .truncate()
                            .min_w(0.0),
                    )
                    .child(
                        label(mod_count(set.mods.len()))
                            .font(text::SMALL)
                            .color(p.faint),
                    ),
            )
            .children((!set.description.is_empty()).then(|| {
                label(set.description.clone())
                    .font(text::SMALL)
                    .color(p.muted)
                    .wrap()
            }))
            .children(
                (!preview.is_empty())
                    .then(|| label(preview).font(text::SMALL).color(p.faint).truncate()),
            ),
    )
    .child(end)
}

/// How many mods load in `set`, when known (from its plan, its official
/// source, or the mods folder).
fn local_mod_count(app: &App, set: &MySet) -> Option<usize> {
    if set.is_installed() {
        return Some(set.lines.len());
    }
    if let Some(plan) = app.sets.plan(&set.key) {
        let count = plan
            .mods
            .iter()
            .filter(|m| m.skipped.is_none() && m.group != Group::LeftOut)
            .count();
        return Some(count);
    }
    if set.locked
        && let Some(link) = set.builds_on().next()
        && let Some(official) = app.mods.sets.iter().find(|official| official.name == link)
    {
        return Some(official.mods.len());
    }
    None
}

/// A compact comma-separated preview of an official set's mods.
fn preview_mods(app: &App, set: &Set) -> String {
    if set.mods.is_empty() {
        return String::new();
    }
    let names: Vec<String> = set
        .mods
        .iter()
        .take(PREVIEW_MODS)
        .map(|m| app.mods.name_of(&m.id))
        .collect();
    let rest = set.mods.len().saturating_sub(PREVIEW_MODS);
    if rest > 0 {
        format!("{}, +{rest} more", names.join(", "))
    } else {
        names.join(", ")
    }
}

/// A section heading inside the Sets tab.
fn section(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .font(text::LABEL)
        .color(p.faint)
        .pt(space::XS)
}
