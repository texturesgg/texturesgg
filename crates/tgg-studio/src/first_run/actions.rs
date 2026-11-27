//! First run's work: finding and choosing the disc, opening the game the app
//! reads skins from, installing or updating tgg-melee, setting up the default
//! set (or plain Melee), and leaving for Play.

use super::{DEFAULT_SET, Stage};
use crate::jobs::Done as Job;
use crate::mods::{Disc, Registry};
use crate::nav::{Place, actions as nav};
use crate::state::App;
use rune::{FileDialogOptions, Tx};
use std::path::Path;
use tgg_app::Game;
use tgg_app::settings::Settings;
use tgg_mod::Net;

/// What a first-run job found.
pub enum Done {
    /// The starting set was made, or why it couldn't be.
    Made(Result<(), String>),
}

/// The work first run does when it opens: read any existing tgg-melee install
/// on disk, find the disc on its own when it can, and ask textures.gg what it
/// offers.
pub fn opened(app: &mut App) {
    crate::mods::actions::read_disk(app);
    if app.mods.disc == Disc::Unknown {
        crate::mods::actions::find_disc(app);
    }
    if app.mods.registry == Registry::NotAsked {
        crate::mods::actions::fetch(app);
    }
}

/// Before each frame: adopt a disc found on its own, and move on once
/// tgg-melee has installed or updated. True when what shows changed.
pub fn poll(app: &mut App) -> bool {
    if app.first_run.is_none() {
        return false;
    }
    let mut changed = false;
    // `find_disc` may have found one the app's game wasn't set from yet.
    if app.game.is_none()
        && let Disc::Found(path) = &app.mods.disc
    {
        let path = path.clone();
        if use_game_at(app, &path).is_ok() {
            changed = true;
        }
    }
    let downloading = app
        .first_run
        .as_ref()
        .is_some_and(|first| first.stage == Stage::Downloading);
    if !downloading || app.mods.work.is_some() {
        return changed;
    }
    // The install or update stopped: succeeded, cancelled, or failed.
    if app.mods.port().is_some() && app.mods.port_update().is_none() {
        begin_sets(app);
    } else if let Some(first) = app.first_run.as_mut() {
        first.stage = Stage::Ready;
        first.problem = app
            .notice
            .as_ref()
            .filter(|notice| notice.error)
            .map(|notice| notice.text.clone());
    }
    true
}

/// Ask for the disc with the system's file dialog.
pub fn choose_disc(_app: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Choose your Melee disc".into(),
        ..Default::default()
    }
    .filter("Melee NTSC 1.02 disc image", &["iso", "gcm"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => match use_game_at(app, &path) {
            Ok(()) => clear_problem(app),
            Err(problem) => set_problem(app, problem),
        },
        Ok(None) => {}
        Err(error) => set_problem(app, format!("The file dialog couldn't open: {error}")),
    });
}

/// Make the disc at `path` both the game the app reads skins from and the
/// one tgg-melee plays, and remember it. Says why not when it can't.
pub fn use_game_at(app: &mut App, path: &Path) -> Result<(), String> {
    let opened = Game::open(path)
        .map_err(|error| error.to_string())
        .and_then(|game| tgg_app::game_references(game).map_err(|error| error.to_string()));
    let references = opened?;
    app.change_settings(|settings| remember(settings, path));
    crate::melee::actions::open_game(app, references);
    // Choosing a disc here chooses the one tgg-melee plays too.
    crate::mods::actions::use_disc(app, path.to_owned());
    if app.first_run.is_none() {
        crate::editor::draft::offer(app);
    }
    Ok(())
}

/// Download or update tgg-melee (when needed) and set up the starting set,
/// once the player presses the one button. Nothing starts before this.
pub fn download(app: &mut App) {
    if app
        .first_run
        .as_ref()
        .is_none_or(|first| first.stage != Stage::Ready)
    {
        return;
    }
    if !ensure_disc(app) {
        set_problem(app, "Choose your Melee disc first.");
        return;
    }
    let update = app.mods.port_update();
    if app.mods.port().is_some() && update.is_none() {
        begin_sets(app);
        return;
    }
    if app.mods.port().is_none() && app.mods.releases.is_empty() {
        set_problem(
            app,
            "textures.gg didn't answer. Check your connection and try again.",
        );
        return;
    }
    // Switching to a newer version that's already installed finishes
    // synchronously without a download.
    if let Some(release) = update
        && app.mods.has_port(&release.version)
    {
        crate::mods::actions::use_port(app, &release.version);
        begin_sets(app);
        return;
    }
    if let Some(first) = app.first_run.as_mut() {
        first.stage = Stage::Downloading;
        first.problem = None;
    }
    crate::mods::actions::install_port(app);
    if app.mods.work.is_none() {
        if let Some(first) = app.first_run.as_mut() {
            first.stage = Stage::Ready;
        }
        set_problem(app, "tgg-melee couldn't be downloaded.");
    }
}

/// Stop the download, and come back to the button.
pub fn cancel(app: &mut App) {
    if let Some(work) = &app.mods.work {
        work.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Turn the recommended default set on or off.
pub fn toggle_default_set(app: &mut App) {
    if let Some(first) = app.first_run.as_mut() {
        first.use_default_set = !first.use_default_set;
    }
}

/// Leave for Play once everything is set up.
pub fn play(app: &mut App) {
    if app
        .first_run
        .as_ref()
        .is_none_or(|first| first.stage != Stage::Done)
    {
        return;
    }
    mark_onboarded(app);
    app.first_run = None;
    nav::go(app, Place::Play);
    crate::editor::draft::offer(app);
}

/// What a first-run job found. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    let Done::Made(result) = done;
    match (&result, app.first_run.as_mut()) {
        (_, Some(first)) => {
            first.stage = Stage::Done;
            first.problem = result
                .err()
                .map(|why| format!("Your set couldn't be made: {why}"));
        }
        (Err(why), None) => app.tell(format!("Your set couldn't be made: {why}"), true),
        (Ok(()), None) => {}
    }
    mark_onboarded(app);
    after_install(app);
    true
}

/// Persist that the player has completed first-run setup once.
fn mark_onboarded(app: &mut App) {
    if !app.settings.onboarded {
        app.change_settings(|settings| settings.onboarded = true);
    }
}

/// The newest tgg-melee installed, and what textures.gg says about it.
fn after_install(app: &mut App) {
    crate::mods::actions::read_disk(app);
    // Setup changed the active set on disk; read them so Play can use it.
    crate::sets::actions::read(app);
    app.mods.registry = Registry::Answered;
    crate::mods::actions::fetch(app);
}

/// After tgg-melee is ready: activate Vanilla if unchecked, or fetch and
/// activate the default set off the window's thread.
fn begin_sets(app: &mut App) {
    let use_default = app
        .first_run
        .as_ref()
        .is_none_or(|first| first.use_default_set);
    if !use_default {
        let problem = tgg_mod::sets::activate(&app.data.tgg, tgg_mod::sets::VANILLA)
            .err()
            .map(|why| format!("Couldn't switch to plain Melee: {why}"));
        if let Some(first) = app.first_run.as_mut() {
            first.stage = Stage::Done;
            first.problem = problem;
        }
        mark_onboarded(app);
        after_install(app);
        return;
    }
    if let Some(first) = app.first_run.as_mut() {
        first.stage = Stage::SettingUp;
        first.problem = None;
    }
    let net = app.mods.net.clone();
    let data = app.data.tgg.clone();
    app.jobs.spawn("first run sets", move || {
        Job::FirstRun(Box::new(Done::Made(make(&*net, &data))))
    });
}

/// Add the default set and make it the active set.
fn make(net: &dyn Net, data: &Path) -> Result<(), String> {
    let set = tgg_mod::sets::get(net, data, DEFAULT_SET).map_err(|error| error.to_string())?;
    tgg_mod::sets::activate(data, &set.key).map_err(|error| error.to_string())
}

/// The disc found, as tgg-melee will play it (else the app's game).
fn ensure_disc(app: &mut App) -> bool {
    if let Some(path) = app.game.as_ref().map(|game| game.game().path().to_owned()) {
        if !matches!(app.mods.disc, Disc::Found(_)) {
            crate::mods::actions::use_disc(app, path);
        }
    } else if let Disc::Found(path) = &app.mods.disc {
        let path = path.clone();
        if let Err(problem) = use_game_at(app, &path) {
            set_problem(app, problem);
            return false;
        }
    }
    matches!(app.mods.disc, Disc::Found(_))
}

fn clear_problem(app: &mut App) {
    if let Some(first) = app.first_run.as_mut() {
        first.problem = None;
    }
}

fn set_problem(app: &mut App, problem: impl Into<String>) {
    let problem = problem.into();
    if let Some(first) = app.first_run.as_mut() {
        first.problem = Some(problem);
    } else {
        app.tell(problem, true);
    }
}

/// Remember the disc at `path`, and its folder, so the next launch finds it.
fn remember(settings: &mut Settings, path: &Path) {
    settings.iso_path = Some(path.to_path_buf());
    if let Some(folder) = path.parent()
        && !settings.game_folders.iter().any(|known| known == folder)
    {
        settings.game_folders.push(folder.to_path_buf());
    }
}
