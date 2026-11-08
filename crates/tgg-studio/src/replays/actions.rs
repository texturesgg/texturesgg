//! What the player does on the Replays page, and what comes back from its
//! jobs.

use super::{Done, Item, MOST, Preparing, Read, Review, Status};
use crate::jobs::Done as Job;
use crate::mods::Disc;
use crate::state::App;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tgg_mod::{Fit, GameNeed, ProfilePlan, SlippiMapping};

/// The folders replays are looked for in: tgg-melee's (its recorder's),
/// and Slippi's.
pub fn folders() -> Vec<(String, PathBuf)> {
    let mut folders = vec![(
        "tgg-melee".to_owned(),
        tgg_mod::game_data_dir().join("replays"),
    )];
    if let Some(slippi) = tgg_app::replays::slippi_folder() {
        folders.push(("Slippi".to_owned(), slippi));
    }
    folders
}

/// The page opened: read the replays the first time, with what textures.gg
/// said last; ask it again when the Mods page hasn't this session.
pub fn opened(app: &mut App) {
    // What's installed and the disc to play, as Mods knows them.
    crate::mods::actions::read_disk(app);
    if app.mods.disc == Disc::Unknown {
        crate::mods::actions::find_disc(app);
    }
    if app.replays.mapping.is_none() {
        app.replays.mapping = tgg_mod::cached_mapping(&app.data.tgg);
    }
    if !app.replays.scanned {
        if app.replays.folders.is_empty() && app.persist {
            app.replays.folders = folders();
        }
        scan(app);
    }
    if !app.replays.refreshing && app.replays.catalogs.is_empty() {
        refresh(app);
    }
}

/// Read every replay in the folders again, newest first.
pub fn scan(app: &mut App) {
    if app.replays.scanning {
        return;
    }
    app.replays.scanning = true;
    let folders = app.replays.folders.clone();
    let mapping = app.replays.mapping.clone();
    app.jobs.spawn("replays", move || {
        let paths: Vec<PathBuf> = folders.iter().map(|(_, path)| path.clone()).collect();
        let reads = tgg_app::replays::replays_in(&paths, MOST)
            .into_iter()
            .map(|file| Read {
                info: tgg_mod::replay_info(&file.path).ok(),
                profile: tgg_mod::profile_from_slp(&file.path, mapping.as_ref())
                    .map_err(|error| error.to_string()),
                source: folders
                    .get(file.folder)
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default(),
                path: file.path,
            })
            .collect();
        Job::Replays(Box::new(Done::Scanned(reads)))
    });
}

/// Ask textures.gg for the Slippi mapping and what's built for each game
/// the replays name, keeping both on disk.
pub fn refresh(app: &mut App) {
    if app.replays.refreshing {
        return;
    }
    app.replays.refreshing = true;
    let (net, data) = (app.mods.net.clone(), app.data.tgg.clone());
    // The current game, and the releases textures.gg offers by version.
    let current = app.mods.port().map(tgg_mod::PortInstall::port);
    let releases = app.mods.releases.clone();
    let wanted: Vec<String> = app
        .replays
        .items
        .iter()
        .filter_map(|item| item.profile.as_ref().ok())
        .map(|profile| profile.game.version.clone())
        .collect();
    app.jobs.spawn("replays refresh", move || {
        let mapping = tgg_mod::refresh_mapping(&*net, &data)
            .inspect_err(|error| tgg_app::log(&format!("the Slippi mapping: {error}")))
            .ok();
        let mut ports: Vec<tgg_mod::Port> = current.into_iter().collect();
        for release in releases
            .iter()
            .filter(|release| wanted.contains(&release.version))
        {
            ports.push(release.port(PathBuf::new()));
        }
        let mut catalogs = Vec::new();
        for port in ports {
            if catalogs.iter().any(|(layout, _)| *layout == port.game_abi) {
                continue;
            }
            match tgg_mod::refresh_catalog(&*net, &data, &port) {
                Ok(catalog) => catalogs.push((port.game_abi.clone(), catalog)),
                Err(error) => tgg_app::log(&format!("the catalog for {}: {error}", port.version)),
            }
        }
        Job::Replays(Box::new(Done::Refreshed { mapping, catalogs }))
    });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Scanned(reads) => {
            app.replays.scanning = false;
            app.replays.scanned = true;
            app.replays.items = reads
                .into_iter()
                .map(|read| Item {
                    path: read.path,
                    source: read.source,
                    info: read.info,
                    profile: read.profile,
                    status: Status::Unknown,
                })
                .collect();
            resolve_all(app);
        }
        Done::Refreshed { mapping, catalogs } => {
            app.replays.refreshing = false;
            if mapping.is_some() {
                app.replays.mapping = mapping;
            }
            for (layout, catalog) in catalogs {
                app.replays.catalogs.retain(|(known, _)| *known != layout);
                app.replays.catalogs.push((layout, catalog));
            }
            resolve_all(app);
        }
        Done::Prepared { path, result } => {
            app.replays.preparing = None;
            match *result {
                Ok(prepared) => launch_prepared(app, &path, &prepared),
                Err(tgg_mod::Error::Cancelled) => {
                    app.tell("Stopped getting the replay's mods.", false)
                }
                Err(error) => app.tell(format!("Couldn't get the replay's mods: {error}."), true),
            }
        }
    }
    true
}

/// The catalog for `layout`: as fetched this session, else as kept on disk.
fn catalog_for(app: &App, layout: &str) -> Option<tgg_mod::Catalog> {
    app.replays
        .catalogs
        .iter()
        .find(|(known, _)| known == layout)
        .map(|(_, catalog)| catalog.clone())
        .or_else(|| tgg_mod::cached_catalog(&app.data.tgg, layout))
}

/// The plan for playing `profile`: what it needs, against the catalog for
/// its game, the mapping, and what's installed. None while there's no
/// catalog to resolve against.
fn plan(app: &App, profile: &tgg_mod::Profile) -> Option<ProfilePlan> {
    let current = app.mods.port().map(|port| port.release.game_abi.clone());
    let layout = tgg_mod::profile_layout(profile, &app.data.ports)
        .or_else(|| (profile.game.version == "any").then_some(current).flatten())
        .or_else(|| {
            app.mods
                .releases
                .iter()
                .find(|release| release.version == profile.game.version.trim_start_matches('v'))
                .map(|release| release.game_abi.clone())
        })?;
    let catalog = catalog_for(app, &layout)?;
    let mapping = app.replays.mapping.clone().unwrap_or_else(empty_mapping);
    let trust = tgg_mod::trust(&app.data.tgg).unwrap_or_default();
    Some(tgg_mod::resolve_profile(
        profile,
        &catalog,
        &mapping,
        &app.data.ports,
        &app.mods.local,
        &trust,
    ))
}

/// A mapping with nothing in it, before textures.gg has sent one.
fn empty_mapping() -> SlippiMapping {
    SlippiMapping {
        format: 1,
        snapshot: None,
        codes: Vec::new(),
        ignore: Vec::new(),
        minor: Vec::new(),
    }
}

/// Work out every replay's status.
fn resolve_all(app: &mut App) {
    let statuses: Vec<Status> = app
        .replays
        .items
        .iter()
        .map(|item| match &item.profile {
            Err(why) => Status::Unreadable(why.clone()),
            Ok(profile) => plan(app, profile).map_or(Status::Unknown, |plan| status(&plan)),
        })
        .collect();
    for (item, status) in app.replays.items.iter_mut().zip(statuses) {
        item.status = status;
    }
}

/// What a plan comes to, for a row.
pub fn status(plan: &ProfilePlan) -> Status {
    let game = match &plan.game {
        GameNeed::Installed(_) => None,
        GameNeed::Available { version, .. } => Some(version.clone()),
        GameNeed::Unavailable { version } => return Status::NoGame(version.clone()),
    };
    if plan.fit == Fit::AsInstalled && game.is_none() {
        return Status::AsIs;
    }
    let fetch = plan.mods.iter().filter(|m| !m.cached).count();
    Status::Needs {
        mods: fetch,
        size: plan.size(),
        game,
    }
}

/// The plan for the replay at `path`, now.
pub fn plan_for(app: &App, path: &Path) -> Option<ProfilePlan> {
    let item = app.replays.items.iter().find(|item| item.path == path)?;
    plan(app, item.profile.as_ref().ok()?)
}

/// Play the replay at `path`: as installed straight away when its mods
/// match, else after its review.
pub fn play(app: &mut App, path: &Path) {
    if app.mods.playing.is_some() || app.replays.preparing.is_some() {
        return;
    }
    let Disc::Found(_) = &app.mods.disc else {
        return app.tell(
            "Playing a replay needs an unmodified Melee disc; choose one on the Mods page.",
            true,
        );
    };
    let Some(plan) = plan_for(app, path) else {
        return app.tell(
            "Check for updates on the Mods page first: what this replay needs isn't known yet.",
            true,
        );
    };
    if plan.fit == Fit::AsInstalled
        && plan.missing.is_empty()
        && matches!(plan.game, GameNeed::Installed(_))
    {
        return launch_as_installed(app, path);
    }
    app.replays.review = Some(Review {
        path: path.to_owned(),
        plan,
        remember: false,
    });
}

pub fn cancel_review(app: &mut App) {
    app.replays.review = None;
}

/// Remember the yes for the publishers the review asks about, or not.
pub fn set_remember(app: &mut App, remember: bool) {
    if let Some(review) = &mut app.replays.review {
        review.remember = remember;
    }
}

/// Play the reviewed replay: as installed when that fits, else fetch its
/// mods, build its folder, and play it there.
pub fn confirm(app: &mut App) {
    let Some(review) = app.replays.review.take() else {
        return;
    };
    if review.remember {
        for publisher in review.plan.untrusted() {
            if let Err(error) = tgg_mod::remember_trust(&app.data.tgg, publisher, true) {
                tgg_app::log(&format!("remembering trust in {publisher}: {error}"));
            }
        }
    }
    if review.plan.fit == Fit::AsInstalled && matches!(review.plan.game, GameNeed::Installed(_)) {
        return launch_as_installed(app, &review.path);
    }
    let progress = Arc::new(Mutex::new(None));
    let cancel = Arc::new(AtomicBool::new(false));
    app.replays.preparing = Some(Preparing {
        path: review.path.clone(),
        progress: progress.clone(),
        cancel: cancel.clone(),
    });
    let (net, data, waker) = (app.mods.net.clone(), app.data.tgg.clone(), app.jobs.waker());
    let (plan, path) = (review.plan, review.path);
    app.jobs.spawn("replay profile", move || {
        let report = move |step: tgg_mod::Progress| {
            if let Ok(mut latest) = progress.lock() {
                *latest = Some(step);
            }
            if let Some(waker) = &waker {
                waker();
            }
        };
        let result = tgg_mod::prepare_profile(&*net, &plan, &data, &report, &cancel);
        Job::Replays(Box::new(Done::Prepared {
            path,
            result: Box::new(result),
        }))
    });
}

/// Stop getting a replay's mods.
pub fn cancel_preparing(app: &mut App) {
    if let Some(preparing) = &app.replays.preparing {
        preparing.cancel.store(true, Ordering::Relaxed);
    }
}

/// Play the replay at `path` with the installed mods.
fn launch_as_installed(app: &mut App, path: &Path) {
    let (Some(port), Disc::Found(disc)) = (app.mods.port(), &app.mods.disc) else {
        return;
    };
    let env = [(
        "TGG_REPLAY_PLAYER_FILE".to_owned(),
        path.as_os_str().to_owned(),
    )];
    let log = app.data.ports.with_file_name("game.log");
    let running = tgg_mod::ports::launch_with_env(
        port,
        disc,
        &app.data.mods,
        std::iter::empty::<&str>(),
        &env,
        Some(&log),
    );
    started(app, running);
}

/// Play the replay at `path` in its prepared folder.
fn launch_prepared(app: &mut App, path: &Path, prepared: &tgg_mod::Prepared) {
    let Disc::Found(disc) = &app.mods.disc else {
        return;
    };
    let log = app.data.ports.with_file_name("game.log");
    let running = tgg_mod::launch_profile(&prepared.port, disc, prepared, Some(path), Some(&log));
    started(app, running);
}

fn started(app: &mut App, running: Result<tgg_mod::Running, tgg_mod::Error>) {
    match running {
        Ok(running) => {
            crate::mods::actions::watch(app, running);
            app.tell("Playing the replay in tgg-melee.", false);
        }
        Err(error) => app.tell(format!("The replay couldn't start: {error}."), true),
    }
}
