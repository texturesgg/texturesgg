//! Versions: tgg-melee's versions, installed and offered, newest first,
//! shown inline inside Settings. The one in use is marked; another installed
//! one can be used or removed, and one not installed yet installed (and used).

use super::{Mods, Registry, actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{ButtonKind, Tone, badge, button, progress, rule, text_action};
use rune::{El, col, row, text as label};

/// The versions list for Settings' inline Versions sub-view.
pub fn body(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let listed = versions(mods);
    let working = mods.work.as_ref().map(|work| {
        col()
            .key("mods/versions/work")
            .px(space::LG)
            .py(space::XS)
            .gap(space::XXS)
            .child(
                label(work.label())
                    .font(text::SMALL)
                    .color(p.muted)
                    .truncate(),
            )
            .child(progress(work.fraction(), p))
    });
    if listed.is_empty() {
        let empty = if mods.registry == Registry::Asking {
            col().px(space::LG).py(space::SM).child(
                label("Checking textures.gg…")
                    .font(text::BODY)
                    .color(p.muted),
            )
        } else {
            row()
                .px(space::LG)
                .py(space::SM)
                .gap(space::XS)
                .items_center()
                .child(
                    label("No tgg-melee versions found.")
                        .font(text::BODY)
                        .color(p.muted),
                )
                .child(
                    text_action("Check again", false, p)
                        .key("mods/versions/check")
                        .on_press(actions::fetch),
                )
        };
        return col()
            .key("mods/versions")
            .children(working)
            .child(empty);
    }
    let rows = listed
        .into_iter()
        .enumerate()
        .map(|(index, version)| version_row(mods, &version, index == 0, p));
    col()
        .key("mods/versions")
        .children(working)
        .children(rows.flat_map(|row| [row, rule(p)]))
}

/// Every version: those textures.gg offers, newest first, then any
/// installed it no longer offers.
fn versions(mods: &Mods) -> Vec<String> {
    let mut versions: Vec<String> = mods
        .releases
        .iter()
        .map(|release| release.version.clone())
        .collect();
    for port in mods.ports.iter().rev() {
        if !versions.contains(&port.version) {
            versions.push(port.version.clone());
        }
    }
    versions
}

/// A version: whether it's in use, installed, the newest; and using,
/// removing or installing it.
fn version_row(mods: &Mods, version: &str, newest: bool, p: &Palette) -> El {
    let current = mods.port().is_some_and(|port| port.version == version);
    let installed = mods.has_port(version);
    let busy = mods.work.is_some();
    let size = mods
        .releases
        .iter()
        .find(|release| release.version == version)
        .map(|release| size_text(release.size()));
    let state = if current {
        Some(badge("In use", Tone::Mine, p))
    } else if installed {
        Some(badge("Installed", Tone::Plain, p))
    } else {
        None
    };
    let owned = version.to_owned();
    let actions: Vec<El> = if current || busy {
        Vec::new()
    } else if installed {
        let (used, removed) = (owned.clone(), owned.clone());
        vec![
            text_action("Remove", true, p)
                .key(format!("mods/versions/{version}/remove"))
                .on_press(move |app: &mut App| actions::remove_port(app, &removed)),
            button("Use", ButtonKind::Secondary, p)
                .key(format!("mods/versions/{version}/use"))
                .on_press(move |app: &mut App| actions::use_port(app, &used)),
        ]
    } else {
        vec![
            button("Install", ButtonKind::Secondary, p)
                .key(format!("mods/versions/{version}/install"))
                .on_press(move |app: &mut App| actions::install_version(app, &owned)),
        ]
    };
    row()
        .key(format!("mods/versions/{version}"))
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
                    row()
                        .gap(space::XS)
                        .items_center()
                        .child(label(version.to_owned()).font(text::MONO_TEXT).color(p.ink))
                        .children(newest.then(|| badge("Newest", Tone::News, p))),
                )
                .children(
                    size.filter(|_| !installed)
                        .map(|size| label(size).font(text::MONO_SMALL).color(p.faint)),
                ),
        )
        .children(state)
        .children(actions)
}
