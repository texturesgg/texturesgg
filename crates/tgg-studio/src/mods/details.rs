//! A mod's details, in a pane that slides in beside the lists: what it
//! does, its version and size, what it needs, the sets that include it,
//! whether netplay peers must have it too, and installing, turning on or
//! off, or removing it. Pressing the mod again, or the pane's close, hides it.

use super::{actions, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, motion, size, space, text};
use crate::ui::{ButtonKind, Tone, badge, button, rule, spacer, switch, text_action};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::Manifest;

/// Whether the pane shows: on Mods, while a mod is chosen.
pub fn shown(app: &App) -> bool {
    app.nav.place == crate::nav::Place::Mods && app.mods.selected.is_some()
}

/// The pane at the window's right, the whole window tall as the sidebar is
/// at its left: its own head (which moves the window and, while it shows,
/// holds the window's controls) over the mod's details. As wide as the
/// editor's pane column while it shows, narrowing to nothing on the default
/// spring otherwise; its content is pinned to the window's edge, so it
/// slides in rather than unfolding.
pub fn view(app: &App, p: &Palette) -> El {
    let shown = shown(app);
    let head = row().h(size::TITLE_ROW).pl(space::MD).pr(space::XS).gap(space::XS).items_center().window_drag()
        .child(label("Details").font(text::LABEL).color(p.faint))
        .child(spacer())
        .child(text_action("Hide", false, p).key("mods/details/close").on_press(actions::close_details))
        .children(shown.then(|| crate::shell::window_controls(p)).flatten());
    row().key("mods/details").w(if shown { size::PANE_COLUMN + size::LINE } else { 0.0 }).h_full().justify_end()
        .motion(motion::DEFAULT).clip_rounded(0.0).fill(p.sunk)
        .child(el().w(size::LINE).h_full().fill(p.line))
        .child(col().w(size::PANE_COLUMN).h_full()
            .child(head)
            .child(col().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).children(body(app, p))))
}

/// What the pane says about the chosen mod.
fn body(app: &App, p: &Palette) -> Option<El> {
    let mods = &app.mods;
    let id = mods.selected.as_ref()?;
    let installed = mods.local.get(id);
    let offered = mods.newest(id);
    let manifest: &Manifest = installed.map(|m| &m.manifest).or(offered.map(|entry| &entry.manifest))?;
    let head = col().gap(space::TIGHT)
        .child(label(manifest.name.clone()).font(text::H3).color(p.ink).wrap())
        .child(label(manifest.id.to_string()).font(text::MONO_SMALL).color(p.faint).truncate());
    let mut facts = vec![fact("Version", manifest.version.to_string(), p)];
    if let Some(entry) = offered {
        if installed.is_some_and(|m| m.manifest.version < entry.manifest.version) {
            facts.push(fact("Newest", entry.manifest.version.to_string(), p));
        }
        facts.push(fact("Download", size_text(entry.package.size), p));
        facts.push(fact("Netplay", if entry.netplay.counts() { "Players need the same version" } else { "Doesn't affect netplay" }.to_owned(), p));
    }
    if !manifest.depends.is_empty() {
        let needs: Vec<String> = manifest.depends.iter().map(|(id, range)| format!("{} {range}", mods.name_of(id))).collect();
        facts.push(fact("Needs", needs.join(", "), p));
    }
    let sets = mods.sets_with(id);
    if !sets.is_empty() {
        facts.push(fact("In collections", sets.join(", "), p));
    }
    if let Some(license) = &manifest.license {
        facts.push(fact("License", license.clone(), p));
    }
    Some(col().key("mods/details/body").px(space::MD).pt(space::XS).pb(space::MD).gap(space::MD)
        .child(head)
        .children(manifest.description.clone().map(|line| label(line).font(text::BODY).color(p.muted).wrap()))
        .child(actions_for(app, p))
        .child(rule(p))
        .child(col().gap(space::SM).children(facts)))
}

/// Installing the mod, or, once it's installed, its switch and removing it.
fn actions_for(app: &App, p: &Palette) -> El {
    let mods = &app.mods;
    let Some(id) = mods.selected.clone() else { return el() };
    match mods.local.get(&id) {
        Some(m) => {
            let on = m.enabled;
            let (toggled, removed) = (id.clone(), id.clone());
            row().gap(space::XS).items_center()
                .child(switch("mods/details/switch", on, p).label(if on { "On" } else { "Off" })
                    .on_press(move |app: &mut App| actions::set_enabled(app, &toggled, !on)))
                .child(label(if on { "On" } else { "Off" }).font(text::STRONG).color(p.ink))
                .children(m.update.as_ref().map(|version| badge(format!("{version} is out"), Tone::News, p)))
                .child(spacer())
                .child(text_action("Remove", true, p).key("mods/details/remove").on_press(move |app: &mut App| actions::remove(app, &removed)))
        }
        None if mods.port().is_none() => label("Install tgg-melee first; then this installs in one step.").font(text::SMALL).color(p.muted).wrap(),
        None => row().children(mods.work.is_none().then(|| {
            button("Install", ButtonKind::Primary, p).key("mods/details/install")
                .on_press(move |app: &mut App| actions::review_mod(app, &id))
        })),
    }
}

/// One fact about the mod: what it is, over its value.
fn fact(name: &str, value: String, p: &Palette) -> El {
    col().gap(space::TIGHT)
        .child(label(name.to_owned()).font(text::LABEL).color(p.faint))
        .child(label(value).font(text::BODY).color(p.ink).wrap())
}
