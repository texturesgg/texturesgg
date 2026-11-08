//! Versions: tgg-melee's versions, installed and offered, newest first.
//! The one in use is marked; another installed one can be used or removed,
//! and one not installed yet installed (and used). It's modal: Escape or a
//! click outside closes it.

use super::{Mods, actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, Tone, badge, button, dialog, rule, spacer, text_action,
};
use rune::{El, ScrollSpec, col, row, text as label};

/// The list's tallest, in points; longer lists scroll.
const LIST_HEIGHT: f32 = 360.0;

/// The dialog over a scrim, while it shows.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    let mods = &app.mods;
    if !mods.versions || app.nav.place != crate::nav::Place::Mods {
        return None;
    }
    let rows = versions(mods)
        .into_iter()
        .enumerate()
        .map(|(index, version)| version_row(mods, &version, index == 0, p));
    let list = col()
        .max_h(LIST_HEIGHT)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(col().children(rows.flat_map(|row| [row, rule(p)])));
    let parts = Dialog {
        title: "tgg-melee versions".into(),
        line: "Keep an older version while your mods catch up with a new one; your mods stay installed whichever you use.".into(),
        body: Some(list),
        buttons: vec![spacer(), button("Done", ButtonKind::Secondary, p).key("mods/versions/done").on_press(actions::hide_versions)],
    };
    Some(dialog(
        "mods/versions",
        "dialog",
        DialogWidth::Narrow,
        parts,
        actions::hide_versions,
        p,
    ))
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
