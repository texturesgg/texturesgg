//! The Mods page's sheets: New set (a name and the textures.gg sets to
//! build on, Online play ticked), Rename, and Build on another set. Each is
//! modal, over the whole window: Escape or a click outside closes it.

use super::{NewSet, actions, current_set, mod_count};
use crate::state::App;
use crate::ui::tokens::{Palette, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, button, button_off, dialog, setting_row, switch, text_field,
};
use rune::{El, col, text as label};
use tgg_mod::Set;
use tgg_mod::sets::{self, MySet};

/// The sheet open, if any. Only one is at a time.
pub fn view(app: &App, p: &Palette) -> Option<El> {
    new_set(app, p)
        .or_else(|| rename(app, p))
        .or_else(|| build_on(app, p))
}

/// New set: what it's called and the sets it builds on.
fn new_set(app: &App, p: &Palette) -> Option<El> {
    let sheet = app.mods.new_set.as_ref()?;
    let choices: Vec<El> = app
        .mods
        .sets
        .iter()
        .filter(|set| set.can_build_on())
        .map(|set| build_on_row(sheet, set, "mods/new-set", p))
        .collect();
    let has_choices = !choices.is_empty();
    let mut body = col()
        .key("mods/new-set/body")
        .pt(space::XS)
        .pb(space::SM)
        .gap(space::XS)
        .child(
            text_field(&sheet.name, "Friday night", p)
                .key("mods/new-set/name")
                .w_full()
                .on_edit(|app: &mut App| &mut app.mods.new_set.as_mut().expect("open").name),
        )
        .child(
            label("Build on")
                .font(text::LABEL)
                .color(p.faint)
                .pt(space::SM),
        )
        .children(choices);
    if has_choices {
        body = body.child(
            label("Leave everything off to start from an empty set.")
                .key("mods/new-set/blank-hint")
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        );
    } else {
        body = body.child(
            label("Starts from an empty set. Add mods once it's created.")
                .key("mods/new-set/none")
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        );
    }
    let create = if sheet.named() {
        button("Create", ButtonKind::Primary, p)
            .key("mods/new-set/create")
            .on_press(actions::create_new_set)
    } else {
        button_off("Create", p).key("mods/new-set/create")
    };
    Some(dialog(
        "mods/new-set",
        "dialog",
        DialogWidth::Narrow,
        Dialog {
            title: "New set".into(),
            line: "Give it a name and pick what it builds on.".into(),
            body: Some(body),
            buttons: vec![
                button("Cancel", ButtonKind::Secondary, p)
                    .key("mods/new-set/cancel")
                    .on_press(actions::close_new_set),
                crate::ui::spacer(),
                create,
            ],
        },
        actions::close_new_set,
        p,
    ))
}

/// A textures.gg set, on or off to build on.
fn build_on_row(sheet: &NewSet, set: &Set, key: &str, p: &Palette) -> El {
    let on = sheet.builds_on.iter().any(|have| have == &set.name);
    let link = set.name.clone();
    setting_row(
        &set.title,
        mod_count(set.mods.len()),
        switch(&format!("{key}/{}", set.name), on, p)
            .label(set.title.clone())
            .on_press(move |app: &mut App| actions::toggle_new_set_set(app, &link)),
        p,
    )
    .key(format!("{key}/row/{}", set.name))
}

/// Rename: the set's name, editable.
fn rename(app: &App, p: &Palette) -> Option<El> {
    let name = app.mods.rename.as_ref()?;
    let body = col()
        .key("mods/rename/body")
        .pt(space::XS)
        .pb(space::SM)
        .child(
            text_field(name, "Friday night", p)
                .key("mods/rename/name")
                .w_full()
                .on_edit(|app: &mut App| app.mods.rename.as_mut().expect("open")),
        );
    Some(dialog(
        "mods/rename",
        "dialog",
        DialogWidth::Narrow,
        Dialog {
            title: "Rename set".into(),
            line: String::new(),
            body: Some(body),
            buttons: vec![
                button("Cancel", ButtonKind::Secondary, p)
                    .key("mods/rename/cancel")
                    .on_press(actions::close_rename),
                crate::ui::spacer(),
                button("Rename", ButtonKind::Primary, p)
                    .key("mods/rename/confirm")
                    .on_press(actions::rename),
            ],
        },
        actions::close_rename,
        p,
    ))
}

/// Build on another set: the textures.gg sets the shown one can add.
fn build_on(app: &App, p: &Palette) -> Option<El> {
    if !app.mods.building_on {
        return None;
    }
    let set = current_set(app)?;
    let rows = candidates(app, set, p);
    let mut body = col()
        .key("mods/build-on/body")
        .pt(space::XS)
        .pb(space::SM)
        .children(rows);
    if set.builds_on().count() >= sets::MOST_SET_LINES {
        body = body.child(
            label("It builds on four sets already.")
                .key("mods/build-on/full")
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        );
    }
    Some(dialog(
        "mods/build-on",
        "dialog",
        DialogWidth::Wide,
        Dialog {
            title: "Build on another set".into(),
            line: "You get its mods, and yours win where they differ.".into(),
            body: Some(body),
            buttons: vec![
                button("Cancel", ButtonKind::Secondary, p)
                    .key("mods/build-on/cancel")
                    .on_press(actions::close_build_on),
                crate::ui::spacer(),
            ],
        },
        actions::close_build_on,
        p,
    ))
}

/// The textures.gg sets the shown set doesn't build on yet.
fn candidates(app: &App, set: &MySet, p: &Palette) -> Vec<El> {
    let full = set.builds_on().count() >= sets::MOST_SET_LINES;
    let rows: Vec<El> = app
        .mods
        .sets
        .iter()
        .filter(|candidate| candidate.can_build_on())
        .filter(|candidate| !set.builds_on().any(|have| have == candidate.name))
        .map(|candidate| {
            let link = candidate.name.clone();
            let build = if full {
                button_off("Build on", p)
            } else {
                button("Build on", ButtonKind::Secondary, p)
                    .on_press(move |app: &mut App| actions::build_on(app, &link))
            };
            setting_row(
                &candidate.title,
                candidate.description.clone(),
                build.key(format!("mods/build-on/{}", candidate.name)),
                p,
            )
            .key(format!("mods/build-on/row/{}", candidate.name))
        })
        .collect();
    if rows.is_empty() {
        return vec![
            label("textures.gg has no other set to build on yet.")
                .key("mods/build-on/none")
                .font(text::SMALL)
                .color(p.muted)
                .wrap(),
        ];
    }
    rows
}
