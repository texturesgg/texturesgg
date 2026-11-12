//! Changing the settings.

use crate::state::App;
use rune::{FileDialogOptions, Tx};
use tgg_app::settings::Appearance;

/// Show the dialog: the apps that open PNGs as they are now, and the image
/// editor's field showing when the one kept is none of them.
pub fn open(app: &mut App) {
    let kept = app.settings.external_editor.clone();
    let apps = tgg_app::image_apps::image_apps();
    let preferences = &mut app.preferences;
    preferences.other = kept
        .as_ref()
        .is_some_and(|command| !apps.iter().any(|known| known.command == *command));
    preferences.editor.text = kept.unwrap_or_default();
    preferences.apps = apps;
    preferences.open = true;
}

/// Close the dialog, keeping another app's command as typed (emptied, the
/// system's default comes back).
pub fn close(app: &mut App) {
    if app.preferences.other {
        let typed = app.preferences.editor.text.trim().to_owned();
        let command = (!typed.is_empty()).then_some(typed);
        if command != app.settings.external_editor {
            app.change_settings(move |settings| settings.external_editor = command.clone());
        }
    }
    app.preferences.open = false;
}

/// Open textures in `command`'s app, or the system's default for PNGs.
pub fn choose_editor(app: &mut App, command: Option<String>) {
    app.preferences.menu = None;
    app.preferences.other = false;
    app.preferences.editor.text = command.clone().unwrap_or_default();
    app.change_settings(move |settings| settings.external_editor = command.clone());
}

/// Open textures in an app not on the list: its command in the field.
pub fn choose_other(app: &mut App) {
    app.preferences.menu = None;
    app.preferences.other = true;
}

/// Pick the app's program with the file dialog; its path goes in the field.
pub fn browse_editor(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Choose your image editor".into(),
        ..Default::default()
    };
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => {
            // Quoted, so a path with spaces stays one word.
            app.preferences.editor.text = format!("\"{}\"", path.display());
            app.preferences.other = true;
        }
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Report a problem: Settings closes, and the report opens.
pub fn report_problem(app: &mut App) {
    close(app);
    crate::report::actions::open(app, false);
}

/// Draw in `appearance`'s palette, and remember it.
pub fn set_appearance(app: &mut App, appearance: Appearance) {
    app.preferences.menu = None;
    app.change_settings(move |settings| settings.set_appearance(appearance));
}

/// Check for a newer version at launch, or not.
pub fn set_update_check(app: &mut App, on: bool) {
    app.change_settings(move |settings| settings.check_for_updates = Some(on));
}

/// Ask for another ISO to put skins into. Not while a costume is open for
/// editing: its edits belong to the game it came from.
pub fn choose_game(app: &mut App, tx: &mut Tx) {
    if app.editor.is_some() {
        app.tell("Close the editor before changing games.", true);
        return;
    }
    let options = FileDialogOptions {
        title: "Choose your Melee ISO".into(),
        ..Default::default()
    }
    .filter("GameCube disc image", &["iso", "gcm"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => {
            crate::welcome::actions::use_game_at(app, &path);
            match app.welcome.problem.take() {
                Some(problem) => app.tell(problem, true),
                None => app.tell("Changed games.", false),
            }
        }
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Open `menu`, or close it when it's open.
pub fn toggle_menu(app: &mut App, menu: super::Menu) {
    let preferences = &mut app.preferences;
    preferences.menu = if preferences.menu == Some(menu) {
        None
    } else {
        Some(menu)
    };
}

pub fn close_menu(app: &mut App) {
    app.preferences.menu = None;
}
