//! What the player does on the Mods page, and what comes back from its jobs.

use super::{Disc, Done, Playing, Registry, Review, Tab, Work};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::{FileDialogOptions, Tx};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tgg_mod::{Error, ModId, Plan, Progress};

/// How often the thread watching the game looks whether it's ended, or
/// been asked to stop.
const GAME_CHECK: Duration = Duration::from_millis(250);

/// The page opened: read what's installed, ask textures.gg what it offers
/// the first time, and look for the disc to play.
pub fn opened(app: &mut App) {
    read_disk(app);
    if app.mods.registry == Registry::NotAsked {
        fetch(app);
    }
    if app.mods.disc == Disc::Unknown {
        find_disc(app);
    }
}

/// Read the installed ports and the mods folder again, and check the mods
/// against what textures.gg last said it has.
pub fn read_disk(app: &mut App) {
    let mods = &mut app.mods;
    mods.problem = None;
    match tgg_mod::installed_ports(&app.data.ports) {
        Ok(ports) => mods.ports = ports,
        Err(error) => mods.problem = Some(capitalized(&error.to_string())),
    }
    let port = mods.port().map(tgg_mod::PortInstall::port);
    match tgg_mod::local(&app.data.mods, port.as_ref()) {
        Ok(mut local) => {
            if let Some(catalog) = &mods.catalog {
                local.check_updates(&mods.sets, catalog);
            }
            mods.local = local;
        }
        Err(error) => mods.problem = Some(capitalized(&error.to_string())),
    }
}

/// Ask textures.gg for tgg-melee's releases, the sets, and what's built for
/// the current port.
pub fn fetch(app: &mut App) {
    if app.mods.registry == Registry::Asking {
        return;
    }
    app.mods.registry = Registry::Asking;
    let net = app.mods.net.clone();
    let port = app.mods.port().map(tgg_mod::PortInstall::port);
    app.jobs.spawn("textures.gg", move || {
        let releases = tgg_mod::port_releases(&*net);
        // Before tgg-melee is installed, what's built for its newest
        // release, so the player can look before they install.
        let layout = port.or_else(|| releases.as_ref().ok()?.first().map(|release| release.port(PathBuf::new())));
        Job::Mods(Box::new(Done::Fetched {
            releases,
            sets: tgg_mod::sets(&*net),
            catalog: layout.map(|port| tgg_mod::catalog(&*net, &port)),
        }))
    });
}

/// Show the list `tab`.
pub fn show_tab(app: &mut App, tab: Tab) {
    app.mods.tab = tab;
}

/// Show the mod `id`'s details beside the list, or hide them when they
/// show already.
pub fn select(app: &mut App, id: &ModId) {
    app.mods.selected = if app.mods.selected.as_ref() == Some(id) { None } else { Some(id.clone()) };
}

pub fn close_details(app: &mut App) {
    app.mods.selected = None;
}

/// Show what installing the mod `id` on its own will do: its newest
/// release, and what it needs.
pub fn review_mod(app: &mut App, id: &ModId) {
    let Some(catalog) = &app.mods.catalog else { return };
    let Ok(any) = "*".parse() else { return };
    let plan = tgg_mod::plan_mods(&[(id.clone(), any)], catalog, &app.mods.local);
    app.mods.review = Some(Review { title: format!("Install {}", app.mods.name_of(id)), plan });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Fetched { releases, sets, catalog } => fetched(app, releases, sets, catalog),
        Done::PortInstalled(result) => {
            app.mods.work = None;
            match result {
                Ok(port) => {
                    app.tell(format!("tgg-melee {} is installed.", port.version), false);
                    read_disk(app);
                    // The new version's layout has a catalog of its own.
                    app.mods.registry = Registry::Answered;
                    fetch(app);
                }
                Err(Error::Cancelled) => app.tell("Stopped installing tgg-melee.", false),
                Err(error) => app.tell(format!("tgg-melee couldn't be installed: {error}."), true),
            }
        }
        Done::Applied(result) => {
            app.mods.work = None;
            match result {
                Ok(applied) => {
                    let (new, updated) = (applied.installed.len(), applied.updated.len());
                    app.tell(match (new, updated) {
                        (0, 0) => "Your mods are up to date.".to_owned(),
                        (new, 0) => format!("Installed {}.", count(new, "mod")),
                        (0, updated) => format!("Updated {}.", count(updated, "mod")),
                        (new, updated) => format!("Installed {} and updated {updated}.", count(new, "mod")),
                    }, false);
                }
                Err(Error::Cancelled) => app.tell("Stopped installing; each mod is as it was or fully installed.", false),
                Err(error) => app.tell(format!("Couldn't install: {error}."), true),
            }
            read_disk(app);
        }
        Done::Disc(found) => app.mods.disc = found.map_or(Disc::Missing, Disc::Found),
        Done::GameEnded(result) => {
            app.mods.playing = None;
            if let Err(why) = result {
                app.tell(why, true);
            }
        }
    }
    true
}

fn fetched(
    app: &mut App,
    releases: Result<Vec<tgg_mod::PortRelease>, Error>,
    sets: Result<Vec<tgg_mod::Set>, Error>,
    catalog: Option<Result<tgg_mod::Catalog, Error>>,
) {
    let mods = &mut app.mods;
    let mut failed = None;
    let mut offline = false;
    let mut note = |error: &Error| match error {
        Error::Offline(_) => offline = true,
        Error::NoCatalog(_) => {}
        error => failed = Some(capitalized(&error.to_string())),
    };
    match releases {
        Ok(releases) => mods.releases = releases,
        Err(error) => note(&error),
    }
    match sets {
        Ok(sets) => mods.sets = sets,
        Err(error) => note(&error),
    }
    mods.no_mods = None;
    match catalog {
        Some(Ok(catalog)) => mods.catalog = Some(catalog),
        Some(Err(error)) => {
            if let Error::NoCatalog(_) = error {
                mods.catalog = None;
                mods.no_mods = Some(capitalized(&error.to_string()));
            }
            note(&error);
        }
        None => mods.catalog = None,
    }
    mods.registry = if offline {
        Registry::Offline
    } else if let Some(why) = failed {
        Registry::Failed(why)
    } else {
        Registry::Answered
    };
    read_disk(app);
}

/// Install the newest tgg-melee, or update to it.
pub fn install_port(app: &mut App) {
    if app.mods.work.is_some() {
        return;
    }
    let release = match app.mods.port_update() {
        Some(release) => release,
        None if app.mods.ports.is_empty() => match app.mods.releases.first() {
            Some(release) => release.clone(),
            None => return,
        },
        None => return,
    };
    let (net, ports) = (app.mods.net.clone(), app.data.ports.clone());
    let work = start(app, format!("Installing tgg-melee {}", release.version));
    app.jobs.spawn("tgg-melee install", move || {
        let report = reporter(&work);
        Job::Mods(Box::new(Done::PortInstalled(tgg_mod::install_port(&*net, &release, &ports, &report, &work.cancel))))
    });
}

/// Show what installing the set `name` will do.
pub fn review_set(app: &mut App, name: &str) {
    let (Some(set), Some(catalog)) = (app.mods.sets.iter().find(|set| set.name == name), &app.mods.catalog) else { return };
    let plan = tgg_mod::plan_set(set, catalog, &app.mods.local);
    let verb = if app.mods.local.set(name).is_some() { "Update" } else { "Install" };
    app.mods.review = Some(Review { title: format!("{verb} {}", set.title), plan });
}

/// Show what updating everything installed will do.
pub fn review_updates(app: &mut App) {
    let Some(catalog) = &app.mods.catalog else { return };
    let plan = tgg_mod::plan_updates(&app.mods.sets, catalog, &app.mods.local);
    app.mods.review = Some(Review { title: "Update your mods".into(), plan });
}

pub fn cancel_review(app: &mut App) {
    app.mods.review = None;
}

/// Apply the plan being reviewed.
pub fn confirm_review(app: &mut App) {
    if app.mods.work.is_some() {
        return;
    }
    let Some(review) = app.mods.review.take() else { return };
    let (net, mods) = (app.mods.net.clone(), app.data.mods.clone());
    let plan: Plan = review.plan;
    let work = start(app, review.title);
    app.jobs.spawn("mods install", move || {
        let report = reporter(&work);
        Job::Mods(Box::new(Done::Applied(tgg_mod::apply(&*net, &plan, &mods, &report, &work.cancel))))
    });
}

/// Stop the install under way, between files.
pub fn cancel_work(app: &mut App) {
    if let Some(work) = &app.mods.work {
        work.cancel.store(true, Ordering::Relaxed);
    }
}

/// Turn the mod `id` on or off.
pub fn set_enabled(app: &mut App, id: &ModId, on: bool) {
    if let Err(error) = tgg_mod::set_enabled(&app.data.mods, id, on) {
        app.tell(format!("Couldn't turn it {}: {error}.", if on { "on" } else { "off" }), true);
    }
    read_disk(app);
}

/// Uninstall the set `name`: the mods only it installed.
pub fn remove_set(app: &mut App, name: &str) {
    let title = app.mods.set_title(name);
    match tgg_mod::uninstall_set(&app.data.mods, name) {
        Ok(()) => app.tell(format!("Removed {title}."), false),
        Err(error) => app.tell(format!("Couldn't remove {title}: {error}."), true),
    }
    read_disk(app);
}

/// Uninstall the mod `id`.
pub fn remove(app: &mut App, id: &ModId) {
    let name = app.mods.local.get(id).map_or_else(|| id.to_string(), |m| m.manifest.name.clone());
    match tgg_mod::uninstall(&app.data.mods, id) {
        Ok(()) => app.tell(format!("Removed {name}."), false),
        Err(error) => app.tell(format!("Couldn't remove {name}: {error}."), true),
    }
    read_disk(app);
}

/// Look for an unmodified disc of the player's: the one they chose to play,
/// their game if nothing's been installed into it, then the others in their
/// game folders. Outside tests only, where the player's folders are looked
/// through.
pub fn find_disc(app: &mut App) {
    app.mods.disc = Disc::Looking;
    let chosen: Vec<PathBuf> = app.settings.play_iso.iter().cloned()
        .chain(app.game.as_ref().map(|references| references.game().path().to_owned()))
        .collect();
    let everywhere = app.persist;
    app.jobs.spawn("clean disc", move || {
        let found = chosen.into_iter().find(|path| clean(path)).or_else(|| {
            everywhere.then(|| tgg_app::discover_games().into_iter().find(|game| game.changed == 0).map(|game| game.path)).flatten()
        });
        Job::Mods(Box::new(Done::Disc(found)))
    });
}

/// Whether the disc at `path` is Melee NTSC 1.02 with every costume and
/// stage as it shipped.
fn clean(path: &Path) -> bool {
    tgg_app::Game::open(path).is_ok_and(|game| game.changed_slots().is_empty())
}

/// Ask for the disc to play.
pub fn choose_disc(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions { title: "Choose an unmodified Melee disc".into(), ..Default::default() }
        .filter("Melee NTSC 1.02 disc image", &["iso", "gcm"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => use_disc(app, path),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Play the disc at `path`, if it's Melee NTSC 1.02, and remember it. A
/// changed one plays, but netplay wants the files as they shipped.
pub fn use_disc(app: &mut App, path: PathBuf) {
    match tgg_app::Game::open(&path) {
        Ok(game) => {
            let changed = game.changed_slots().len();
            if changed > 0 {
                app.tell(format!("{} on this disc changed; netplay wants an unmodified one.", count(changed, "file")), true);
            }
            let remembered = path.clone();
            app.change_settings(move |settings| settings.play_iso = Some(remembered.clone()));
            app.mods.disc = Disc::Found(path);
        }
        Err(error) => app.tell(capitalized(&error.to_string()), true),
    }
}

/// Start tgg-melee with the player's disc and mods, and watch it until it
/// ends.
pub fn play(app: &mut App) {
    if app.mods.playing.is_some() {
        return;
    }
    let (Some(port), Disc::Found(disc)) = (app.mods.port(), &app.mods.disc) else { return };
    let log = app.data.ports.with_file_name("game.log");
    let version = port.version.clone();
    let running = tgg_mod::ports::launch_with_args(port, disc, &app.data.mods, std::iter::empty::<&str>(), Some(&log));
    let mut running = match running {
        Ok(running) => running,
        Err(error) => {
            app.tell(format!("tgg-melee couldn't start: {error}."), true);
            return;
        }
    };
    let stop = Arc::new(AtomicBool::new(false));
    app.mods.playing = Some(Playing { stop: stop.clone() });
    app.tell(format!("tgg-melee {version} is running."), false);
    app.jobs.spawn("tgg-melee", move || {
        let ended = loop {
            if stop.load(Ordering::Relaxed) {
                break running.kill().map(|()| None);
            }
            match running.try_wait() {
                Ok(Some(status)) => break Ok(Some(status)),
                Ok(None) => std::thread::sleep(GAME_CHECK),
                Err(error) => break Err(error),
            }
        };
        Job::Mods(Box::new(Done::GameEnded(match ended {
            Ok(Some(status)) if !status.success() => Err(match status.code() {
                Some(code) => format!("tgg-melee stopped with exit code {code}."),
                None => "tgg-melee was stopped.".to_owned(),
            }),
            Ok(_) => Ok(()),
            Err(error) => Err(format!("Lost track of tgg-melee: {error}.")),
        })))
    });
}

/// End the game.
pub fn stop(app: &mut App) {
    if let Some(playing) = &app.mods.playing {
        playing.stop.store(true, Ordering::Relaxed);
    }
}

/// Start showing work named `doing`, and hand back what its thread reports to.
fn start(app: &mut App, doing: String) -> Arc<Shared> {
    let shared = Arc::new(Shared {
        progress: Arc::new(Mutex::new(None)),
        cancel: Arc::new(AtomicBool::new(false)),
        waker: app.jobs.waker(),
    });
    app.mods.work = Some(Work { doing, progress: shared.progress.clone(), cancel: shared.cancel.clone() });
    shared
}

/// What a work's thread shares with the window.
struct Shared {
    progress: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
    waker: Option<rune::Waker>,
}

/// A progress callback that keeps the latest progress and wakes the window.
fn reporter(shared: &Arc<Shared>) -> impl Fn(Progress) + Sync + use<> {
    let shared = shared.clone();
    move |progress| {
        if let Ok(mut latest) = shared.progress.lock() {
            *latest = Some(progress);
        }
        if let Some(waker) = &shared.waker {
            waker();
        }
    }
}

/// "1 mod", "3 mods".
fn count(n: usize, noun: &str) -> String {
    if n == 1 { format!("1 {noun}") } else { format!("{n} {noun}s") }
}

/// `line` with its first letter capitalized, as a sentence on its own.
fn capitalized(line: &str) -> String {
    let mut characters = line.chars();
    characters.next().map_or_else(String::new, |first| first.to_uppercase().chain(characters).collect())
}
