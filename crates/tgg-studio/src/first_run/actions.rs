//! First run's work: finding and choosing the disc, opening the game the app
//! reads skins from, installing tgg-melee and making the chosen sets, and
//! leaving for Play.

use super::Stage;
use crate::jobs::Done as Job;
use crate::mods::{Disc, Registry};
use crate::nav::{Place, actions as nav};
use crate::state::App;
use rune::{FileDialogOptions, Tx};
use std::path::Path;
use tgg_app::Game;
use tgg_app::settings::Settings;
use tgg_mod::Local;
use tgg_mod::Net;

/// What a first-run job found.
pub enum Done {
    /// The picked sets were made, or why they couldn't be.
    Made(Result<(), String>),
}

/// The work first run does when it opens: find the disc on its own when it
/// can, and ask textures.gg what it offers.
pub fn opened(app: &mut App) {
    if app.mods.disc == Disc::Unknown {
        crate::mods::actions::find_disc(app);
    }
    if app.mods.registry == Registry::NotAsked {
        crate::mods::actions::fetch(app);
    }
}

/// Before each frame: adopt a disc found on its own, and move on once
/// tgg-melee has installed. True when what shows changed.
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
    // The install stopped: installed, cancelled, or failed.
    if app.mods.port().is_some() {
        match app.first_run.as_ref().map(|first| first.picks.links()) {
            Some(links) if !links.is_empty() => {
                if let Some(first) = app.first_run.as_mut() {
                    first.stage = Stage::SettingUp;
                }
                start_sets(app, links);
            }
            _ => {
                // No picks: Installed mods stays active.
                if let Some(first) = app.first_run.as_mut() {
                    first.stage = Stage::Done;
                }
                after_install(app);
            }
        }
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

/// Download tgg-melee and make the picked sets, once the player presses the
/// one button. Nothing starts before this.
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
    if app.mods.port().is_some() {
        begin_sets(app);
        return;
    }
    if app.mods.releases.is_empty() {
        set_problem(
            app,
            "textures.gg didn't answer. Check your connection and try again.",
        );
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

/// Turn `link` on or off among the mods to start with.
pub fn toggle(app: &mut App, link: &str) {
    if let Some(first) = app.first_run.as_mut() {
        first.picks.toggle(link);
    }
}

/// Leave for Play once everything is installed.
pub fn play(app: &mut App) {
    if app
        .first_run
        .as_ref()
        .is_none_or(|first| first.stage != Stage::Done)
    {
        return;
    }
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
                .map(|why| format!("Your sets couldn't be made: {why}"));
        }
        (Err(why), None) => app.tell(format!("Your sets couldn't be made: {why}"), true),
        (Ok(()), None) => {}
    }
    after_install(app);
    true
}

/// The newest tgg-melee installed, and what textures.gg says about it.
fn after_install(app: &mut App) {
    crate::mods::actions::read_disk(app);
    // The picks made sets on disk; read them so Play can use them.
    crate::sets::actions::read(app);
    app.mods.registry = Registry::Answered;
    crate::mods::actions::fetch(app);
}

/// After the game installs: with no picks, Installed mods stays active;
/// with some, make them off the window's thread.
fn begin_sets(app: &mut App) {
    let links = app
        .first_run
        .as_ref()
        .map_or_else(Vec::new, |first| first.picks.links());
    if links.is_empty() {
        if let Some(first) = app.first_run.as_mut() {
            first.stage = Stage::Done;
        }
        after_install(app);
        return;
    }
    if let Some(first) = app.first_run.as_mut() {
        first.stage = Stage::SettingUp;
    }
    start_sets(app, links);
}

fn start_sets(app: &mut App, links: Vec<String>) {
    let net = app.mods.net.clone();
    let data = app.data.tgg.clone();
    let local = app.mods.local.clone();
    app.jobs.spawn("first run sets", move || {
        Job::FirstRun(Box::new(Done::Made(make(&*net, &data, &local, &links))))
    });
}

/// One pick is that set as it is; several make the player's "My set", which
/// builds on all of them. Either way it becomes the active set.
fn make(net: &dyn Net, data: &Path, local: &Local, links: &[String]) -> Result<(), String> {
    let key = if let [link] = links {
        tgg_mod::sets::get(net, data, link)
            .map_err(|error| error.to_string())?
            .key
    } else {
        let mine = my_set(data, local)?;
        for link in links {
            tgg_mod::sets::build_on(net, data, local, &mine.key, link)
                .map_err(|error| error.to_string())?;
        }
        mine.key
    };
    tgg_mod::sets::activate(data, &key).map_err(|error| error.to_string())
}

/// The player's "My set", or a new one by that name. First run can run again
/// after a cancelled install; an existing one is built on again.
fn my_set(data: &Path, local: &Local) -> Result<tgg_mod::sets::MySet, String> {
    match tgg_mod::sets::new(data, "My set") {
        Ok(set) => Ok(set),
        Err(_) => tgg_mod::sets::list(data, local)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|set| {
                !set.is_installed() && !set.locked && set.name.eq_ignore_ascii_case("My set")
            })
            .ok_or_else(|| "a set named My set is already taken".to_owned()),
    }
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
