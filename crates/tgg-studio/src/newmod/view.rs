//! The New mod dialog: its name, id, folder and example in a form, a line
//! on the tools that build mods; once it's made, where it is and the
//! commands that build and run it.

use super::library::{CreatedMod, Repository, Tools};
use super::{NewMod, Stage, actions, id_from};
use crate::state::App;
use crate::ui::tokens::{Palette, radius, size, space, text};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, button_off, callout, dialog, rule,
    select, select_item, select_menu, setting_row, setting_row_with, spacer, text_action,
    text_field,
};
use rune::{El, col, row, text as label};

/// The form's fields and select, at their widest.
const FIELD: f32 = 300.0;

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
    let rows = [
        name(new_mod, p),
        id(new_mod, p),
        folder(new_mod, p),
        example(new_mod, p),
    ];
    let mut body = col().children(rows.into_iter().enumerate().flat_map(|(index, row)| {
        let line = (index > 0).then(|| rule(p));
        line.into_iter().chain([row])
    }));
    let notes: Vec<El> = tools_note(new_mod.tools.as_ref(), p)
        .into_iter()
        .chain(match &new_mod.stage {
            Stage::Failed(why) => Some(
                callout(
                    label(format!("Couldn't start it: {why}"))
                        .font(text::SMALL)
                        .color(p.danger)
                        .wrap(),
                    None,
                    p,
                )
                .key("newmod/failed"),
            ),
            _ => None,
        })
        .collect();
    if !notes.is_empty() {
        body = body.child(
            col()
                .px(space::LG)
                .pt(space::XS)
                .pb(space::SM)
                .gap(space::XS)
                .children(notes),
        );
    }
    let create = match (&new_mod.stage, actions::can_create(new_mod)) {
        (Stage::Creating, _) => button_off("Starting…", p),
        (_, true) => button("Start it", ButtonKind::Primary, p).on_press(actions::create),
        (_, false) => button_off("Start it", p),
    };
    Dialog {
        title: "New mod".into(),
        line: "A code mod of your own, from tgg-melee's SDK, in a folder on this computer.".into(),
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

fn name(new_mod: &NewMod, p: &Palette) -> El {
    setting_row(
        "Name",
        "What players see it called.",
        text_field(&new_mod.name, "Neutral spawns", p)
            .key("newmod/name")
            .w(FIELD)
            .on_edit(|app: &mut App| &mut app.new_mod.name),
        p,
    )
    .key("newmod/name/row")
}

fn id(new_mod: &NewMod, p: &Palette) -> El {
    let typed = !new_mod.id.text.trim().is_empty();
    let line = if typed && new_mod.mod_id().is_none() {
        label("1 to 64 of a–z, 0–9, '.', '-' and '_'.")
            .key("newmod/id/invalid")
            .font(text::SMALL)
            .color(p.danger)
            .wrap()
    } else {
        label("Its address on textures.gg, for good.")
            .font(text::SMALL)
            .color(p.muted)
            .wrap()
    };
    let suggested = id_from(&new_mod.name.text);
    let placeholder = if suggested.is_empty() {
        "neutral-spawns"
    } else {
        &suggested
    };
    let field = text_field(&new_mod.id, placeholder, p)
        .key("newmod/id")
        .w(FIELD)
        .on_edit(|app: &mut App| &mut app.new_mod.id);
    setting_row_with("Id", line, field, p).key("newmod/id/row")
}

fn folder(new_mod: &NewMod, p: &Palette) -> El {
    let path = col()
        .w(FIELD)
        .items_end()
        .gap(space::TIGHT)
        .child(
            label(new_mod.folder().display().to_string())
                .key("newmod/folder")
                .font(text::MONO_SMALL)
                .color(p.ink)
                .truncate()
                .min_w(0.0),
        )
        .child(
            row().mr(-TEXT_ACTION_INSET).child(
                text_action("Choose…", false, p)
                    .key("newmod/choose-folder")
                    .on_press_tx(actions::choose_folder),
            ),
        );
    setting_row(
        "Folder",
        "Its own folder, named by its id. It has to be new or empty.",
        path,
        p,
    )
    .key("newmod/folder/row")
}

fn example(new_mod: &NewMod, p: &Palette) -> El {
    let control = match &new_mod.examples {
        None => label("Reading the SDK…")
            .font(text::SMALL)
            .color(p.muted)
            .into(),
        Some(Err(why)) => label(format!("Couldn't read the SDK: {why}"))
            .key("newmod/examples/failed")
            .font(text::SMALL)
            .color(p.danger)
            .wrap()
            .w(FIELD),
        Some(Ok(_)) => select(&new_mod.example, new_mod.menu, p)
            .key("newmod/example")
            .label("Start from")
            .w(FIELD)
            .on_press(actions::toggle_menu),
    };
    setting_row(
        "Start from",
        "The SDK's template, or one of its examples to learn from.",
        control,
        p,
    )
    .key("newmod/example/row")
}

/// Which tools that build mods are missing, if any; a quiet line when
/// they're all there.
fn tools_note(tools: Option<&Tools>, p: &Palette) -> Option<El> {
    let tools = tools?;
    let missing: Vec<&str> = [
        ("GCC", tools.gcc.is_none()),
        ("CMake", tools.cmake.is_none()),
        ("git", tools.git.is_none()),
    ]
    .into_iter()
    .filter(|(_, missing)| *missing)
    .map(|(name, _)| name)
    .collect();
    if missing.is_empty() {
        return Some(
            label("GCC, CMake and git are installed, so it's ready to build.")
                .key("newmod/tools")
                .font(text::SMALL)
                .color(p.muted),
        );
    }
    Some(
        callout(
            label(format!(
                "Building mods needs {} too; `tgg doctor` says how to get {them}. The mod starts without {them}.",
                and_list(&missing),
                them = if missing.len() == 1 { "it" } else { "them" },
            ))
            .font(text::SMALL)
            .color(p.muted)
            .wrap(),
            None,
            p,
        )
        .key("newmod/tools"),
    )
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn created_parts(new_mod: &NewMod, created: &CreatedMod, p: &Palette) -> Dialog {
    let repository = match &created.repository {
        Repository::Committed => Some("A git repository holds it, its first commit made."),
        Repository::AlreadyThere => Some("It's in the git repository that was already there."),
        Repository::NoIdentity => Some(
            "A git repository holds it; tell git your name and email to commit (git config --global user.name, user.email).",
        ),
        Repository::NoGit => Some("git isn't installed, so it has no repository yet."),
        Repository::Failed(_) => Some("Its git repository couldn't be started."),
    };
    let commands = col()
        .key("newmod/commands")
        .px(space::SM)
        .py(space::XS)
        .fill(p.sunk)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
        .child(
            label(actions::commands(created))
                .font(text::MONO_SMALL)
                .color(p.ink)
                .wrap(),
        );
    let body = col()
        .px(space::LG)
        .py(space::MD)
        .gap(space::SM)
        .child(
            label(format!(
                "Made from the SDK's {} in {}.",
                created.example,
                created.dir.display()
            ))
            .key("newmod/created")
            .font(text::BODY)
            .color(p.ink)
            .wrap(),
        )
        .children(repository.map(|words| {
            label(words)
                .key("newmod/repository")
                .font(text::SMALL)
                .color(p.muted)
                .wrap()
        }))
        .child(
            label("To build it and run it in the game, from its folder:")
                .font(text::SMALL)
                .color(p.muted),
        )
        .child(commands);
    let name = match new_mod.name.text.trim() {
        "" => new_mod.id_text(),
        name => name.to_owned(),
    };
    Dialog {
        title: format!("{name} is started"),
        line: "Edit it in your own editor; the tgg command builds it and runs it.".into(),
        body: Some(body),
        buttons: vec![
            button("Copy commands", ButtonKind::Secondary, p)
                .key("newmod/copy")
                .on_press_tx(actions::copy_commands),
            button("Open folder", ButtonKind::Secondary, p)
                .key("newmod/open-folder")
                .on_press_tx(actions::open_folder),
            button("Guide", ButtonKind::Secondary, p)
                .key("newmod/guide")
                .on_press_tx(actions::open_guide),
            spacer(),
            button("Done", ButtonKind::Primary, p)
                .key("newmod/done")
                .on_press(actions::close),
        ],
    }
}

/// The examples' menu, at the window's root over the dialog.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    let new_mod = &app.new_mod;
    if !new_mod.open || !new_mod.menu {
        return None;
    }
    let Some(Ok(examples)) = &new_mod.examples else {
        return None;
    };
    let items = examples
        .iter()
        .map(|example| {
            let owned = example.clone();
            select_item(example, *example == new_mod.example, p)
                .key(format!("newmod/example/{example}"))
                .on_press(move |app: &mut App| actions::pick_example(app, &owned))
        })
        .collect();
    Some(select_menu("newmod/example", items, actions::close_menu, p))
}
