//! Changing the settings.

use crate::state::App;
use rune::{FileDialogOptions, Tx};
use tgg_app::settings::Appearance;

/// Show the dialog, its image editor field holding the command kept.
pub fn open(app: &mut App) {
    app.preferences.editor.text = app.settings.external_editor.clone().unwrap_or_default();
    app.preferences.open = true;
}

/// Close the dialog, keeping the image editor's command as typed (none
/// means the system's default app for PNGs).
pub fn close(app: &mut App) {
    let typed = app.preferences.editor.text.trim().to_owned();
    let command = (!typed.is_empty()).then_some(typed);
    if command != app.settings.external_editor {
        app.change_settings(move |settings| settings.external_editor = command.clone());
    }
    app.preferences.open = false;
}

/// Draw in `appearance`'s palette, and remember it.
pub fn set_appearance(app: &mut App, appearance: Appearance) {
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
