//! The Mods page: the game (tgg-melee, and the disc it plays), then the
//! sets, every mod, or what's installed, a tab each; a mod's details slide
//! in beside them.

use super::{Disc, Mods, Registry, Tab, actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, Tone, badge, button, page_head, panel, progress, rule, search_field, spacer, status,
    switch, tab, text_action,
};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::{CatalogEntry, LocalMod, ModId, Set, SetStatus};

/// A set's card: wide enough for its description to read as a line or two.
const SET_CARD: f32 = 300.0;
/// The game's panel, and the lists, read best no wider than this.
const COLUMN: f32 = 720.0;

pub fn view(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let head = row().items_end().gap(space::SM)
        .child(col().grow(1.0).child(page_head("Mods", "From textures.gg, for tgg-melee: Melee with mods built in.", p)))
        .children((mods.registry != Registry::Asking).then(|| {
            text_action("Check for updates", false, p).key("mods/refresh").on_press(actions::fetch)
        }));
    let list = match mods.tab {
        Tab::Sets => sets(mods, p),
        Tab::All => all(mods, p),
        Tab::Installed => installed(mods, p),
    };
    let page = col().px(space::LG + 4.0).pt(space::XS).pb(space::XL).gap(space::LG)
        .child(head)
        .children(registry_line(mods, p))
        .children(mods.problem.as_ref().map(|problem| label(problem.clone()).key("mods/problem").font(text::BODY).color(p.danger)))
        .child(game(app, p))
        .child(col().gap(space::MD).child(col().child(tabs(mods, p)).child(rule(p))).child(list));
    row().grow(1.0).min_h(0.0).w_full()
        .child(col().key("mods/scroll").grow(1.0).min_w(0.0).h_full().scroll(ScrollSpec { stick_bottom: false }).child(page))
        .child(super::details::view(app, p))
}

fn tabs(mods: &Mods, p: &Palette) -> El {
    let tab_for = |name: &str, count: usize, which: Tab, key: &str| {
        tab(name, count, mods.tab == which, p).key(key.to_owned()).on_press(move |app: &mut App| actions::show_tab(app, which))
    };
    row().gap(space::LG)
        .child(tab_for("Collections", mods.sets.len(), Tab::Sets, "mods/tab/sets"))
        .child(tab_for("All mods", mods.offered().len(), Tab::All, "mods/tab/all"))
        .child(tab_for("Installed", mods.local.mods.len(), Tab::Installed, "mods/tab/installed"))
}

/// Whether textures.gg is being asked, or couldn't be.
fn registry_line(mods: &Mods, p: &Palette) -> Option<El> {
    let (words, danger) = match &mods.registry {
        Registry::Asking => ("Checking textures.gg…".to_owned(), false),
        Registry::Offline => ("Can't reach textures.gg. What's installed still works.".to_owned(), true),
        Registry::Failed(why) => (why.clone(), true),
        Registry::NotAsked | Registry::Answered => return None,
    };
    Some(label(words).key("mods/registry").font(text::BODY).color(if danger { p.danger } else { p.muted }))
}

/// tgg-melee: installing it, or the version in use, its update, and the
/// disc it plays; the install's progress while one runs.
fn game(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let mut body = panel(p).key("mods/game").max_w(COLUMN);
    match mods.port() {
        None => {
            body = body
                .child(label("tgg-melee").font(text::H3).color(p.ink))
                .child(label("Melee, rebuilt to run on your computer with a mod loader built in. Mods install into it; your disc stays as it is.")
                    .font(text::BODY).color(p.muted).wrap());
            if let Some(release) = mods.releases.first().filter(|_| mods.work.is_none()) {
                let words = format!("Install tgg-melee {} · {}", release.version, size_text(release.size()));
                body = body.child(row().child(button(&words, ButtonKind::Primary, p).key("mods/install-port").on_press(actions::install_port)));
            }
        }
        Some(port) => {
            let update = mods.port_update().filter(|_| mods.work.is_none()).map(|release| {
                row().gap(space::XS).items_center()
                    .child(badge(format!("{} is out", release.version), Tone::News, p))
                    .child(button("Update", ButtonKind::Secondary, p).key("mods/update-port").on_press(actions::install_port))
            });
            body = body
                .child(row().gap(space::XS).items_center()
                    .child(label("tgg-melee").font(text::H3).color(p.ink))
                    .child(label(port.version.clone()).font(text::MONO_TEXT).color(p.muted))
                    .child(spacer())
                    .children(update))
                .child(disc(app, p));
        }
    }
    if let Some(work) = &mods.work {
        body = body.child(working(work, p));
    }
    body
}

/// The disc tgg-melee plays, and choosing another.
fn disc(app: &App, p: &Palette) -> El {
    let (words, found) = match &app.mods.disc {
        Disc::Unknown | Disc::Looking => ("Looking for an unmodified Melee disc…".to_owned(), true),
        Disc::Found(path) => {
            let name = path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
            (format!("Plays {name}"), true)
        }
        Disc::Missing => ("Needs an unmodified Melee disc: netplay and replays want the same game files on every side.".to_owned(), false),
    };
    row().gap(space::XS).items_center()
        .child(label(words).key("mods/disc").font(text::SMALL).color(if found { p.muted } else { p.danger }).wrap().shrink().min_w(0.0))
        .child(spacer())
        .child(text_action("Choose a disc…", false, p).key("mods/choose-disc").on_press_tx(actions::choose_disc))
}

/// An install's step and progress, and stopping it.
fn working(work: &super::Work, p: &Palette) -> El {
    col().key("mods/work").gap(space::XS)
        .child(row().gap(space::XS).items_center()
            .child(label(work.label()).font(text::SMALL).color(p.muted).truncate().shrink().min_w(0.0))
            .child(spacer())
            .child(text_action("Stop", false, p).key("mods/stop-work").on_press(actions::cancel_work)))
        .child(progress(work.fraction(), p))
}

/// Why a list can't install yet, or has nothing: tgg-melee isn't installed,
/// or textures.gg has nothing built for it.
fn note(mods: &Mods, p: &Palette) -> Option<El> {
    let note = if mods.port().is_none() {
        Some("Install tgg-melee first; then mods install in one step.".to_owned())
    } else {
        mods.no_mods.clone()
    };
    note.map(|note| label(note).key("mods/note").font(text::SMALL).color(p.muted))
}

/// The sets textures.gg offers, as cards.
fn sets(mods: &Mods, p: &Palette) -> El {
    let empty = (mods.sets.is_empty() && mods.registry == Registry::Answered)
        .then(|| label("textures.gg has no collections yet.").key("mods/no-sets").font(text::BODY).color(p.muted));
    col().gap(space::SM)
        .children(note(mods, p))
        .children(empty)
        .child(row().flex_wrap().gap(space::MD).children(mods.sets.iter().map(|set| set_card(mods, set, p))))
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
    let main = ready.then(|| match local.map(|local| local.status) {
        None => Some(button("Install", ButtonKind::Primary, p)),
        Some(SetStatus::Outdated | SetStatus::Incomplete) => Some(button("Update", ButtonKind::Secondary, p)),
        Some(SetStatus::Complete | SetStatus::Customized) => None,
    }).flatten().map(|button| {
        let name = name.clone();
        button.key(format!("mods/set/{}/install", set.name)).on_press(move |app: &mut App| actions::review_set(app, &name))
    });
    let remove = local.filter(|_| mods.work.is_none()).map(|_| {
        let name = name.clone();
        text_action("Remove", true, p).key(format!("mods/set/{}/remove", set.name))
            .on_press(move |app: &mut App| actions::remove_set(app, &name))
    });
    let names: Vec<String> = set.mods.iter().map(|m| mods.name_of(&m.id)).collect();
    panel(p).key(format!("mods/set/{}", set.name)).w(SET_CARD)
        .child(row().gap(space::XS).items_center()
            .child(label(set.title.clone()).font(text::H3).color(p.ink))
            .child(spacer())
            .children(state))
        .child(label(set.description.clone()).font(text::BODY).color(p.muted).wrap())
        .child(col().gap(space::TIGHT)
            .child(label(mod_count(set.mods.len())).font(text::MONO_SMALL).color(p.faint))
            .child(label(names.join(" · ")).key(format!("mods/set/{}/mods", set.name)).font(text::SMALL).color(p.faint).wrap()))
        .child(row().gap(space::XS).items_center().children(main).child(spacer()).children(remove))
}

/// Every mod textures.gg has built for the game, newest release of each,
/// with a search.
fn all(mods: &Mods, p: &Palette) -> El {
    let found = mods.found();
    let empty = found.is_empty().then(|| {
        let words = if mods.offered().is_empty() { "textures.gg has no mods for this tgg-melee yet." } else { "No mod matches that." };
        label(words).key("mods/all/empty").font(text::BODY).color(p.muted)
    });
    let rows = found.into_iter().map(|entry| catalog_row(mods, entry, p));
    col().max_w(COLUMN).gap(space::SM)
        .child(search_field(&mods.search, "Search mods", p).key("mods/search").on_edit(|app: &mut App| &mut app.mods.search))
        .children(note(mods, p).filter(|_| mods.port().is_none()))
        .children(empty)
        .child(list(rows, p))
}

/// Rows between rules.
fn list(rows: impl Iterator<Item = El>, p: &Palette) -> El {
    col().child(rule(p)).children(rows.flat_map(|row| [row, rule(p)]))
}

/// A row of a list of mods: pressing it shows the mod's details,
/// highlighted while they show.
fn mod_line(mods: &Mods, id: &ModId, key: String, p: &Palette) -> El {
    let chosen = mods.selected.as_ref() == Some(id);
    let pressed = id.clone();
    row().key(key).py(space::XS).px(space::XS).mx(-space::XS).gap(space::SM).items_center().radius(radius::CONTROL)
        .fill(if chosen { p.selected } else { rune::Color::TRANSPARENT }).hover(p.hover).cursor(rune::Cursor::Pointer)
        .role("button").focusable(true)
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
            switch(&format!("mods/catalog/{id}/switch"), on, p).label(manifest.name.clone())
                .on_press(move |app: &mut App| actions::set_enabled(app, &toggled, !on))
        }
        None if mods.port().is_some() && mods.work.is_none() => {
            let wanted = id.clone();
            button("Install", ButtonKind::Secondary, p).key(format!("mods/catalog/{id}/install"))
                .on_press(move |app: &mut App| actions::review_mod(app, &wanted))
        }
        None => el(),
    };
    mod_line(mods, &id, format!("mods/catalog/{id}"), p).label(manifest.name.clone())
        .child(col().gap(space::TIGHT).grow(1.0).min_w(0.0)
            .child(row().gap(space::XS).items_end()
                .child(label(manifest.name.clone()).font(text::STRONG).color(p.ink).truncate())
                .child(label(manifest.version.to_string()).font(text::MONO_SMALL).color(p.faint)))
            .children(manifest.description.clone().map(|line| label(line).font(text::SMALL).color(p.muted).truncate())))
        .child(label(size_text(entry.package.size)).font(text::MONO_SMALL).color(p.faint))
        .child(end)
}

/// What's installed, each mod with its switch; updating them all.
fn installed(mods: &Mods, p: &Palette) -> El {
    let update_all = (mods.has_updates() && mods.catalog.is_some() && mods.work.is_none())
        .then(|| button("Update all", ButtonKind::Secondary, p).key("mods/update-all").on_press(actions::review_updates));
    let empty = mods.local.mods.is_empty()
        .then(|| label("Nothing installed yet. Install a collection, or any mod from All mods.").key("mods/installed/empty").font(text::BODY).color(p.muted));
    let rows = mods.local.mods.iter().map(|m| mod_row(mods, m, p));
    col().key("mods/installed").max_w(COLUMN).gap(space::SM)
        .children(update_all.map(|update_all| row().child(spacer()).child(update_all)))
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
        m.sets().iter().map(|name| mods.set_title(name)).collect::<Vec<_>>().join(", ")
    };
    let notes = m.update.as_ref().map(|version| badge(format!("{version} is out"), Tone::News, p))
        .or_else(|| (m.fits == Some(false)).then(|| badge("Not built for this tgg-melee", Tone::Danger, p)));
    let toggled = id.clone();
    let toggle = switch(&format!("{key}/switch"), on, p).label(m.manifest.name.clone())
        .on_press(move |app: &mut App| actions::set_enabled(app, &toggled, !on));
    let removed = id.clone();
    mod_line(mods, &id, key.clone(), p).label(m.manifest.name.clone())
        .child(toggle)
        .child(col().gap(space::TIGHT).grow(1.0).min_w(0.0)
            .child(row().gap(space::XS).items_end()
                .child(label(m.manifest.name.clone()).font(text::STRONG).color(if on { p.ink } else { p.muted }).truncate())
                .child(label(m.manifest.version.to_string()).font(text::MONO_SMALL).color(p.faint)))
            .child(status(origin, false, p)))
        .children(notes)
        // Quiet in a list of many; removing a whole set is the loud one.
        .child(text_action("Remove", false, p).key(format!("{key}/remove")).mr(-TEXT_ACTION_INSET)
            .on_press(move |app: &mut App| actions::remove(app, &removed)))
}

/// The title row's action on Mods: Play, once tgg-melee is installed, or
/// Stop while it runs.
pub fn title_actions(app: &App, p: &Palette) -> Option<El> {
    let mods = &app.mods;
    mods.port()?;
    if mods.playing.is_some() {
        return Some(button("Stop tgg-melee", ButtonKind::Secondary, p).key("mods/stop").on_press(actions::stop));
    }
    matches!(mods.disc, Disc::Found(_)).then(|| button("Play", ButtonKind::Primary, p).key("mods/play").on_press(actions::play))
}

/// "1 mod", "8 mods".
pub fn mod_count(count: usize) -> String {
    if count == 1 { "1 mod".to_owned() } else { format!("{count} mods") }
}
