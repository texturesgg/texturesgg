//! The Mods page: every mod textures.gg offers, with the search, the order
//! and the filters over it, and the set showing as a panel at the right
//! that stays while the list scrolls. Pressing a mod's row shows its
//! details where the panel was; Add puts the mod in the set showing (or in
//! the mods folder, for Installed mods), and Added takes it back out.

use super::{Menu, Mods, Registry, Sort, actions, count_text, current_set, size_text};
use crate::nav::Place;
use crate::state::App;
use crate::ui::tokens::{Palette, size, space, text};
use crate::ui::{
    ButtonKind, Icon, button, button_done, button_off, callout, chip, icon, icon_tile, list_row,
    progress, search_box, search_field, select, select_item, select_menu, select_rule, skeleton,
    text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::sets::Group;
use tgg_mod::{CatalogEntry, ModId, ModSummary};

/// A mod's icon in the list.
const MOD_ICON: f32 = 40.0;
/// Add and Added, one width, so the column of them lines up.
const ADD_WIDTH: f32 = 92.0;
/// The order's select: narrower than a select's usual, beside the search.
const SORT_WIDTH: f32 = 168.0;
/// Placeholder rows while textures.gg first answers.
const PLACEHOLDERS: usize = 5;

pub fn view(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    // Wide, the set shows as a panel at the right, and a mod's details take
    // its place while they show. Narrow, the set is a bar over the list,
    // and the details slide in over the page instead.
    let wide = app.window_width <= 0.0 || app.side_by_side();
    let left = col()
        .grow(2.0)
        .min_w(0.0)
        .min_h(0.0)
        .gap(space::SM)
        .children(notices(app, p))
        .children(first_run(mods, p))
        .children((!wide).then(|| super::panel::bar(app, p)))
        .child(browse(app, p))
        .child(
            // Out past the search's edges by a row's inset, so a row's
            // words line up with the search and only its wash reaches out.
            col()
                .key("mods/scroll")
                .mx(-space::SM)
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(col().pb(space::XL).child(all(app, p))),
        );
    let side = wide.then(|| {
        if mods.selected.is_some() {
            super::details::column(app, p)
        } else {
            super::panel::view(app, p)
        }
    });
    row()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::LG)
        .gap(space::LG)
        .grow(1.0)
        .min_h(0.0)
        .child(left)
        .children(side)
}

/// The top bar's end on Mods: Start a mod, once there's a tgg-melee to make
/// one for.
pub fn top_bar(app: &App, p: &Palette) -> Vec<El> {
    app.mods
        .port()
        .map(|_| {
            button("Start a mod", ButtonKind::Secondary, p)
                .key("mods/new-mod")
                .on_press(crate::newmod::actions::open)
        })
        .into_iter()
        .collect()
}

/// Over the list: the search as wide as it, the order beside it, and the
/// filters under them.
fn browse(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let placeholder = match mods.total() {
        0 => "Search mods".to_owned(),
        1 => "Search 1 mod".to_owned(),
        total => format!("Search {total} mods"),
    };
    let search = search_box(
        search_field(&mods.search, &placeholder, p)
            .key("mods/search")
            .on_edit(|app: &mut App| &mut app.mods.search),
        p,
    )
    .border(size::LINE, p.line)
    .grow(1.0)
    .min_w(0.0);
    let sort = select(mods.sort.label(), mods.menu == Some(Menu::Sort), p)
        .key("mods/sort")
        .min_w(SORT_WIDTH)
        .label(format!("Sort by {}", mods.sort.label().to_lowercase()))
        .on_press(|app: &mut App| actions::toggle_menu(app, Menu::Sort));
    col()
        .gap(space::SM)
        .child(
            row()
                .gap(space::XS)
                .items_center()
                .child(search)
                .child(sort),
        )
        .child(filters(mods, p))
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

/// The filter chips: All, Rules, Training, Online modes, On screen.
fn filters(mods: &Mods, p: &Palette) -> El {
    row()
        .gap(space::SNUG)
        .flex_wrap()
        .children(super::Filter::ALL.map(|filter| {
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
            .py(space::SM)
    });
    let rows = found.into_iter().map(|entry| mod_row(app, entry, p));
    // What waits for a build, after what installs.
    let waiting = (!unbuilt.is_empty()).then(|| {
        col()
            .key("mods/all/unbuilt")
            .pt(space::SM)
            .gap(space::XXS)
            .child(section("Not built for this version yet", p).px(space::SM))
            .child(
                col().children(
                    unbuilt
                        .into_iter()
                        .map(|summary| unbuilt_row(app, summary, p)),
                ),
            )
    });
    col()
        .gap(space::SM)
        .children(
            note(mods, p)
                .filter(|_| mods.port().is_none())
                .map(|note| note.px(space::SM)),
        )
        .children(empty.map(|empty| empty.px(space::SM)))
        .child(
            col().children(
                rows.chain(
                    loading(mods)
                        .then(|| placeholder_rows(p))
                        .into_iter()
                        .flatten(),
                ),
            ),
        )
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
            .padding(space::SM)
            .gap(space::XS)
            .child(skeleton(0.3, p))
            .child(skeleton(0.6, p))
    })
}

/// A mod's name, and who publishes it unless that's textures.gg itself.
fn name_line(app: &App, id: &ModId, name: &str, p: &Palette) -> El {
    let by = app
        .mods
        .listings
        .get(id)
        .filter(|summary| !summary.publisher.official)
        .map(|summary| {
            label(format!("by {}", summary.publisher.name))
                .font(text::SMALL)
                .color(p.faint)
                .truncate()
                .min_w(0.0)
        });
    row()
        .gap(space::XS)
        .items_center()
        .child(
            label(name.to_owned())
                .font(text::STRONG)
                .color(p.ink)
                .truncate()
                .min_w(0.0),
        )
        .children(by)
}

/// How many times a mod's been installed, faint, with the download glyph.
fn installs(app: &App, id: &ModId, p: &Palette) -> Option<El> {
    let count = app.mods.listings.get(id)?.installs;
    (count > 0).then(|| {
        row()
            .gap(space::XXS)
            .items_center()
            .label(format!("Installed {count} times"))
            .help(format!("Installed {count} times"))
            .child(icon(Icon::Download, p.faint))
            .child(
                label(count_text(count))
                    .font(text::MONO_SMALL)
                    .color(p.faint),
            )
    })
}

/// A mod textures.gg offers: its icon, name, one line of what it does, how
/// often it's installed, and Add, or Added while the set showing has it.
fn mod_row(app: &App, entry: &CatalogEntry, p: &Palette) -> El {
    let manifest = &entry.manifest;
    let id = manifest.id.clone();
    let key = format!("mods/catalog/{id}");
    let shown = id.clone();
    let running = current_set(app).is_some_and(|set| app.sets.running(&set.key));
    let end: Option<El> = match in_set(app, &id) {
        Some(true) if !running => {
            let gone = id.clone();
            Some(
                button_done("Added", p)
                    .key(format!("{key}/remove"))
                    .w(ADD_WIDTH)
                    .label(format!("Take {} out of the set", manifest.name))
                    .on_press(move |app: &mut App| actions::take_away(app, &gone)),
            )
        }
        Some(_) => Some(
            row()
                .key(format!("{key}/added"))
                .w(ADD_WIDTH)
                .gap(space::XXS)
                .items_center()
                .justify_center()
                .child(icon(Icon::Check, p.faint))
                .child(label("Added").font(text::STRONG).color(p.faint)),
        ),
        None if running => Some(button_off("Add", p).key(format!("{key}/add")).w(ADD_WIDTH)),
        None if app.mods.port().is_none() || app.mods.work.is_some() => None,
        None => {
            let add = id.clone();
            Some(
                button("Add", ButtonKind::Secondary, p)
                    .key(format!("{key}/add"))
                    .w(ADD_WIDTH)
                    .on_press(move |app: &mut App| actions::add(app, &add)),
            )
        }
    };
    list_row(p)
        .key(key)
        .label(manifest.name.clone())
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::select(app, &shown))
        .child(icon_tile(app.mods.icon(&id), MOD_ICON, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(name_line(app, &id, &manifest.name, p))
                .children(manifest.description.as_ref().map(|line| {
                    label(line.clone())
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate()
                })),
        )
        .children(installs(app, &id, p))
        .children(end)
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
        .child(icon_tile(app.mods.icon(id), MOD_ICON, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(name_line(app, id, &summary.name, p))
                .children(summary.description.as_ref().map(|line| {
                    label(line.clone())
                        .font(text::SMALL)
                        .color(p.muted)
                        .truncate()
                })),
        )
        .children(installs(app, id, p))
}

/// Whether the set showing has the mod, and if so whether it can be taken
/// out from here: installed and on, for Installed mods; loading from its
/// plan, for the rest, where only a mod of its own or of a set it builds on
/// can be taken out of one of the player's.
fn in_set(app: &App, id: &ModId) -> Option<bool> {
    let set = current_set(app)?;
    if set.is_installed() {
        return app
            .mods
            .local
            .get(id)
            .is_some_and(|m| m.enabled)
            .then_some(true);
    }
    let plan = app.sets.plan(&set.key)?;
    let m = plan
        .mods
        .iter()
        .find(|m| m.id == *id && m.skipped.is_none() && m.group != Group::LeftOut)?;
    Some(!set.locked && matches!(m.group, Group::Added | Group::FromSet { .. }))
}

/// The page's open menu: which set shows, what else to do to it, or the
/// list's order.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    if app.nav.place != Place::Mods {
        return None;
    }
    let (anchor, items) = match app.mods.menu? {
        Menu::Sets => ("mods/set/pick", sets_menu(app, p)),
        Menu::More => ("mods/set/more", more_menu(p)),
        Menu::Sort => ("mods/sort", sort_menu(app, p)),
    };
    Some(select_menu(anchor, items, actions::close_menu, p))
}

/// The player's sets, choosing one shows it; then New set.
fn sets_menu(app: &App, p: &Palette) -> Vec<El> {
    let shown = current_set(app).map(|set| set.key.clone());
    let mut items: Vec<El> = app
        .sets
        .list
        .iter()
        .filter(|set| !set.is_installed() || !set.lines.is_empty())
        .map(|set| {
            let key = set.key.clone();
            select_item(&set.name, shown.as_deref() == Some(set.key.as_str()), p)
                .key(format!("mods/set/pick/{}", set.key))
                .on_press(move |app: &mut App| {
                    actions::close_menu(app);
                    actions::choose_set(app, &key);
                })
        })
        .collect();
    items.push(select_rule(p));
    items.push(
        select_item("New set", false, p)
            .key("mods/new-set")
            .on_press(|app: &mut App| {
                actions::close_menu(app);
                actions::open_new_set(app);
            }),
    );
    items
}

/// What else can be done to a set of the player's.
fn more_menu(p: &Palette) -> Vec<El> {
    vec![
        select_item("Rename", false, p)
            .key("mods/set/rename")
            .on_press(|app: &mut App| {
                actions::close_menu(app);
                actions::open_rename(app);
            }),
        select_item("Build on another set", false, p)
            .key("mods/set/build-on")
            .on_press(|app: &mut App| {
                actions::close_menu(app);
                actions::open_build_on(app);
            }),
        select_rule(p),
        select_item("Delete set", false, p)
            .key("mods/set/delete")
            .on_press(|app: &mut App| {
                actions::close_menu(app);
                if let Some(key) = current_set(app).map(|set| set.key.clone()) {
                    actions::delete_set(app, &key);
                }
            }),
    ]
}

/// The orders the list can show in.
fn sort_menu(app: &App, p: &Palette) -> Vec<El> {
    Sort::ALL
        .into_iter()
        .map(|sort| {
            select_item(sort.label(), app.mods.sort == sort, p)
                .key(format!("mods/sort/{}", sort.label()))
                .on_press(move |app: &mut App| actions::set_sort(app, sort))
        })
        .collect()
}
