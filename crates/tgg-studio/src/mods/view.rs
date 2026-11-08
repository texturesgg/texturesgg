//! The Mods page: the game (tgg-melee, and the disc it plays), the sets
//! textures.gg offers, and the mods installed, each with its switch.

use super::{Disc, Mods, Registry, actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, TEXT_ACTION_INSET, Tone, badge, button, page_head, panel, progress, rule, section_label, spacer, status,
    switch, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::{LocalMod, Set, SetStatus};

/// A set's card: wide enough for its description to read as a line or two.
const SET_CARD: f32 = 300.0;
/// The game's panel, and the installed list, read best no wider than this.
const COLUMN: f32 = 720.0;

pub fn view(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let head = row().items_end().gap(space::SM)
        .child(col().grow(1.0).child(page_head("Mods", "From textures.gg, for tgg-melee: Melee with mods built in.", p)))
        .children((mods.registry != Registry::Asking).then(|| {
            text_action("Check for updates", false, p).key("mods/refresh").on_press(actions::fetch)
        }));
    let page = col().px(space::LG + 4.0).pt(space::XS).pb(space::XL).gap(space::LG)
        .child(head)
        .children(registry_line(mods, p))
        .children(mods.problem.as_ref().map(|problem| label(problem.clone()).key("mods/problem").font(text::BODY).color(p.danger)))
        .child(game(app, p))
        .child(sets(mods, p))
        .children(installed(mods, p));
    col().key("mods/scroll").grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page)
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

/// The sets textures.gg offers, as cards.
fn sets(mods: &Mods, p: &Palette) -> El {
    let note = if mods.port().is_none() {
        Some("Install tgg-melee first; then a set installs in one step.".to_owned())
    } else {
        mods.no_mods.clone()
    };
    let empty = (mods.sets.is_empty() && mods.registry == Registry::Answered)
        .then(|| label("textures.gg has no sets yet.").key("mods/no-sets").font(text::BODY).color(p.muted));
    col().gap(space::SM)
        .child(section_label("Sets", None, p))
        .children(note.map(|note| label(note).key("mods/sets-note").font(text::SMALL).color(p.muted)))
        .children(empty)
        .child(row().flex_wrap().gap(space::MD).children(mods.sets.iter().map(|set| set_card(mods, set, p))))
}

/// A set: its title, what it's for, how many mods, whether it's installed,
/// and installing, updating or removing it.
fn set_card(mods: &Mods, set: &Set, p: &Palette) -> El {
    let local = mods.local.set(&set.name);
    let state = local.map(|local| match local.status {
        SetStatus::Complete => badge("Installed", Tone::Mine, p),
        SetStatus::Customized => badge("Customized", Tone::Mine, p),
        SetStatus::Outdated => badge("Update", Tone::News, p),
        SetStatus::Incomplete => badge("Incomplete", Tone::Danger, p),
    });
    let ready = mods.catalog.is_some() && mods.work.is_none();
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
    panel(p).key(format!("mods/set/{}", set.name)).w(SET_CARD)
        .child(row().gap(space::XS).items_center()
            .child(label(set.title.clone()).font(text::H3).color(p.ink))
            .child(spacer())
            .children(state))
        .child(label(set.description.clone()).font(text::BODY).color(p.muted).wrap())
        .child(label(mod_count(set.mods.len())).font(text::MONO_SMALL).color(p.faint))
        .child(row().gap(space::XS).items_center().children(main).child(spacer()).children(remove))
}

/// What's installed, each mod with its switch; updating them all.
fn installed(mods: &Mods, p: &Palette) -> Option<El> {
    if mods.local.mods.is_empty() {
        return None;
    }
    let update_all = (mods.has_updates() && mods.catalog.is_some() && mods.work.is_none())
        .then(|| button("Update all", ButtonKind::Secondary, p).key("mods/update-all").on_press(actions::review_updates));
    let rows = mods.local.mods.iter().map(|m| mod_row(mods, m, p));
    Some(col().key("mods/installed").max_w(COLUMN).gap(space::SM)
        .child(row().items_center().child(section_label("Installed", Some(mods.local.mods.len()), p)).child(spacer()).children(update_all))
        .child(col().child(rule(p)).children(rows.flat_map(|row| [row, rule(p)]))))
}

/// A mod: its switch, name and version, where it came from, and its update.
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
    let toggle = switch(&format!("{key}/switch"), on, p).label(m.manifest.name.clone())
        .on_press(move |app: &mut App| actions::set_enabled(app, &id, !on));
    let remove_id = m.id().clone();
    row().key(key.clone()).py(space::XS).gap(space::SM).items_center()
        .child(toggle)
        .child(col().gap(space::TIGHT).grow(1.0).min_w(0.0)
            .child(row().gap(space::XS).items_end()
                .child(label(m.manifest.name.clone()).font(text::STRONG).color(if on { p.ink } else { p.muted }).truncate())
                .child(label(m.manifest.version.to_string()).font(text::MONO_SMALL).color(p.faint)))
            .child(status(origin, false, p)))
        .children(notes)
        // Quiet in a list of many; removing a whole set is the loud one.
        .child(text_action("Remove", false, p).key(format!("{key}/remove")).mr(-TEXT_ACTION_INSET)
            .on_press(move |app: &mut App| actions::remove(app, &remove_id)))
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
fn mod_count(count: usize) -> String {
    if count == 1 { "1 mod".to_owned() } else { format!("{count} mods") }
}
