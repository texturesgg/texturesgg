//! Choosing the player's game: from the ISOs found, or any file they pick.

use super::Welcome;
use crate::nav::{Place, actions as nav};
use crate::state::App;
use rune::{FileDialogOptions, Tx};
use std::path::Path;
use tgg_app::Game;
use tgg_app::settings::Settings;

/// Ask for an ISO with the system's file dialog.
pub fn choose(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions { title: "Choose your Melee ISO".into(), ..Default::default() }
        .filter("GameCube disc image", &["iso", "gcm"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => use_game_at(app, &path),
        Ok(None) => {}
        Err(error) => app.welcome.problem = Some(format!("The file dialog couldn't open: {error}")),
    });
}

/// Make the ISO at `path` the player's game if it's Melee NTSC 1.02, and
/// remember it; otherwise say why not.
pub fn use_game_at(app: &mut App, path: &Path) {
    let opened = Game::open(path)
        .map_err(|error| error.to_string())
        .and_then(|game| tgg_app::game_references(game).map_err(|error| error.to_string()));
    match opened {
        Ok(references) => {
            app.change_settings(|settings| remember(settings, path));
            app.welcome = Welcome::default();
            crate::melee::actions::open_game(app, references);
            nav::go(app, Place::Melee);
        }
        Err(problem) => app.welcome.problem = Some(problem),
    }
}

/// Remember the ISO at `path` as the player's, so the next launch opens it,
/// and its folder, so other ISOs there are listed when they change games.
fn remember(settings: &mut Settings, path: &Path) {
    settings.iso_path = Some(path.to_path_buf());
    if let Some(folder) = path.parent()
        && !settings.game_folders.iter().any(|known| known == folder)
    {
        settings.game_folders.push(folder.to_path_buf());
    }
}
