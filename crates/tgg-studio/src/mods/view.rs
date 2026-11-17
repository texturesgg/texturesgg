//! The Mods page: the player's sets as cards across the top with New set,
//! then every mod textures.gg offers with the filters over it, and the set
//! showing as a panel at the right that stays while the list scrolls.
//! Pressing a mod's row shows its details where the panel was, and Add puts
//! the mod in the set showing (or in the mods folder, for Installed mods).

use super::{Filter, Mods, Registry, actions, current_set, mod_count, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Icon, Tone, badge, button, button_off, callout, chip, icon, icon_tile, list_row,
    progress, rule, search_box, search_field, skeleton, spacer, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use std::collections::BTreeSet;
use tgg_mod::sets::{Group, Line, MySet};
use tgg_mod::{CatalogEntry, ModId, ModSummary};

/// A set card's width.
const SET_CARD: f32 = 152.0;
/// A set card's height: its name, its one line, and who made it.
const SET_CARD_HEIGHT: f32 = 128.0;
/// Placeholder rows while textures.gg first answers.
const PLACEHOLDERS: usize = 5;

pub fn view(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let notices: Vec<El> = notices(app, p);
    let left = col().grow(2.0).min_w(0.0).min_h(0.0).child(
        col()
            .key("mods/scroll")
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(
                col()
                    .pb(space::XL)
                    .gap(space::SM)
                    .children(notices)
                    .children(first_run(mods, p))
                    .child(section("Sets", p))
                    .child(cards(app, p))
                    .child(
                        col()
                            .pt(space::MD)
                            .gap(space::XXS)
                            .child(section("All mods", p))
                            .child(filters(mods, p))
                            .child(all(app, p)),
                    ),
            ),
    );
    // A mod's details take the panel's place while they show, wide enough
    // for two columns; narrow, they slide in over the page instead.
    let wide = app.window_width <= 0.0 || app.side_by_side();
    let side = if wide && mods.selected.is_some() {
        super::details::column(app, p)
    } else {
        super::panel::view(app, p)
    };
    row()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::LG)
        .gap(space::LG)
        .grow(1.0)
        .min_h(0.0)
        .child(left)
        .child(side)
}

/// The top bar's end on Mods: the search, and Start a mod once there's a
/// tgg-melee to make one for.
pub fn top_bar(app: &App, p: &Palette) -> Vec<El> {
    let search = search_box(
        search_field(&app.mods.search, "Search mods", p)
            .key("mods/search")
            .on_edit(|app: &mut App| &mut app.mods.search),
        p,
    );
    let start = app.mods.port().is_some().then(|| {
        button("Start a mod", ButtonKind::Secondary, p)
            .key("mods/new-mod")
            .on_press(crate::newmod::actions::open)
    });
    std::iter::once(search).chain(start).collect()
}

/// A section's label, over what it heads.
fn section(words: &str, p: &Palette) -> El {
    label(words.to_owned())
        .font(text::LABEL)
        .color(p.faint)
        .pt(space::XS)
}

/// What needs the player, each in a slim row: textures.gg unreachable,
/// what's installed unreadable, an install under way.
fn notices(app: &App, p: &Palette) -> Vec<El> {
    let mods = &app.mods;
    let mut notices = Vec::new();
    let say = |words: String, danger: bool| {
        label(words)
            .font(text::SMALL)
            .color(if danger { p.danger } else { p.muted })
            .wrap()
    };
    match &mods.registry {
        Registry::Offline => notices.push(
            callout(
                say(
                    "Can't reach textures.gg. What's installed still works.".into(),
                    true,
                ),
                None,
                p,
            )
            .key("mods/registry"),
        ),
        Registry::Failed(why) => {
            notices.push(callout(say(why.clone(), true), None, p).key("mods/registry"))
        }
        Registry::NotAsked | Registry::Asking | Registry::Answered => {}
    }
    if let Some(problem) = &mods.problem {
        notices.push(callout(say(problem.clone(), true), None, p).key("mods/problem"));
    }
    // A one-mod update shows its progress in that mod's row instead.
    if let Some(work) = mods.work.as_ref().filter(|work| work.target.is_none()) {
        notices.push(working(work, p));
    }
    notices
}

/// Before tgg-melee is installed: what it is, and installing it.
fn first_run(mods: &Mods, p: &Palette) -> Option<El> {
    if mods.port().is_some() || mods.work.is_some() {
        return None;
    }
    let install = mods.releases.first().map(|release| {
        let words = format!(
            "Install tgg-melee {} · {}",
            release.version,
            size_text(release.size())
        );
        row().child(
            button(&words, ButtonKind::Primary, p)
                .key("mods/install-port")
                .on_press(actions::install_port),
        )
    });
    Some(
        callout(
            col()
                .gap(space::TIGHT)
                .child(
                    label("Mods run in tgg-melee.")
                        .font(text::STRONG)
                        .color(p.ink),
                )
                .child(
                    label("Melee, rebuilt to run on your computer with a mod loader built in.")
                        .font(text::SMALL)
                        .color(p.muted)
                        .wrap(),
                ),
            install,
            p,
        )
        .key("mods/first-run"),
    )
}

/// An install's step and progress, and stopping it.
fn working(work: &super::Work, p: &Palette) -> El {
    let words = col()
        .gap(space::XS)
        .child(
            label(work.label())
                .font(text::SMALL)
                .color(p.muted)
                .truncate(),
        )
        .child(progress(work.fraction(), p));
    callout(
        words,
        Some(
            text_action("Stop", false, p)
                .key("mods/stop-work")
                .on_press(actions::cancel_work),
        ),
        p,
    )
    .key("mods/work")
}

/// The player's sets, Installed mods among them once it has any, then New
/// set.
fn cards(app: &App, p: &Palette) -> El {
    let shown = current_set(app).map(|set| set.key.clone());
    let cards = app
        .sets
        .list
        .iter()
        .filter(|set| !set.is_installed() || !set.lines.is_empty())
        .map(|set| card(app, set, shown.as_deref() == Some(set.key.as_str()), p));
    row()
        .flex_wrap()
        .gap(space::SM)
        .children(cards)
        .child(new_tile(p))
}

/// A set: its name, one line of what it is, who made it, how many mods, and
/// Using while it's the active one.
fn card(app: &App, set: &MySet, chosen: bool, p: &Palette) -> El {
    let key = set.key.clone();
    col()
        .key(format!("mods/set-card/{}", set.key))
        .w(SET_CARD)
        .min_h(SET_CARD_HEIGHT)
        .padding(space::SM)
        .gap(space::XXS)
        .fill(p.surface)
        .border(size::LINE, if chosen { p.line_strong } else { p.line })
        .radius(radius::CARD)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .label(set.name.clone())
        .on_press(move |app: &mut App| actions::choose_set(app, &key))
        .child(
            label(set.name.clone())
                .font(text::STRONG)
                .color(p.ink)
                .truncate(),
        )
        .child(
            label(what(app, set))
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        )
        .child(spacer())
        .child(
            row()
                .gap(space::XS)
                .items_end()
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(space::TIGHT)
                        .child(
                            label(if set.locked {
                                "By textures.gg"
                            } else {
                                "Yours"
                            })
                            .font(text::SMALL)
                            .color(p.faint)
                            .truncate(),
                        )
                        .child(
                            label(mod_count(count_of(app, set)))
                                .font(text::MONO_SMALL)
                                .color(p.faint),
                        ),
                )
                .children((app.sets.active == set.key).then(|| badge("Using", Tone::Mine, p))),
        )
}

/// New set, at the end of the cards.
fn new_tile(p: &Palette) -> El {
    col()
        .key("mods/new-set")
        .w(SET_CARD)
        .min_h(SET_CARD_HEIGHT)
        .center()
        .gap(space::XXS)
        .border(size::LINE, p.line_strong)
        .radius(radius::CARD)
        .hover(p.fill)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .label("New set")
        .on_press(actions::open_new_set)
        .child(icon(Icon::Plus, p.muted))
        .child(label("New set").font(text::SMALL).color(p.muted))
}

/// One line saying what the set is: a textures.gg set's description, or
/// what a set of the player's builds on and adds.
fn what(app: &App, set: &MySet) -> String {
    if set.is_installed() {
        return "Your mods folder.".to_owned();
    }
    if set.locked {
        let Some(link) = set.builds_on().next() else {
            return "Kept up to date by textures.gg.".to_owned();
        };
        let described = app
            .mods
            .sets
            .iter()
            .find(|published| published.name == link)
            .map(|published| published.description.trim().to_owned())
            .unwrap_or_default();
        if described.is_empty() {
            "Kept up to date by textures.gg.".to_owned()
        } else {
            described
        }
    } else {
        let on: Vec<String> = set
            .builds_on()
            .map(|link| {
                app.mods
                    .sets
                    .iter()
                    .find(|published| published.name == link)
                    .map_or_else(|| link.to_owned(), |published| published.title.clone())
            })
            .collect();
        let added = set
            .lines
            .iter()
            .filter(|line| matches!(line, Line::Mod(m) if !m.left_out))
            .count();
        match (on.is_empty(), added) {
            (true, _) => "Its own mods.".to_owned(),
            (false, 0) => on.join(" and "),
            (false, added) => format!("{} + {}", on.join(" and "), mod_count(added)),
        }
    }
}

/// How many mods the set lists: each set it builds on's, minus the ones it
/// leaves out, plus its own.
fn count_of(app: &App, set: &MySet) -> usize {
    if set.is_installed() {
        return set
            .lines
            .iter()
            .filter(|line| matches!(line, Line::Mod(m) if !m.left_out))
            .count();
    }
    let mut ids: BTreeSet<ModId> = BTreeSet::new();
    for link in set.builds_on() {
        if let Some(published) = app
            .mods
            .sets
            .iter()
            .find(|published| published.name == link)
        {
            ids.extend(published.mods.iter().map(|m| m.id.clone()));
        }
    }
    for line in &set.lines {
        if let Line::Mod(m) = line {
            if m.left_out {
                ids.remove(&m.id);
            } else {
                ids.insert(m.id.clone());
            }
        }
    }
    ids.len()
}

/// The filter chips: All, Rules, Training, Online modes, On screen.
fn filters(mods: &Mods, p: &Palette) -> El {
    row().gap(space::SNUG).children(Filter::ALL.map(|filter| {
        chip(filter.label(), mods.filter == filter, p)
            .key(format!("mods/filter/{}", filter.label()))
            .role("radio")
            .checked(mods.filter == filter)
            .on_press(move |app: &mut App| actions::set_filter(app, filter))
    }))
}

/// Every mod textures.gg has built for the game, newest release of each.
fn all(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let found = mods.found();
    let unbuilt = mods.found_unbuilt();
    let empty = (found.is_empty() && unbuilt.is_empty() && !loading(mods)).then(|| {
        let words = if mods.offered().is_empty() && mods.listings.is_empty() {
            "textures.gg has no mods for this tgg-melee yet."
        } else {
            "No mod matches that."
        };
        label(words)
            .key("mods/all/empty")
            .font(text::BODY)
            .color(p.muted)
    });
    let rows = found.into_iter().map(|entry| mod_row(app, entry, p));
    // What waits for a build, after what installs.
    let waiting = (!unbuilt.is_empty()).then(|| {
        col()
            .key("mods/all/unbuilt")
            .pt(space::SM)
            .gap(space::XXS)
            .child(section("Not built for this version yet", p))
            .child(list(
                unbuilt
                    .into_iter()
                    .map(|summary| unbuilt_row(app, summary, p)),
                p,
            ))
    });
    col()
        .gap(space::SM)
        .children(note(mods, p).filter(|_| mods.port().is_none()))
        .children(empty)
        .child(list(
            rows.chain(
                loading(mods)
                    .then(|| placeholder_rows(p))
                    .into_iter()
                    .flatten(),
            ),
            p,
        ))
        .children(waiting)
}

/// Whether textures.gg hasn't answered yet this session.
fn loading(mods: &Mods) -> bool {
    matches!(mods.registry, Registry::NotAsked | Registry::Asking)
        && mods.sets.is_empty()
        && mods.catalog.is_none()
}

/// Why the list can't install yet, or has nothing.
fn note(mods: &Mods, p: &Palette) -> Option<El> {
    let note = if mods.port().is_none() {
        Some("Install tgg-melee first; then mods install in one step.".to_owned())
    } else {
        mods.no_mods.clone()
    };
    note.map(|note| {
        label(note)
            .key("mods/note")
            .font(text::SMALL)
            .color(p.muted)
    })
}

/// Placeholder rows while textures.gg first answers.
fn placeholder_rows(p: &Palette) -> impl Iterator<Item = El> + '_ {
    (0..PLACEHOLDERS).map(move |_| {
        col()
            .py(space::SM)
            .gap(space::XS)
            .child(skeleton(0.3, p))
            .child(skeleton(0.6, p))
    })
}

/// Rows between rules.
fn list(rows: impl Iterator<Item = El>, p: &Palette) -> El {
    col()
        .child(rule(p))
        .children(rows.flat_map(|row| [row, rule(p)]))
}

/// A mod textures.gg offers: its icon, name, whether it has to match in
/// netplay, one line of what it does, and Add, or In it while the set
/// showing has it.
fn mod_row(app: &App, entry: &CatalogEntry, p: &Palette) -> El {
    let manifest = &entry.manifest;
    let id = manifest.id.clone();
    let key = format!("mods/catalog/{id}");
    let shown = id.clone();
    let chosen = shows(app, &id);
    let running = current_set(app).is_some_and(|set| app.sets.running(&set.key));
    let end: Option<El> = if chosen {
        Some(badge("In it", Tone::Mine, p))
    } else if running {
        Some(button_off("Add", p).key(format!("{key}/add")))
    } else if app.mods.port().is_none() || app.mods.work.is_some() {
        None
    } else {
        let add = id.clone();
        Some(
            button("Add", ButtonKind::Secondary, p)
                .key(format!("{key}/add"))
                .on_press(move |app: &mut App| actions::add(app, &add)),
        )
    };
    let mut line = list_row(p)
        .key(key)
        .label(manifest.name.clone())
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::select(app, &shown))
        .child(icon_tile(app.mods.icon(&id), size::THUMBNAIL, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    row()
                        .gap(space::XS)
                        .items_center()
                        .child(
                            label(manifest.name.clone())
                                .font(text::STRONG)
                                .color(p.ink)
                                .truncate(),
                        )
                        .children(
                            entry
                                .netplay
                                .counts()
                                .then(|| badge("Has to match in netplay", Tone::Mine, p)),
                        ),
                )
                .children(manifest.description.as_ref().map(|line| {
                    label(line.clone())
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate()
                })),
        );
    if let Some(end) = end {
        line = line.child(end);
    }
    line
}

/// A mod the registry lists that isn't built for this tgg-melee yet.
fn unbuilt_row(app: &App, summary: &ModSummary, p: &Palette) -> El {
    let id = &summary.id;
    let shown = id.clone();
    list_row(p)
        .key(format!("mods/listed/{id}"))
        .label(summary.name.clone())
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::select(app, &shown))
        .child(icon_tile(app.mods.icon(id), size::THUMBNAIL, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    label(summary.name.clone())
                        .font(text::STRONG)
                        .color(p.ink)
                        .truncate(),
                )
                .children(summary.description.as_ref().map(|line| {
                    label(line.clone())
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate()
                })),
        )
        .child(badge("Not built for this version yet", Tone::Plain, p))
}

/// Whether the set showing has the mod: installed and on, for Installed
/// mods; loading from its plan, for the rest.
fn shows(app: &App, id: &ModId) -> bool {
    let Some(set) = current_set(app) else {
        return false;
    };
    if set.is_installed() {
        return app.mods.local.get(id).is_some_and(|m| m.enabled);
    }
    app.sets.plan(&set.key).is_some_and(|plan| {
        plan.mods
            .iter()
            .any(|m| m.id == *id && m.skipped.is_none() && m.group != Group::LeftOut)
    })
}
