//! The Mods page: its head (with tgg-melee's version and the disc it plays
//! at its side), a notice when something needs the player, then the
//! collections, every mod, or what's installed, a tab each; a mod's details
//! slide in at the window's right.

use super::{Disc, Mods, Registry, Tab, actions, count_text, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, Tone, badge, button, button_off, callout, icon_tile, page_head,
    panel, progress, rule, search_field, skeleton, spacer, split, split_list, split_page, status,
    switch, tab, text_action,
};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::{CatalogEntry, LocalMod, ModId, ModSummary, Set, SetStatus};

/// A collection's card: wide enough for its description to read as a line
/// or two.
const SET_CARD: f32 = 300.0;
/// The notices and the lists read best no wider than this.
const COLUMN: f32 = 720.0;
/// The head's side, where tgg-melee's version and disc are, at its widest.
const HEAD_SIDE: f32 = 420.0;
/// Placeholder cards and rows while textures.gg first answers.
const PLACEHOLDERS: usize = 4;

pub fn view(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let head = row()
        .items_end()
        .gap(space::LG)
        .child(col().grow(1.0).min_w(0.0).child(page_head(
            "Mods",
            "From textures.gg, for tgg-melee: Melee with mods built in.",
            p,
        )))
        .child(head_side(app, p));
    let list = match mods.tab {
        Tab::Sets => sets(mods, p),
        Tab::All => all(mods, p),
        Tab::Installed => installed(mods, p),
    };
    let notices: Vec<El> = notices(app, p);
    // Wide, a list and its details sit side by side, each scrolling on its
    // own under the head and tabs, which stay.
    let top = top(mods, p);
    if app.side_by_side() && mods.tab != Tab::Sets {
        let lists = split(
            split_list("mods/scroll", top, list),
            super::details::column(app, p),
        );
        let page = split_page()
            .child(head)
            .children(
                (!notices.is_empty()).then(|| col().max_w(COLUMN).gap(space::XS).children(notices)),
            )
            .child(
                col()
                    .gap(space::MD)
                    .grow(1.0)
                    .min_h(0.0)
                    .child(col().child(tabs(mods, p)).child(rule(p)))
                    .child(lists),
            );
        return col().grow(1.0).min_h(0.0).child(page);
    }
    // Narrow, a list reads best no wider than the column; collections'
    // cards wrap to the page.
    let list = if mods.tab == Tab::Sets {
        list
    } else {
        col().max_w(COLUMN).gap(space::SM).children(top).child(list)
    };
    let page = col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::XL)
        .gap(space::LG)
        .child(head)
        .children(
            (!notices.is_empty()).then(|| col().max_w(COLUMN).gap(space::XS).children(notices)),
        )
        .children(first_run(mods, p))
        .child(
            col()
                .gap(space::MD)
                .child(col().child(tabs(mods, p)).child(rule(p)))
                .child(list),
        );
    col()
        .key("mods/scroll")
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(page)
}

/// What stays above a tab's list: All mods' search.
fn top(mods: &Mods, p: &Palette) -> Option<El> {
    (mods.tab == Tab::All).then(|| {
        search_field(&mods.search, "Search mods", p)
            .key("mods/search")
            .on_edit(|app: &mut App| &mut app.mods.search)
    })
}

/// Installed's Update all, at the tabs' end while there are updates: no
/// taller than the tabs, so the list under them never moves for it; faint
/// while other work runs.
fn update_all(mods: &Mods, p: &Palette) -> Option<El> {
    if mods.tab != Tab::Installed || !mods.has_updates() || mods.catalog.is_none() {
        return None;
    }
    let action = text_action("Update all", false, p).key("mods/update-all");
    Some(if mods.work.is_none() {
        action.on_press(actions::review_updates)
    } else {
        action.color(p.faint)
    })
}

fn tabs(mods: &Mods, p: &Palette) -> El {
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, mods.tab == which, p)
            .key(key.to_owned())
            .on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    row()
        .gap(space::LG)
        .child(tab_for(
            "Collections",
            mods.sets.len(),
            Tab::Sets,
            "mods/tab/sets",
        ))
        .child(tab_for(
            "All mods",
            mods.offered().len() + mods.found_unbuilt().len(),
            Tab::All,
            "mods/tab/all",
        ))
        .child(tab_for(
            "Installed",
            mods.local.mods.len(),
            Tab::Installed,
            "mods/tab/installed",
        ))
        .child(spacer())
        .child(
            row()
                .h(size::TAB)
                .items_center()
                .gap(space::XXS)
                .mr(-TEXT_ACTION_INSET)
                .children(update_all(mods, p))
                // Starting a mod of one's own, from tgg-melee's SDK.
                .children(mods.port().is_some().then(|| {
                    text_action("New mod…", false, p)
                        .key("mods/new-mod")
                        .on_press(crate::newmod::actions::open)
                })),
        )
}

/// The head's side: tgg-melee's version and the disc it plays, once it's
/// installed, then choosing another disc and checking for updates. Asking
/// textures.gg changes the action's words in place, so nothing moves.
fn head_side(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let check = if mods.registry == Registry::Asking {
        text_action("Checking…", false, p).key("mods/checking")
    } else {
        text_action("Check for updates", false, p)
            .key("mods/refresh")
            .on_press(actions::fetch)
    };
    let mut side = col()
        .key("mods/game")
        .items_end()
        .gap(space::TIGHT)
        .max_w(HEAD_SIDE)
        .min_w(0.0);
    if let Some(port) = mods.port() {
        side = side
            .child(
                row()
                    .gap(space::XS)
                    .items_end()
                    .child(label("tgg-melee").font(text::STRONG).color(p.ink))
                    .child(
                        label(port.version.clone())
                            .font(text::MONO_SMALL)
                            .color(p.muted),
                    ),
            )
            .children(disc_line(app, p));
    }
    side.child(
        row()
            .mr(-TEXT_ACTION_INSET)
            .gap(space::XXS)
            .children(mods.port().is_some().then(|| {
                text_action("Versions…", false, p)
                    .key("mods/versions")
                    .on_press(|app: &mut App| actions::show_versions(app, true))
            }))
            .children(mods.port().is_some().then(|| {
                text_action("Choose a disc…", false, p)
                    .key("mods/choose-disc")
                    .on_press_tx(actions::choose_disc)
            }))
            .child(check),
    )
}

/// The disc tgg-melee plays, in a line; a missing one is a notice instead.
fn disc_line(app: &App, p: &Palette) -> Option<El> {
    let words = match &app.mods.disc {
        Disc::Unknown | Disc::Looking => "Looking for an unmodified disc…".to_owned(),
        Disc::Found(path) => {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            format!("Plays {name}")
        }
        Disc::Missing => return None,
    };
    Some(
        label(words)
            .key("mods/disc")
            .font(text::SMALL)
            .color(p.muted)
            .truncate_middle()
            .min_w(0.0),
    )
}

/// What needs the player, each in a slim row: textures.gg unreachable,
/// what's installed unreadable, an install under way, a newer tgg-melee, a
/// version to go back to, a missing disc.
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
    let Some(port) = mods.port() else {
        return notices;
    };
    // Kept while other work runs, its button off, so the page doesn't move.
    let installing_port = mods.work.as_ref().is_some_and(|work| work.target.is_none());
    if let Some(release) = mods.port_update().filter(|_| !installing_port) {
        let words = col()
            .gap(space::TIGHT)
            .child(say(format!("tgg-melee {} is out.", release.version), false))
            .children(
                waiting(mods, &release.version)
                    .map(|words| say(words, false).key("mods/update-waits")),
            );
        let update = if mods.work.is_none() {
            button("Update", ButtonKind::Secondary, p)
                .key("mods/update-port")
                .on_press(actions::install_port)
        } else {
            button_off("Update", p).key("mods/update-port")
        };
        notices.push(callout(words, Some(update), p).key("mods/update"));
    }
    notices.extend(fallback(mods, &port.version, p));
    if app.mods.disc == Disc::Missing {
        let choose = button("Choose a disc…", ButtonKind::Secondary, p)
            .key("mods/choose-missing-disc")
            .on_press_tx(actions::choose_disc);
        notices.push(callout(say("Play needs an unmodified Melee disc: netplay and replays want the same game files on every side.".into(), true), Some(choose), p)
            .key("mods/disc-missing"));
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
    Some(panel(p).key("mods/first-run").max_w(COLUMN)
        .child(label("tgg-melee").font(text::H3).color(p.ink))
        .child(label("Melee, rebuilt to run on your computer with a mod loader built in. Mods install into it; your disc stays as it is.")
            .font(text::BODY).color(p.muted).wrap())
        .children(install))
}

/// Before updating: what the newer tgg-melee has no build of yet. Nothing
/// at all, or the installed mods that would wait for one.
fn waiting(mods: &Mods, version: &str) -> Option<String> {
    let built = mods.next_built.as_ref()?;
    if built.is_empty() {
        return Some(format!(
            "No mods are built for {version} yet. Update later to keep playing with mods."
        ));
    }
    let names = mods.unbuilt_for_update();
    let first = names.first()?;
    let who = match names.len() {
        1 => first.clone(),
        n => format!("{first} and {} others", n - 1),
    };
    let verb = if names.len() == 1 { "isn't" } else { "aren't" };
    Some(format!(
        "{who} {verb} built for {version} yet. Update now and they wait; or update later."
    ))
}

/// After an update: what isn't built for the version in use (installed
/// mods, or nothing at all), and going back to an older one installed that
/// has them.
fn fallback(mods: &Mods, current: &str, p: &Palette) -> Option<El> {
    let (words, version) = match (&mods.fallback, &mods.older_with_mods) {
        (Some(fallback), _) => {
            let count = if fallback.unfit == 1 {
                "1 of your mods isn't".to_owned()
            } else {
                format!("{} of your mods aren't", fallback.unfit)
            };
            (
                format!("{count} built for {current} yet."),
                fallback.version.clone(),
            )
        }
        (None, Some((version, count))) => (
            format!(
                "No mods are built for {current} yet; {version} has {}.",
                mod_count(*count)
            ),
            version.clone(),
        ),
        (None, None) => return None,
    };
    let back = button(&format!("Use {version} for now"), ButtonKind::Secondary, p)
        .key("mods/use-fallback")
        .on_press(move |app: &mut App| actions::use_port(app, &version));
    Some(
        callout(
            label(words).font(text::SMALL).color(p.danger).wrap(),
            Some(back),
            p,
        )
        .key("mods/fallback"),
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

/// Placeholder cards for the collections, while textures.gg first answers.
fn placeholder_cards(p: &Palette) -> impl Iterator<Item = El> + '_ {
    (0..PLACEHOLDERS / 2).map(move |_| {
        panel(p)
            .w(SET_CARD)
            .child(skeleton(0.45, p))
            .child(
                col()
                    .gap(space::XS)
                    .child(skeleton(1.0, p))
                    .child(skeleton(0.8, p)),
            )
            .child(skeleton(0.3, p))
    })
}

/// Placeholder rows for every mod, while textures.gg first answers.
fn placeholder_rows(p: &Palette) -> impl Iterator<Item = El> + '_ {
    (0..PLACEHOLDERS).map(move |_| {
        col()
            .py(space::SM)
            .gap(space::XS)
            .child(skeleton(0.3, p))
            .child(skeleton(0.6, p))
    })
}

/// Whether textures.gg hasn't answered yet this session.
fn loading(mods: &Mods) -> bool {
    matches!(mods.registry, Registry::NotAsked | Registry::Asking)
        && mods.sets.is_empty()
        && mods.catalog.is_none()
}

/// Why a list can't install yet, or has nothing: tgg-melee isn't installed,
/// or textures.gg has nothing built for it.
fn note(mods: &Mods, p: &Palette) -> Option<El> {
    let note = if mods.port().is_none() {
        Some("Install tgg-melee first; then mods install in one step.".to_owned())
    } else {
        // When an older version has mods, the notice above says so.
        mods.no_mods
            .clone()
            .filter(|_| mods.older_with_mods.is_none())
    };
    note.map(|note| {
        label(note)
            .key("mods/note")
            .font(text::SMALL)
            .color(p.muted)
    })
}

/// The sets textures.gg offers, as cards.
fn sets(mods: &Mods, p: &Palette) -> El {
    let empty = (mods.sets.is_empty() && mods.registry == Registry::Answered).then(|| {
        label("textures.gg has no collections yet.")
            .key("mods/no-sets")
            .font(text::BODY)
            .color(p.muted)
    });
    col()
        .gap(space::SM)
        .children(note(mods, p))
        .children(empty)
        .child(
            row()
                .flex_wrap()
                .gap(space::MD)
                .children(mods.sets.iter().map(|set| set_card(mods, set, p)))
                .children(
                    loading(mods)
                        .then(|| placeholder_cards(p))
                        .into_iter()
                        .flatten(),
                ),
        )
}

/// A set: its title, what it's for, the mods in it, whether it's
/// installed, and installing, updating or removing it.
fn set_card(mods: &Mods, set: &Set, p: &Palette) -> El {
    let local = mods.local.set(&set.name);
    let state = local.map(|local| match local.status {
        SetStatus::Complete => badge("Installed", Tone::Mine, p),
        SetStatus::Customized => badge("Customized", Tone::Mine, p),
        SetStatus::Outdated => badge("Update", Tone::News, p),
        SetStatus::Incomplete => badge("Incomplete", Tone::Danger, p),
    });
    let ready = mods.catalog.is_some() && mods.port().is_some() && mods.work.is_none();
    let name = set.name.clone();
    let main = ready
        .then(|| match local.map(|local| local.status) {
            None => Some(button("Install", ButtonKind::Primary, p)),
            Some(SetStatus::Outdated | SetStatus::Incomplete) => {
                Some(button("Update", ButtonKind::Secondary, p))
            }
            Some(SetStatus::Complete | SetStatus::Customized) => None,
        })
        .flatten()
        .map(|button| {
            let name = name.clone();
            button
                .key(format!("mods/set/{}/install", set.name))
                .on_press(move |app: &mut App| actions::review_set(app, &name))
        });
    let remove = local.filter(|_| mods.work.is_none()).map(|_| {
        let name = name.clone();
        text_action("Remove", true, p)
            .key(format!("mods/set/{}/remove", set.name))
            .on_press(move |app: &mut App| actions::remove_set(app, &name))
    });
    let names: Vec<String> = set.mods.iter().map(|m| mods.name_of(&m.id)).collect();
    panel(p)
        .key(format!("mods/set/{}", set.name))
        .w(SET_CARD)
        .child(
            row()
                .gap(space::XS)
                .items_center()
                .child(label(set.title.clone()).font(text::H3).color(p.ink))
                .child(spacer())
                .children(state),
        )
        .child(
            label(set.description.clone())
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        )
        .child(
            col()
                .gap(space::TIGHT)
                .child(
                    label(mod_count(set.mods.len()))
                        .font(text::MONO_SMALL)
                        .color(p.faint),
                )
                .child(
                    label(names.join(" · "))
                        .key(format!("mods/set/{}/mods", set.name))
                        .font(text::SMALL)
                        .color(p.faint)
                        .wrap(),
                ),
        )
        // Cards in a row are as tall as the tallest; the buttons sit at the foot.
        .child(spacer())
        .child(
            row()
                .gap(space::XS)
                .items_center()
                .children(main)
                .child(spacer())
                .children(remove),
        )
}

/// Every mod textures.gg has built for the game, newest release of each,
/// with a search.
fn all(mods: &Mods, p: &Palette) -> El {
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
    let rows = found.into_iter().map(|entry| catalog_row(mods, entry, p));
    // What waits for a build, after what installs.
    let waiting = (!unbuilt.is_empty()).then(|| {
        col()
            .key("mods/all/unbuilt")
            .gap(space::XS)
            .child(
                label("Not built for this version yet")
                    .font(text::LABEL)
                    .color(p.faint),
            )
            .child(list(
                unbuilt
                    .into_iter()
                    .map(|summary| unbuilt_row(mods, summary, p)),
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

/// Rows between rules.
fn list(rows: impl Iterator<Item = El>, p: &Palette) -> El {
    col()
        .child(rule(p))
        .children(rows.flat_map(|row| [row, rule(p)]))
}

/// A row of a list of mods: pressing it shows the mod's details,
/// highlighted while they show.
fn mod_line(mods: &Mods, id: &ModId, key: String, p: &Palette) -> El {
    let chosen = mods.selected.as_ref() == Some(id);
    let pressed = id.clone();
    row()
        .key(key)
        .py(space::XS)
        .px(space::XS)
        .mx(-space::XS)
        .gap(space::SM)
        .items_center()
        .radius(radius::CONTROL)
        .fill(if chosen {
            p.selected
        } else {
            rune::Color::TRANSPARENT
        })
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("button")
        .focusable(true)
        .on_press(move |app: &mut App| actions::select(app, &pressed))
}

/// A mod textures.gg offers: its name, version and line, its size, and
/// installing it, or its switch once it's installed.
fn catalog_row(mods: &Mods, entry: &CatalogEntry, p: &Palette) -> El {
    let manifest = &entry.manifest;
    let id = manifest.id.clone();
    let installed = mods.local.get(&id);
    let end = match installed {
        Some(m) => {
            let on = m.enabled;
            let toggled = id.clone();
            switch(&format!("mods/catalog/{id}/switch"), on, p)
                .label(manifest.name.clone())
                .on_press(move |app: &mut App| actions::set_enabled(app, &toggled, !on))
        }
        None if mods.port().is_some() && mods.work.is_none() => {
            let wanted = id.clone();
            button("Install", ButtonKind::Secondary, p)
                .key(format!("mods/catalog/{id}/install"))
                .on_press(move |app: &mut App| actions::review_mod(app, &wanted))
        }
        None => el(),
    };
    // Installed, the version the player has, and the newer one when it's out.
    let shown = installed.map_or_else(
        || manifest.version.to_string(),
        |m| m.manifest.version.to_string(),
    );
    let update = installed
        .and_then(|m| m.update.as_ref())
        .map(|version| update_mod(mods, &id, version, &format!("mods/catalog/{id}"), p));
    mod_line(mods, &id, format!("mods/catalog/{id}"), p)
        .label(manifest.name.clone())
        .child(icon_tile(mods.icon(&id), size::THUMBNAIL, p))
        .child(mod_text(
            mods,
            &id,
            &manifest.name,
            &shown,
            manifest.description.as_deref(),
            p,
        ))
        .children(update)
        .child(
            label(size_text(entry.package.size))
                .font(text::MONO_SMALL)
                .color(p.faint),
        )
        .child(end)
}

/// A mod the registry lists that isn't built for this tgg-melee yet: what
/// it is, and that it waits for a build.
fn unbuilt_row(mods: &Mods, summary: &ModSummary, p: &Palette) -> El {
    let id = &summary.id;
    mod_line(mods, id, format!("mods/listed/{id}"), p)
        .label(summary.name.clone())
        .child(icon_tile(mods.icon(id), size::THUMBNAIL, p))
        .child(mod_text(
            mods,
            id,
            &summary.name,
            &summary.version.to_string(),
            summary.description.as_deref(),
            p,
        ))
        .child(badge("Not built for this version yet", Tone::Plain, p))
}

/// A mod's words in a row: its name and version, its line, and who
/// publishes it and how often it's installed, when the registry says.
fn mod_text(
    mods: &Mods,
    id: &ModId,
    name: &str,
    version: &str,
    description: Option<&str>,
    p: &Palette,
) -> El {
    let listed = mods.listings.get(id).map(|summary| {
        let installs = if summary.installs == 1 {
            "install"
        } else {
            "installs"
        };
        let mut words = format!(
            "by {} · {} {installs}",
            summary.publisher.name,
            count_text(summary.installs)
        );
        if !summary.tags.is_empty() {
            words = format!("{words} · {}", summary.tags.join(", "));
        }
        label(words).font(text::SMALL).color(p.faint).truncate()
    });
    col()
        .gap(space::TIGHT)
        .grow(1.0)
        .min_w(0.0)
        .child(
            row()
                .gap(space::XS)
                .items_end()
                .child(
                    label(name.to_owned())
                        .font(text::STRONG)
                        .color(p.ink)
                        .truncate(),
                )
                .child(
                    label(version.to_owned())
                        .font(text::MONO_SMALL)
                        .color(p.faint),
                )
                .children(mods.official(id).then(|| badge("Official", Tone::News, p))),
        )
        .children(description.map(|line| {
            label(line.to_owned())
                .font(text::SMALL)
                .color(p.muted)
                .truncate()
        }))
        .children(listed)
}

/// What's installed, each mod with its switch; updating them all.
fn installed(mods: &Mods, p: &Palette) -> El {
    let empty = mods.local.mods.is_empty().then(|| {
        label("Nothing installed yet. Install a collection, or any mod from All mods.")
            .key("mods/installed/empty")
            .font(text::BODY)
            .color(p.muted)
    });
    let rows = mods.local.mods.iter().map(|m| mod_row(mods, m, p));
    col()
        .key("mods/installed")
        .gap(space::SM)
        .children(empty)
        .children((!mods.local.mods.is_empty()).then(|| list(rows, p)))
}

/// An installed mod: its switch, name and version, where it came from, and
/// its update.
fn mod_row(mods: &Mods, m: &LocalMod, p: &Palette) -> El {
    let id = m.id().clone();
    let on = m.enabled;
    let key = format!("mods/mod/{id}");
    let origin = if m.dev {
        "A dev build".to_owned()
    } else if m.sets().is_empty() {
        "On its own".to_owned()
    } else {
        m.sets()
            .iter()
            .map(|name| mods.set_title(name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let notes = m
        .update
        .as_ref()
        .map(|version| update_mod(mods, &id, version, &key, p))
        .or_else(|| {
            (m.fits == Some(false)).then(|| badge("Not built for this tgg-melee", Tone::Danger, p))
        });
    let toggled = id.clone();
    let toggle = switch(&format!("{key}/switch"), on, p)
        .label(m.manifest.name.clone())
        .on_press(move |app: &mut App| actions::set_enabled(app, &toggled, !on));
    let removed = id.clone();
    mod_line(mods, &id, key.clone(), p)
        .label(m.manifest.name.clone())
        .child(toggle)
        .child(icon_tile(mods.icon(&id), size::THUMBNAIL, p))
        .child(
            col()
                .gap(space::TIGHT)
                .grow(1.0)
                .min_w(0.0)
                .child(
                    row()
                        .gap(space::XS)
                        .items_end()
                        .child(
                            label(m.manifest.name.clone())
                                .font(text::STRONG)
                                .color(if on { p.ink } else { p.muted })
                                .truncate(),
                        )
                        .child(
                            label(m.manifest.version.to_string())
                                .font(text::MONO_SMALL)
                                .color(p.faint),
                        ),
                )
                .child(status(origin, false, p)),
        )
        .children(notes)
        // Quiet in a list of many; removing a whole set is the loud one.
        .child(
            text_action("Remove", false, p)
                .key(format!("{key}/remove"))
                .mr(-TEXT_ACTION_INSET)
                .on_press(move |app: &mut App| actions::remove(app, &removed)),
        )
}

/// "1 mod", "8 mods".
pub fn mod_count(count: usize) -> String {
    if count == 1 {
        "1 mod".to_owned()
    } else {
        format!("{count} mods")
    }
}

/// A newer release of an installed mod, and updating it alone (through the
/// review), keyed `{key}/update`; just the news while an install runs.
pub(super) fn update_mod(
    mods: &Mods,
    id: &ModId,
    version: &impl std::fmt::Display,
    key: &str,
    p: &Palette,
) -> El {
    // Updating this one: its progress, in place of the news.
    if let Some(work) = mods
        .work
        .as_ref()
        .filter(|work| work.target.as_ref() == Some(id))
    {
        return row()
            .key(format!("{key}/updating"))
            .gap(space::XS)
            .items_center()
            .child(label("Updating…").font(text::SMALL).color(p.muted))
            .child(
                col()
                    .w(size::INLINE_PROGRESS)
                    .child(progress(work.fraction(), p)),
            );
    }
    let wanted = id.clone();
    row()
        .gap(space::XS)
        .items_center()
        .child(badge(format!("{version} is out"), Tone::News, p))
        .child(if mods.work.is_none() {
            button("Update", ButtonKind::Secondary, p)
                .key(format!("{key}/update"))
                .on_press(move |app: &mut App| actions::review_mod_update(app, &wanted))
        } else {
            button_off("Update", p).key(format!("{key}/update"))
        })
}
