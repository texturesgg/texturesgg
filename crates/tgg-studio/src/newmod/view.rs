//! The Start a mod sheet: what the player wants to make and what it's
//! called, then, once it's made, where it is, the command that runs it, and
//! how to open it and read the guide.

use super::actions;
use super::library::Tools;
use super::{CreatedMod, Kind, NewMod, Stage};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, button, button_off, callout, dialog, progress, spacer,
    text_action, text_field,
};
use rune::{El, col, row, text as label};

pub fn view(app: &App, p: &Palette) -> Option<El> {
    let new_mod = &app.new_mod;
    if !new_mod.open {
        return None;
    }
    let parts = match &new_mod.stage {
        Stage::Created(created) => created_parts(new_mod, created, p),
        _ => form_parts(new_mod, p),
    };
    Some(dialog(
        "newmod",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::close,
        p,
    ))
}

fn form_parts(new_mod: &NewMod, p: &Palette) -> Dialog {
    let mut body = col()
        .key("newmod/body")
        .px(space::LG)
        .pt(space::XS)
        .pb(space::SM)
        .gap(space::SM)
        .child(
            col()
                .key("newmod/kinds")
                .equal_columns(2)
                .gap(space::XS)
                .children(Kind::ALL.map(|kind| kind_tile(new_mod.kind == kind, kind, p))),
        )
        .child(
            col()
                .gap(space::XS)
                .child(label("Name").font(text::LABEL).color(p.faint))
                .child(
                    text_field(&new_mod.name, "Neutral spawns", p)
                        .key("newmod/name")
                        .w_full()
                        .on_edit(|app: &mut App| &mut app.new_mod.name),
                ),
        );
    match &new_mod.sdk {
        // The first time, the SDK downloads (a few megabytes).
        None => {
            body = body.child(
                col()
                    .key("newmod/sdk/fetching")
                    .gap(space::XS)
                    .child(
                        label("Downloading the tgg-melee SDK…")
                            .font(text::SMALL)
                            .color(p.muted),
                    )
                    .child(progress(
                        new_mod
                            .fetching
                            .as_ref()
                            .map_or(0.0, super::Fetching::fraction),
                        p,
                    )),
            );
        }
        Some(Err(why)) => {
            body = body.child(callout(
                label(format!("Couldn't download the SDK: {why}"))
                    .key("newmod/sdk/failed")
                    .font(text::SMALL)
                    .color(p.danger)
                    .wrap(),
                None,
                p,
            ));
        }
        Some(Ok(_)) => {}
    }
    if let Some(note) = native_note(new_mod, p) {
        body = body.child(note);
    }
    if let Stage::Failed(why) = &new_mod.stage {
        body = body.child(callout(
            label(format!("Couldn't start it: {why}"))
                .key("newmod/failed")
                .font(text::SMALL)
                .color(p.danger)
                .wrap(),
            None,
            p,
        ));
    }
    let create = match (&new_mod.stage, new_mod.ready()) {
        (Stage::Creating, _) => button_off("Starting…", p),
        (_, true) => button("Create", ButtonKind::Primary, p).on_press(actions::create),
        (_, false) => button_off("Create", p),
    };
    Dialog {
        title: "Start a mod".into(),
        line: "What do you want to make?".into(),
        body: Some(body),
        buttons: vec![
            button("Cancel", ButtonKind::Secondary, p)
                .key("newmod/cancel")
                .on_press(actions::close),
            spacer(),
            create.key("newmod/create"),
        ],
    }
}

/// One of the five things to make: its name, its line, and off-white while
/// it's chosen.
fn kind_tile(chosen: bool, kind: Kind, p: &Palette) -> El {
    col()
        .key(format!("newmod/kind/{}", kind.label()))
        .padding(space::SM)
        .gap(space::TIGHT)
        .fill(if chosen { p.selected } else { p.surface })
        .border(size::LINE, if chosen { p.accent } else { p.line })
        .radius(radius::CARD)
        .hover(p.hover)
        .cursor(rune::Cursor::Pointer)
        .role("radio")
        .checked(chosen)
        .focusable(true)
        .label(kind.label())
        .on_press(move |app: &mut App| actions::pick_kind(app, kind))
        .child(
            label(kind.label())
                .font(text::STRONG)
                .color(p.ink)
                .truncate(),
        )
        .child(label(kind.line()).font(text::SMALL).color(p.muted).wrap())
}

/// Native C builds with CMake; nothing else mentions it.
fn native_note(new_mod: &NewMod, p: &Palette) -> Option<El> {
    if new_mod.kind != Kind::Native {
        return None;
    }
    let missing = new_mod
        .tools
        .as_ref()
        .is_some_and(|tools: &Tools| tools.cmake.is_none());
    Some(
        label(if missing {
            "Native C mods need CMake, which isn't installed. Run tgg doctor for setup steps."
        } else {
            "Native C builds with CMake, one build for each platform."
        })
        .key("newmod/cmake")
        .font(text::SMALL)
        .color(if missing { p.danger } else { p.muted })
        .wrap(),
    )
}

/// After it's made: where it is, the command that runs it, and what to do
/// with it.
fn created_parts(new_mod: &NewMod, created: &CreatedMod, p: &Palette) -> Dialog {
    let script = actions::is_script(created);
    let line = if script {
        format!("It's in {} and it already runs.", created.dir.display())
    } else {
        format!("It's in {}.", created.dir.display())
    };
    let command = row()
        .key("newmod/dev")
        .px(space::SM)
        .py(space::XS)
        .gap(space::XS)
        .items_center()
        .fill(p.sunk)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
        .child(
            label(actions::dev_command(created))
                .font(text::MONO_SMALL)
                .color(p.ink)
                .wrap()
                .grow(1.0)
                .min_w(0.0),
        )
        .child(
            text_action("Copy", false, p)
                .key("newmod/copy")
                .on_press_tx(actions::copy_dev),
        );
    let note = if script {
        "Starts the game with your mod. It reloads when you save."
    } else {
        "Builds it, then starts the game with your mod. It rebuilds when you save."
    };
    let body = col()
        .key("newmod/created")
        .px(space::LG)
        .pt(space::XS)
        .pb(space::SM)
        .gap(space::SM)
        .child(command)
        .child(label(note).font(text::SMALL).color(p.muted).wrap());
    Dialog {
        title: format!("{} is ready", new_mod.title()),
        line,
        body: Some(body),
        buttons: vec![
            button("Open in your editor", ButtonKind::Primary, p)
                .key("newmod/open")
                .on_press_tx(actions::open_in_editor),
            button("Read the guide", ButtonKind::Secondary, p)
                .key("newmod/guide")
                .on_press_tx(actions::open_guide),
            spacer(),
            button("Done", ButtonKind::Secondary, p)
                .key("newmod/done")
                .on_press(actions::close),
        ],
    }
}
