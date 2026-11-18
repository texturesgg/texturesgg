//! The set the page shows, on the right: its name (pressing it picks
//! another set, or makes a new one), Use, and its mods in the groups its
//! plan gives (from each set it builds on, added, left out, skipped with
//! the reason). A mod's row shows what takes it out while the pointer's on
//! it. Installed mods are the mods folder, so its rows carry switches
//! instead. A game running from the set makes it read-only. The panel is
//! as tall as its mods, up to the page.

use super::{Menu, actions, current_set};
use crate::sets::Planned;
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Icon, Tone, badge, button_small, icon, icon_button, icon_tile, rule, switch,
};
use rune::{El, ScrollSpec, col, row, text as label};
use tgg_mod::sets::{Group, MySet, SetMod};

/// A mod's icon in a row here: smaller than a thumbnail, as the panel's
/// rows are close together.
const ICON: f32 = 22.0;

/// The set showing, as a panel on the page's right.
pub fn view(app: &App, p: &Palette) -> El {
    let inside = match current_set(app) {
        None => none(p),
        Some(set) => {
            let running = app.sets.running(&set.key);
            let body = col()
                .key("mods/set/body")
                .gap(space::XXS)
                .children(groups(app, &set.key, running, p));
            col()
                .shrink()
                .min_h(0.0)
                .child(top(app, set, p).px(space::MD).py(space::SM))
                .child(rule(p))
                .child(
                    col()
                        .shrink()
                        .min_h(0.0)
                        .scroll(ScrollSpec {
                            stick_bottom: false,
                        })
                        .child(col().px(space::MD).py(space::SM).child(body)),
                )
        }
    };
    // As tall as what's in it, and no taller than the page.
    col()
        .grow(1.0)
        .min_w(size::DETAILS_MIN)
        .max_w(size::DETAILS_MAX)
        .min_h(0.0)
        .child(card(inside, p).shrink().min_h(0.0))
}

/// The set showing, as a bar over the list when the window's too narrow
/// for the panel: its name, Use, and what else to do to it. Its mods show
/// as Added in the list.
pub fn bar(app: &App, p: &Palette) -> El {
    let inside = match current_set(app) {
        None => none(p),
        Some(set) => top(app, set, p).px(space::MD).py(space::SM),
    };
    card(inside, p)
}

/// No set to show: what one is, and New set.
fn none(p: &Palette) -> El {
    col()
        .key("mods/set/none")
        .padding(space::MD)
        .gap(space::SM)
        .child(
            label("A set is the mods you play with. Make one to start adding to it.")
                .font(text::BODY)
                .color(p.muted)
                .wrap(),
        )
        .child(
            row().child(
                button_small("New set", ButtonKind::Primary, p)
                    .key("mods/new-set")
                    .on_press(actions::open_new_set),
            ),
        )
}

/// The set's card.
fn card(inside: El, p: &Palette) -> El {
    col()
        .key("mods/set")
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CARD)
        .clip_rounded(radius::CARD)
        .child(inside)
}

/// The set's head: its name, which picks another set, Using or Use, and
/// More for one of the player's; then, while its game runs, that changes
/// wait, or, for a textures.gg set, Make your own.
fn top(app: &App, set: &MySet, p: &Palette) -> El {
    let running = app.sets.running(&set.key);
    let more = (!set.locked && !set.is_installed() && !running).then(|| {
        icon_button(icon(Icon::More, p.muted), "More", p)
            .key("mods/set/more")
            .on_press(|app: &mut App| actions::toggle_menu(app, Menu::More))
    });
    let head = row()
        .gap(space::XS)
        .items_center()
        .child(picker(app, set, p))
        .child(if app.sets.active == set.key {
            badge("Using", Tone::Mine, p)
        } else {
            let key = set.key.clone();
            button_small("Use", ButtonKind::Primary, p)
                .key("mods/set/use")
                .on_press(move |app: &mut App| actions::use_set(app, &key))
        })
        .children(more);
    let note = running.then(|| {
        label(format!(
            "{} is running. Changes apply next time you play.",
            set.name
        ))
        .key("mods/set/running")
        .font(text::SMALL)
        .color(p.muted)
        .wrap()
    });
    let own = (set.locked && !running).then(|| {
        let key = set.key.clone();
        row().key("mods/set/actions").child(
            button_small("Make your own", ButtonKind::Secondary, p)
                .key("mods/set/make-own")
                .on_press(move |app: &mut App| actions::make_own(app, &key)),
        )
    });
    col()
        .gap(space::SM)
        .child(head)
        .children(note)
        .children(own)
}

/// The set's name, which picks another set, and in a few words what it is.
fn picker(app: &App, set: &MySet, p: &Palette) -> El {
    col()
        .key("mods/set/pick")
        .grow(1.0)
        .min_w(0.0)
        .gap(space::TIGHT)
        .px(space::XS)
        .py(space::XXS)
        .ml(-space::XS)
        .radius(radius::CONTROL)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("combobox")
        .checked(app.mods.menu == Some(Menu::Sets))
        .focusable(true)
        .label(set.name.clone())
        .on_press(|app: &mut App| actions::toggle_menu(app, Menu::Sets))
        .child(
            row()
                .gap(space::XXS)
                .items_center()
                .child(
                    label(set.name.clone())
                        .key("mods/set/name")
                        .font(text::H3)
                        .color(p.ink)
                        .truncate()
                        .min_w(0.0),
                )
                .child(icon(Icon::Chevron, p.muted)),
        )
        .child(
            label(kind(app, set))
                .font(text::SMALL)
                .color(p.muted)
                .truncate()
                .min_w(0.0),
        )
}

/// What the set is, under its name.
fn kind(app: &App, set: &MySet) -> String {
    if set.is_installed() {
        "Your mods folder".to_owned()
    } else if set.locked {
        "By textures.gg, kept up to date".to_owned()
    } else {
        match crate::sets::summary(app, set) {
            summary if summary.is_empty() => "Yours".to_owned(),
            summary => format!("Yours · {summary}"),
        }
    }
}

/// Its mods, group by group.
fn groups(app: &App, key: &str, running: bool, p: &Palette) -> Vec<El> {
    let Some(set) = app.sets.get(key) else {
        return Vec::new();
    };
    if set.is_installed() {
        return installed(app, key, running, p);
    }
    let editable = !set.locked && !running;
    match app.sets.plans.get(key) {
        Some(Planned::Ready(plan)) => {
            let mut blocks: Vec<(Group, Vec<&SetMod>)> = Vec::new();
            for m in plan.mods.iter().filter(|m| m.skipped.is_none()) {
                match blocks.last_mut() {
                    Some((group, rows)) if *group == m.group => rows.push(m),
                    _ => blocks.push((m.group.clone(), vec![m])),
                }
            }
            let mut out: Vec<El> = blocks
                .into_iter()
                .flat_map(|(group, rows)| {
                    std::iter::once(section(&title(&group), rows.len(), p))
                        .chain(rows.into_iter().map(move |m| row_for(app, m, editable, p)))
                })
                .collect();
            let skipped: Vec<&SetMod> = plan.skipped().collect();
            if !skipped.is_empty() {
                out.push(section("Skipped", skipped.len(), p));
                out.extend(skipped.into_iter().map(|m| skipped_row(app, m, p)));
            }
            out
        }
        Some(Planned::Failed(why)) => vec![
            label(why.clone())
                .key("mods/set/failed")
                .font(text::SMALL)
                .color(p.danger)
                .wrap(),
        ],
        Some(Planned::Working) | None => vec![
            label("Working out its mods…")
                .key("mods/set/working")
                .font(text::SMALL)
                .color(p.faint),
        ],
    }
}

/// A group's words, as the panel's rows head it.
fn title(group: &Group) -> String {
    match group {
        Group::FromSet { title, .. } => format!("From {title}"),
        Group::Added => "Added".to_owned(),
        Group::LeftOut => "Left out".to_owned(),
        Group::Skins => "Skins".to_owned(),
        Group::Needed => "Needed".to_owned(),
    }
}

/// A group's label, tight over its rows, and how many it has.
fn section(words: &str, count: usize, p: &Palette) -> El {
    row()
        .pt(space::XS)
        .pb(space::TIGHT)
        .gap(space::XS)
        .items_center()
        .child(
            label(words.to_owned())
                .font(text::LABEL)
                .color(p.faint)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(
            label(count.to_string())
                .font(text::MONO_SMALL)
                .color(p.faint),
        )
}

/// A mod's row in the panel: its icon and name, and what's at its end.
fn mod_line(app: &App, id: &tgg_mod::ModId, faint: bool, p: &Palette) -> El {
    row()
        .key(format!("mods/set/{id}"))
        .h(size::ROW)
        .gap(space::XS)
        .items_center()
        .child(icon_tile(app.mods.icon(id), ICON, p))
        .child(
            label(app.mods.name_of(id))
                .font(text::BODY)
                .color(if faint { p.faint } else { p.ink })
                .truncate()
                .grow(1.0)
                .min_w(0.0),
        )
}

/// A mod of a set, and what the player can do with its line: take it out
/// or leave it out, shown while the pointer's on the row; put it back.
fn row_for(app: &App, m: &SetMod, editable: bool, p: &Palette) -> El {
    let id = &m.id;
    let name = app.mods.name_of(id);
    let left_out = m.group == Group::LeftOut;
    let end = if !editable {
        None
    } else {
        let gone = id.clone();
        match &m.group {
            Group::LeftOut => Some(
                icon_button(icon(Icon::Plus, p.muted), &format!("Put {name} back"), p)
                    .key(format!("mods/set/{id}/put-back"))
                    .on_press(move |app: &mut App| actions::leave_out(app, &gone, false)),
            ),
            Group::Added => Some(
                icon_button(icon(Icon::Close, p.muted), &format!("Take {name} out"), p)
                    .key(format!("mods/set/{id}/take-out"))
                    .group_opacity(0.0, 1.0)
                    .on_press(move |app: &mut App| actions::take_out(app, &gone)),
            ),
            Group::FromSet { .. } => Some(
                icon_button(icon(Icon::Close, p.muted), &format!("Leave {name} out"), p)
                    .key(format!("mods/set/{id}/leave-out"))
                    .group_opacity(0.0, 1.0)
                    .on_press(move |app: &mut App| actions::leave_out(app, &gone, true)),
            ),
            Group::Skins | Group::Needed => None,
        }
    };
    mod_line(app, id, left_out, p).hover_group().children(end)
}

/// Installed mods: the mods folder, each with the switch that turns it on
/// or off and the update it has.
fn installed(app: &App, key: &str, running: bool, p: &Palette) -> Vec<El> {
    let Some(set) = app.sets.get(key) else {
        return Vec::new();
    };
    let mut out = vec![section("In your mods folder", set.lines.len(), p)];
    for line in &set.lines {
        let tgg_mod::sets::Line::Mod(line) = line else {
            continue;
        };
        let id = &line.id;
        let on = !line.left_out;
        let update = app.mods.local.get(id).and_then(|m| m.update.clone());
        let end: Vec<El> = if running {
            vec![
                label(if on { "On" } else { "Off" })
                    .font(text::SMALL)
                    .color(p.muted),
            ]
        } else {
            let update = update.filter(|_| app.mods.work.is_none()).map(|version| {
                let id = id.clone();
                crate::ui::text_action(&format!("Update to {version}"), false, p)
                    .key(format!("mods/set/{id}/update"))
                    .on_press(move |app: &mut App| {
                        crate::mods::actions::review_mod_update(app, &id);
                    })
            });
            let toggled = id.clone();
            update
                .into_iter()
                .chain(std::iter::once(
                    switch(&format!("mods/set/{id}/switch"), on, p)
                        .label(app.mods.name_of(id))
                        .on_press(move |app: &mut App| actions::set_on(app, &toggled, !on)),
                ))
                .collect()
        };
        out.push(mod_line(app, id, !on, p).children(end));
    }
    out
}

/// A mod the set skips, and why.
fn skipped_row(app: &App, m: &SetMod, p: &Palette) -> El {
    let id = &m.id;
    row()
        .key(format!("mods/set/{id}/skipped"))
        .py(space::XXS)
        .gap(space::XS)
        .items_center()
        .child(icon_tile(app.mods.icon(id), ICON, p))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(space::TIGHT)
                .child(
                    label(app.mods.name_of(id))
                        .font(text::BODY)
                        .color(p.muted)
                        .truncate(),
                )
                .child(
                    label(m.skipped.clone().unwrap_or_default())
                        .font(text::SMALL)
                        .color(p.danger)
                        .wrap(),
                ),
        )
}
