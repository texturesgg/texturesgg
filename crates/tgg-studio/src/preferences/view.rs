//! The Settings dialog: a row per setting, its name and what it does on
//! the left, its control on the right, and help at the end.

use super::{Menu, actions};
use crate::mods::Disc;
use crate::state::App;
use crate::ui::tokens::{Palette, space};
use crate::ui::{
    ButtonKind, Dialog, DialogWidth, TEXT_ACTION_INSET, button, dialog, rule, select, select_item,
    select_menu, select_rule, spacer, switch, text_action, text_field,
};
use rune::{El, col, row};
use tgg_app::settings::Appearance;

/// The image editor's choices and field, at their widest.
const CHOICES: f32 = 300.0;

pub fn view(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    let rows = [
        image_editor(app, p),
        appearance(app, p),
        updates(app, p),
        game(app, p),
        disc(app, p),
        help(p),
    ];
    let body = col().children(rows.into_iter().enumerate().flat_map(|(index, row)| {
        let line = (index > 0).then(|| rule(p));
        line.into_iter().chain([row])
    }));
    let parts = Dialog {
        title: "Settings".into(),
        line: "Kept as you change them.".into(),
        body: Some(body),
        buttons: vec![
            spacer(),
            button("Done", ButtonKind::Secondary, p)
                .key("settings/done")
                .on_press(actions::close),
        ],
    };
    Some(dialog(
        "settings",
        "dialog",
        DialogWidth::Wide,
        parts,
        actions::close,
        p,
    ))
}

/// A setting: its name and a line saying what it does, then its control.
fn setting(key: &str, name: &str, line: impl Into<String>, control: El, p: &Palette) -> El {
    crate::ui::setting_row(name, line, control, p).key(key.to_owned())
}

/// The image editor: the system's default, an app on this computer that
/// opens PNGs, or another (a command typed, or a program browsed for), in a
/// select; another's field and Browse under it.
fn image_editor(app: &App, p: &Palette) -> El {
    let preferences = &app.preferences;
    let open = preferences.menu == Some(Menu::ImageEditor);
    let value = editor_value(app);
    let field = preferences.other.then(|| {
        col()
            .gap(space::XXS)
            .child(
                text_field(&preferences.editor, "A program, such as aseprite", p)
                    .key("settings/image-editor/field")
                    .on_edit(|app: &mut App| &mut app.preferences.editor),
            )
            .child(
                row().ml(-TEXT_ACTION_INSET).child(
                    text_action("Browse…", false, p)
                        .key("settings/image-editor/browse")
                        .on_press_tx(actions::browse_editor),
                ),
            )
    });
    let control = col()
        .w(CHOICES)
        .gap(space::XS)
        .child(
            select(&value, open, p)
                .key("settings/image-editor")
                .label("Image editor")
                .on_press(|app: &mut App| actions::toggle_menu(app, Menu::ImageEditor)),
        )
        .children(field);
    setting(
        "settings/image-editor/row",
        "Image editor",
        "Where Edit in your image editor opens a texture.",
        control,
        p,
    )
}

/// What the image editor select says: the app's name, the system's
/// default, or Other.
fn editor_value(app: &App) -> String {
    if app.preferences.other {
        return "Other…".into();
    }
    match app.settings.external_editor.as_deref() {
        None => "System default".into(),
        Some(command) => app
            .preferences
            .apps
            .iter()
            .find(|known| known.command == command)
            .map_or_else(|| command.to_owned(), |known| known.name.clone()),
    }
}

fn appearance(app: &App, p: &Palette) -> El {
    let open = app.preferences.menu == Some(Menu::Appearance);
    let value = appearance_name(app.settings.appearance());
    setting(
        "settings/appearance/row",
        "Appearance",
        "How the app looks.",
        select(value, open, p)
            .key("settings/appearance")
            .label("Appearance")
            .on_press(|app: &mut App| actions::toggle_menu(app, Menu::Appearance)),
        p,
    )
}

/// The palettes, by the name they show.
const APPEARANCES: [(Appearance, &str); 2] = [
    (Appearance::Gallery, "Gallery (dark)"),
    (Appearance::Paper, "Paper (light)"),
];

fn appearance_name(appearance: Appearance) -> &'static str {
    APPEARANCES
        .iter()
        .find(|(known, _)| *known == appearance)
        .map_or("Gallery (dark)", |(_, name)| name)
}

/// The open select's menu, at the window's root over Settings.
pub fn menu(app: &App, p: &Palette) -> Option<El> {
    if !app.preferences.open {
        return None;
    }
    match app.preferences.menu? {
        Menu::ImageEditor => {
            let preferences = &app.preferences;
            let kept = app.settings.external_editor.as_deref();
            let mut items = vec![
                select_item("System default", kept.is_none() && !preferences.other, p)
                    .key("settings/image-editor/default")
                    .on_press(|app: &mut App| actions::choose_editor(app, None)),
            ];
            items.extend(preferences.apps.iter().enumerate().map(|(index, known)| {
                let command = known.command.clone();
                select_item(
                    &known.name,
                    !preferences.other && kept == Some(command.as_str()),
                    p,
                )
                .key(format!("settings/image-editor/app/{index}"))
                .on_press(move |app: &mut App| actions::choose_editor(app, Some(command.clone())))
            }));
            items.push(select_rule(p));
            items.push(
                select_item("Other…", preferences.other, p)
                    .key("settings/image-editor/other")
                    .on_press(actions::choose_other),
            );
            Some(select_menu(
                "settings/image-editor",
                items,
                actions::close_menu,
                p,
            ))
        }
        Menu::Appearance => {
            let now = app.settings.appearance();
            let items = APPEARANCES
                .iter()
                .map(|(appearance, name)| {
                    let appearance = *appearance;
                    select_item(name, now == appearance, p)
                        .key(format!(
                            "settings/appearance/{}",
                            name.split(' ').next().unwrap_or(name).to_lowercase()
                        ))
                        .on_press(move |app: &mut App| actions::set_appearance(app, appearance))
                })
                .collect();
            Some(select_menu(
                "settings/appearance",
                items,
                actions::close_menu,
                p,
            ))
        }
    }
}

fn updates(app: &App, p: &Palette) -> El {
    let on = app.settings.check_for_updates.unwrap_or(true);
    setting(
        "settings/updates",
        "Check for updates",
        "Once at launch, the app asks textures.gg whether a newer version is out. Nothing downloads on its own.",
        switch("settings/updates/switch", on, p)
            .label("Check for updates")
            .on_press(move |app: &mut App| actions::set_update_check(app, !on)),
        p,
    )
}

/// Slippi's ISO: the game skins install into and Slippi's Play opens, and
/// choosing another. Without Slippi's Dolphin, where to get it.
fn game(app: &App, p: &Palette) -> El {
    let path = app.game.as_ref().map(|references| references.game().path());
    let file = file_name(path);
    let line = if app.slippi.dolphin.is_some() {
        format!("Skins install into {file}, and Play beside Slippi opens it in Slippi's Dolphin.")
    } else {
        format!(
            "Skins install into {file}. Play beside Slippi shows once Slippi Launcher is installed (slippi.gg)."
        )
    };
    // The file's name says which; its whole path is a hover away.
    setting(
        "settings/game",
        "Slippi ISO",
        line,
        text_action("Choose another…", false, p)
            .key("settings/choose-game")
            .on_press_tx(actions::choose_game),
        p,
    )
    .help(
        path.map(|path| path.display().to_string())
            .unwrap_or_default(),
    )
}

/// tgg-melee's disc: an unmodified one, which may be the Slippi ISO while
/// nothing's installed into it, and choosing another.
fn disc(app: &App, p: &Palette) -> El {
    let found = match &app.mods.disc {
        Disc::Found(path) => Some(path.as_path()),
        _ => None,
    };
    let same =
        found.is_some() && found == app.game.as_ref().map(|references| references.game().path());
    let line = match &app.mods.disc {
        Disc::Found(_) if same => {
            "tgg-melee plays your Slippi ISO too, while it's unmodified. Netplay and replays need the game files as they shipped.".to_owned()
        }
        Disc::Found(_) => format!(
            "tgg-melee plays {}, unmodified, since netplay and replays need the game files as they shipped.",
            file_name(found)
        ),
        Disc::Missing => {
            "None of your discs is unmodified, and tgg-melee needs one: netplay and replays need the game files as they shipped.".to_owned()
        }
        Disc::Unknown | Disc::Looking => "Looking for an unmodified disc…".to_owned(),
    };
    setting(
        "settings/disc",
        "tgg-melee disc",
        line,
        text_action("Choose another…", false, p)
            .key("settings/choose-disc")
            .on_press_tx(crate::mods::actions::choose_disc),
        p,
    )
    .help(
        found
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
    )
}

/// A path's file name, or nothing.
fn file_name(path: Option<&std::path::Path>) -> String {
    path.and_then(|path| path.file_name())
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Help: reporting a problem, the community, and what's running.
fn help(p: &Palette) -> El {
    let actions = row()
        .gap(space::XS)
        .items_center()
        .child(
            button("Report a problem", ButtonKind::Secondary, p)
                .key("settings/report")
                .on_press(actions::report_problem),
        )
        .child(
            text_action("Discord", false, p)
                .key("settings/discord")
                .on_press_tx(crate::report::actions::discord),
        );
    setting("settings/help", "Help", tgg_app::log::version(), actions, p)
}
