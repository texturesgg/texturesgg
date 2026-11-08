//! A mod's details, in a pane that slides in beside the lists: what it
//! does, its version and size, what it needs, the sets that include it,
//! whether netplay peers must have it too, and installing, turning on or
//! off, or removing it. Pressing the mod again, or the pane's close, hides it.

use super::{Expand, actions, count_text, size_text};
use crate::state::App;
use crate::ui::tokens::{Palette, motion, radius, size, space, text};
use crate::ui::{ButtonKind, TEXT_ACTION_INSET, Tone, badge, button, icon_tile, rule, spacer, switch, text_action};
use rune::{El, ScrollSpec, col, el, row, text as label};
use tgg_mod::{Manifest, ModDetails};

/// The mod's icon in the pane's head.
const ICON: f32 = 48.0;

/// How much of a long README or release history shows before "Show more":
/// blocks (paragraphs, headings, list items) of the README, releases, and
/// blocks of each changelog.
const README_BLOCKS: usize = 6;
const RELEASES_SHOWN: usize = 3;
const CHANGELOG_BLOCKS: usize = 4;

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

/// What the pane says about the chosen mod: from what's installed, the
/// catalog and the registry's listing, then its registry page once that's
/// arrived.
fn body(app: &App, p: &Palette) -> Option<El> {
    let mods = &app.mods;
    let id = mods.selected.as_ref()?;
    let installed = mods.local.get(id);
    let offered = mods.newest(id);
    let listed = mods.listings.get(id);
    let manifest: Option<&Manifest> = installed.map(|m| &m.manifest).or(offered.map(|entry| &entry.manifest));
    let name = manifest.map(|manifest| manifest.name.clone()).or_else(|| listed.map(|summary| summary.name.clone()))?;
    let page = mods.details.get(id).and_then(Option::as_ref);
    let head = row().gap(space::SM).items_center()
        .child(icon_tile(mods.icon(id), ICON, p))
        .child(col().gap(space::TIGHT).grow(1.0).min_w(0.0)
            .child(label(name).font(text::H3).color(p.ink).wrap())
            .child(label(listed.map_or_else(|| id.to_string(), |summary| format!("by {}", summary.author))).font(text::SMALL).color(p.muted).truncate()));
    let description = manifest.and_then(|manifest| manifest.description.clone()).or_else(|| listed.and_then(|summary| summary.description.clone()));
    let tags = listed.filter(|summary| !summary.tags.is_empty())
        .map(|summary| row().flex_wrap().gap(space::SNUG).children(summary.tags.iter().map(|tag| badge(tag.clone(), Tone::Plain, p))));
    Some(col().key("mods/details/body").px(space::MD).pt(space::XS).pb(space::MD).gap(space::MD)
        .child(head)
        .children(tags)
        .children(description.map(|line| label(line).font(text::BODY).color(p.muted).wrap()))
        .child(actions_for(app, p))
        .children(page.and_then(|page| links(page, p)))
        .child(rule(p))
        .child(col().gap(space::SM).children(facts(app, id, p)))
        .children(page.map(|page| screenshots(mods, page, p)))
        .children(page.and_then(|page| page.readme.as_deref()).map(|readme| section("About", readme_shown(readme, mods.expanded.contains(&Expand::Readme), p), p).key("mods/details/readme")))
        .children(page.filter(|page| !page.releases.is_empty()).map(|page| section("Releases", releases(page, mods.expanded.contains(&Expand::Releases), p), p).key("mods/details/releases")))
        .children((mods.details.get(id).is_some_and(Option::is_none)).then(|| label("Asking textures.gg for more…").font(text::SMALL).color(p.faint))))
}

/// The facts about the mod, each over its value.
fn facts(app: &App, id: &tgg_mod::ModId, p: &Palette) -> Vec<El> {
    let mods = &app.mods;
    let installed = mods.local.get(id);
    let offered = mods.newest(id);
    let listed = mods.listings.get(id);
    let page = mods.details.get(id).and_then(Option::as_ref);
    let manifest: Option<&Manifest> = installed.map(|m| &m.manifest).or(offered.map(|entry| &entry.manifest));
    let mut facts = Vec::new();
    if let Some(manifest) = manifest {
        facts.push(fact("Version", manifest.version.to_string(), p));
    } else if let Some(summary) = listed {
        facts.push(fact("Version", format!("{} · not built for this tgg-melee yet", summary.version), p));
    }
    if let Some(entry) = offered {
        if installed.is_some_and(|m| m.manifest.version < entry.manifest.version) {
            facts.push(fact("Newest", entry.manifest.version.to_string(), p));
        }
        facts.push(fact("Download", size_text(entry.package.size), p));
        facts.push(fact("Netplay", if entry.netplay.counts() { "Players need the same version" } else { "Doesn't affect netplay" }.to_owned(), p));
    }
    if let Some(summary) = listed {
        facts.push(fact("Installs", count_text(summary.installs), p));
        if let Some(date) = summary.updated_at.get(..10) {
            facts.push(fact("Updated", date.to_owned(), p));
        }
    }
    let depends: Vec<String> = match manifest {
        Some(manifest) => manifest.depends.iter().map(|(id, range)| format!("{} {range}", mods.name_of(id))).collect(),
        None => page.map(|page| page.depends.iter().map(|m| format!("{} {}", mods.name_of(&m.id), m.range)).collect()).unwrap_or_default(),
    };
    if !depends.is_empty() {
        facts.push(fact("Needs", depends.join(", "), p));
    }
    let sets = mods.sets_with(id);
    if !sets.is_empty() {
        facts.push(fact("In collections", sets.join(", "), p));
    }
    if let Some(license) = manifest.and_then(|manifest| manifest.license.clone()).or_else(|| page.and_then(|page| page.license.clone())) {
        facts.push(fact("License", license, p));
    }
    facts
}

/// The mod's homepage and source, as links that open in the browser.
fn links(page: &ModDetails, p: &Palette) -> Option<El> {
    let links: Vec<El> = [("Homepage", page.homepage.clone()), ("Source", page.source.clone())].into_iter()
        .filter_map(|(name, url)| {
            let url = url.filter(|url| url.starts_with("https://"))?;
            Some(text_action(name, false, p).key(format!("mods/details/{}", name.to_lowercase())).on_press_tx(move |app: &mut App, tx| {
                if let Err(error) = tx.open_url(&url) {
                    app.tell(format!("Couldn't open {url}: {error}"), true);
                }
            }))
        })
        .collect();
    (!links.is_empty()).then(|| row().ml(-TEXT_ACTION_INSET).gap(space::XXS).children(links))
}

/// The mod's screenshots, the pane wide, as they arrive.
fn screenshots(mods: &super::Mods, page: &ModDetails, p: &Palette) -> El {
    col().key("mods/details/screenshots").gap(space::XS).children(page.screenshots.iter().map(|image| {
        let ratio = if image.height > 0 { image.width as f32 / image.height as f32 } else { 16.0 / 9.0 };
        let shot = el().w_full().aspect_ratio(ratio).radius(radius::CONTROL).clip_rounded(radius::CONTROL).fill(p.raise);
        match mods.images.get(&image.url) {
            Some(pixels) => shot.pixels_fit(pixels, rune::SurfaceFit::Fill),
            None => shot,
        }
    }))
}

/// A section of the pane: its label over what's in it.
fn section(name: &str, body: Vec<El>, p: &Palette) -> El {
    col().gap(space::SM).child(rule(p)).child(label(name.to_owned()).font(text::LABEL).color(p.faint)).children(body)
}

/// The README's first blocks, or all of it once the player asks.
fn readme_shown(readme: &str, all: bool, p: &Palette) -> Vec<El> {
    let mut blocks = markdown(readme, p);
    if !all && blocks.len() > README_BLOCKS {
        blocks.truncate(README_BLOCKS);
        blocks.push(more("Show more", "mods/details/readme/more", Expand::Readme, p));
    }
    blocks
}

/// The latest releases, or all of them once the player asks: each its
/// version and date over its changelog, a changelog cut to its first lines
/// until then.
fn releases(page: &ModDetails, all: bool, p: &Palette) -> Vec<El> {
    let shown = if all { page.releases.len() } else { RELEASES_SHOWN };
    let mut rows: Vec<El> = page.releases.iter().take(shown).map(|release| {
        let mut changelog = release.changelog.as_deref().map(|changelog| markdown(changelog, p)).unwrap_or_default();
        if !all {
            changelog.truncate(CHANGELOG_BLOCKS);
        }
        col().gap(space::TIGHT)
            .child(row().gap(space::XS).items_end()
                .child(label(release.version.to_string()).font(text::MONO_TEXT).color(p.ink))
                .child(label(release.date.get(..10).unwrap_or(&release.date).to_owned()).font(text::MONO_SMALL).color(p.faint)))
            .child(col().gap(space::XXS).children(changelog))
    }).collect();
    if page.releases.len() > shown {
        rows.push(more(&format!("Show all {} releases", page.releases.len()), "mods/details/releases/more", Expand::Releases, p));
    }
    rows
}

/// A text action that shows the rest of a part of the details.
fn more(words: &str, key: &str, part: Expand, p: &Palette) -> El {
    row().ml(-TEXT_ACTION_INSET).child(text_action(words, false, p).key(key.to_owned()).on_press(move |app: &mut App| actions::expand(app, part)))
}

/// Markdown read plainly: headings as strong lines, paragraphs and list
/// items as wrapped text, images and the source's relative links left out
/// (they point into a repository that isn't served).
fn markdown(source: &str, p: &Palette) -> Vec<El> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let flush = |paragraph: &mut String, blocks: &mut Vec<El>| {
        if !paragraph.trim().is_empty() {
            blocks.push(label(plain(paragraph.trim())).font(text::BODY).color(p.muted).wrap());
        }
        paragraph.clear();
    };
    for line in source.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("![") || trimmed.starts_with("```") {
            flush(&mut paragraph, &mut blocks);
        } else if let Some(heading) = trimmed.strip_prefix('#') {
            flush(&mut paragraph, &mut blocks);
            blocks.push(label(plain(heading.trim_start_matches('#').trim())).font(text::STRONG).color(p.ink).wrap());
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(label(format!("•  {}", plain(item))).font(text::BODY).color(p.muted).wrap());
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(trimmed);
        }
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

/// Markdown's inline marks dropped: emphasis, code ticks, and links kept as
/// their words.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|at| open + at) else { break };
        let Some(end) = rest[close..].find(')').map(|at| close + at) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "").replace("__", "")
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
        None if mods.newest(&id).is_none() => label("Not built for this tgg-melee yet; it installs once it is.").key("mods/details/unbuilt")
            .font(text::SMALL).color(p.muted).wrap(),
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
