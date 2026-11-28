//! What the player does on the Replays page, and what comes back from its
//! jobs.

use super::{Done, Item, MOST, Preparing, Read, Review, Status};
use crate::jobs::Done as Job;
use crate::mods::Disc;
use crate::state::App;
use rune::Tx;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tgg_mod::{Fit, GameNeed, Net, NetError, ProfilePlan};

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

/// The page opened: read the replays the first time (planning them once
/// read); plan them again when they've been read but not planned.
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
    } else if !app.replays.planned {
        refresh(app);
    }
}

/// Read every replay in the folders again, newest first; they're planned
/// once read.
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

/// textures.gg as though out of reach, so planning falls back to what's
/// kept on disk: for every replay of a game version after the first, whose
/// plan has just fetched (and kept) what they share.
struct Kept(Arc<dyn Net>);

impl Net for Kept {
    fn get(&self, url: &str, _: u64) -> Result<Vec<u8>, NetError> {
        Err(NetError::Unreachable {
            url: url.to_owned(),
            reason: "asked already for this game version".to_owned(),
        })
    }

    fn api_url(&self) -> &str {
        self.0.api_url()
    }

    fn releases_url(&self) -> &str {
        self.0.releases_url()
    }

    fn meet_url(&self) -> &str {
        self.0.meet_url()
    }
}

/// Plan every replay that could be read: what each needs to play, asking
/// textures.gg once for each game version they name.
pub fn refresh(app: &mut App) {
    if app.replays.refreshing {
        return;
    }
    app.replays.refreshing = true;
    let net = app.mods.net.clone();
    let (data, mods) = (app.data.tgg.clone(), app.data.mods.clone());
    let profiles: Vec<(PathBuf, tgg_mod::Profile)> = app
        .replays
        .items
        .iter()
        .filter_map(|item| Some((item.path.clone(), item.profile.clone().ok()?)))
        .collect();
    app.jobs.spawn("replays plan", move || {
        let kept = Kept(net.clone());
        let mut asked: HashSet<String> = HashSet::new();
        let mut offline = false;
        let mut plans = Vec::new();
        for (path, profile) in profiles {
            let first = asked.insert(profile.game.version.clone());
            let ask: &dyn Net = if first && !offline { &*net } else { &kept };
            match tgg_mod::plan_profile(ask, &data, &mods, &profile) {
                Ok((plan, note)) => {
                    if first && let Some(note) = note {
                        offline = true;
                        tgg_app::log(&format!("planning replays offline: {note}"));
                    }
                    plans.push((path, plan));
                }
                Err(error) => tgg_app::log(&format!("planning {}: {error}", path.display())),
            }
        }
        let mapping = tgg_mod::cached_mapping(&data);
        Job::Replays(Box::new(Done::Refreshed { mapping, plans }))
    });
}

/// What's installed changed (a mod installed, updated, removed or switched,
/// or another tgg-melee): plan the replays again, now while the page shows
/// (keeping their statuses until the new ones arrive), else when it opens.
pub fn installs_changed(app: &mut App) {
    if !app.replays.scanned {
        return;
    }
    app.replays.planned = false;
    app.replays.checked.clear();
    if app.replays.refreshing {
        app.replays.plan_again = true;
    } else if showing(app) {
        refresh(app);
    }
}

/// Whether the Replays page shows.
fn showing(app: &App) -> bool {
    app.nav.place == crate::nav::Place::Replays
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
                    status: match &read.profile {
                        Err(why) => Status::Unreadable(why.clone()),
                        Ok(_) => Status::Unknown,
                    },
                    path: read.path,
                    source: read.source,
                    info: read.info,
                    profile: read.profile,
                    plan: None,
                })
                .collect();
            refresh(app);
            request_renders(app);
        }
        Done::Refreshed { mapping, plans } => {
            app.replays.refreshing = false;
            app.replays.planned = !std::mem::take(&mut app.replays.plan_again);
            if mapping.is_some() {
                app.replays.mapping = mapping;
            }
            for (path, plan) in plans {
                if let Some(item) = app.replays.items.iter_mut().find(|item| item.path == path) {
                    item.status = status(&plan);
                    item.plan = Some(plan);
                }
            }
            if !app.replays.planned && showing(app) {
                refresh(app);
            }
        }
        Done::Stats { path, result } => {
            app.replays.stats.insert(path, Some(result));
        }
        Done::Prepared {
            path,
            profile_key,
            result,
        } => {
            app.replays.preparing = None;
            match *result {
                Ok(prepared) => launch_prepared(app, &path, &prepared, profile_key),
                Err(tgg_mod::Error::Cancelled) => {
                    app.tell("Stopped getting the replay's mods.", false)
                }
                Err(error) => app.tell(
                    format!("Couldn't download the replay's mods: {error}."),
                    true,
                ),
            }
        }
        Done::Checked { path, result } => {
            app.replays.checking = None;
            app.replays.checked.insert(path.clone());
            let notices = match result {
                Ok(notices) => notices
                    .iter()
                    .map(|notice| crate::sets::actions::withdrawn_words(app, notice))
                    .collect::<Vec<String>>(),
                Err(error) => {
                    // Never blocks watching: say it couldn't be checked.
                    tgg_app::log(&format!("checking withdrawals for a replay: {error}"));
                    Vec::new()
                }
            };
            if notices.is_empty() {
                if let Some(plan) = plan_for(app, &path) {
                    play_checked(app, &path, plan);
                }
            } else if let Some(plan) = plan_for(app, &path) {
                app.replays.review = Some(Review {
                    path,
                    plan,
                    remember: false,
                    notices,
                });
            }
        }
        Done::Opened { read, plan } => {
            let Read {
                path,
                source,
                info,
                profile,
            } = *read;
            let status = match (&profile, &plan) {
                (Err(why), _) => Status::Unreadable(why.clone()),
                (Ok(_), Some(plan)) => status(plan),
                (Ok(_), None) => Status::Unknown,
            };
            app.replays.items.insert(
                0,
                Item {
                    path: path.clone(),
                    source,
                    info,
                    profile,
                    plan: plan.map(|plan| *plan),
                    status,
                },
            );
            if matches!(
                app.replays.items.first().map(|item| &item.status),
                Some(Status::Unknown | Status::Unreadable(_))
            ) {
                let why = app
                    .replays
                    .items
                    .first()
                    .and_then(|item| item.profile.as_ref().err().cloned())
                    .unwrap_or_else(|| "what it needs isn't known".to_owned());
                app.tell(format!("This replay can't play: {why}."), true);
            } else {
                request_renders(app);
                play(app, &path);
            }
        }
    }
    true
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

/// The plan for the replay at `path`, once planned.
pub fn plan_for(app: &App, path: &Path) -> Option<ProfilePlan> {
    let item = app.replays.items.iter().find(|item| item.path == path)?;
    item.plan.clone()
}

/// Whether the replay at `path` can be loaded in-place into the currently
/// running `tgg-melee` via companion without restarting.
pub fn replay_fits_running(app: &App, path: &Path) -> bool {
    if let Some(live) = app.live_game()
        && !live.caps.is_empty()
        && !live.has_cap("replay")
    {
        return false;
    }
    let (Some(_), Some(session), Some(plan)) = (
        &app.companion,
        app.sets.session.as_ref(),
        plan_for(app, path),
    ) else {
        return false;
    };
    if !plan.missing.is_empty() {
        return false;
    }
    if session.profile_key.as_deref() == Some(plan.key.as_str()) {
        return true;
    }
    if !matches!(plan.game, GameNeed::Installed(_)) {
        return false;
    }
    if session.set_key == tgg_mod::sets::INSTALLED {
        return plan.fit == Fit::AsInstalled;
    }
    let Some(set_plan) = app.sets.plan(&session.set_key) else {
        return false;
    };
    let has_player = set_plan
        .plan
        .mods
        .iter()
        .any(|m| m.id.as_str() == "tgg.replay-player");
    if !has_player {
        return false;
    }
    fn gameplay(mods: &[tgg_mod::ProfileMod]) -> Vec<(&str, String)> {
        let mut g: Vec<_> = mods
            .iter()
            .filter(|m| m.changes_game)
            .map(|m| (m.id.as_str(), m.version.to_string()))
            .collect();
        g.sort_by_key(|(id, _)| *id);
        g
    }
    gameplay(&set_plan.plan.mods) == gameplay(&plan.mods)
}

/// Play the replay at `path`: if `tgg-melee` is already running, load it
/// in-place via companion when its mods fit or cleanly relaunch (confirming
/// first if in an active match). Otherwise check withdrawals and launch.
pub fn play(app: &mut App, path: &Path) {
    if app.replays.preparing.is_some() {
        return;
    }
    let Disc::Found(_) = &app.mods.disc else {
        return app.tell(
            "Watching a replay needs a Melee disc; choose one in Settings.",
            true,
        );
    };
    let Some(plan) = plan_for(app, path) else {
        return app.tell(
            "Check for updates on the Mods page first: what this replay needs isn't known yet.",
            true,
        );
    };
    if app.mods.playing.is_some() {
        crate::play::actions::watch_replay(app, path.to_owned());
        return;
    }
    if app.replays.checked.contains(path) {
        return play_checked(app, path, plan);
    }
    if app.replays.checking.as_deref() == Some(path) {
        return;
    }
    // A replay that plays from the player's own mods folder is the one a
    // withdrawal can change; one in a folder of its own has its own mods.
    let GameNeed::Installed(install) = &plan.game else {
        app.replays.checked.insert(path.to_owned());
        return play_checked(app, path, plan);
    };
    app.replays.checking = Some(path.to_owned());
    let port = install.port();
    let (net, data, mods) = (
        app.mods.net.clone(),
        app.data.tgg.clone(),
        app.data.mods.clone(),
    );
    let checked = path.to_owned();
    app.jobs.spawn("replay withdrawals", move || {
        let result = tgg_mod::withdraw::check_withdrawn(&*net, &data, &mods, &port)
            .map_err(|error| error.to_string());
        Job::Replays(Box::new(Done::Checked {
            path: checked,
            result,
        }))
    });
}

/// Play `path` with `plan`, its mods already checked.
fn play_checked(app: &mut App, path: &Path, plan: ProfilePlan) {
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
        notices: Vec::new(),
    });
}

/// Ask for a replay file with the system's dialog, then watch it as one the
/// folders held would play.
pub fn open_file(_: &mut App, tx: &mut Tx) {
    let options = rune::FileDialogOptions {
        title: "Open a replay".into(),
        ..Default::default()
    }
    .filter("Slippi replay", &["slp"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => open(app, path),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Read the replay at `path` and plan it, then watch it.
fn open(app: &mut App, path: PathBuf) {
    if app.replays.items.iter().any(|item| item.path == path) {
        return play(app, &path);
    }
    let (net, data, mods) = (
        app.mods.net.clone(),
        app.data.tgg.clone(),
        app.data.mods.clone(),
    );
    let mapping = app.replays.mapping.clone();
    let read = path.clone();
    app.jobs.spawn("chosen replay", move || {
        let info = tgg_mod::replay_info(&read).ok();
        let profile =
            tgg_mod::profile_from_slp(&read, mapping.as_ref()).map_err(|error| error.to_string());
        let plan = profile
            .as_ref()
            .ok()
            .and_then(|profile| tgg_mod::plan_profile(&*net, &data, &mods, profile).ok())
            .map(|(plan, _)| Box::new(plan));
        Job::Replays(Box::new(Done::Opened {
            read: Box::new(Read {
                path: read,
                source: "Chosen".to_owned(),
                info,
                profile,
            }),
            plan,
        }))
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
                tgg_app::log(&format!("remembering trust in {}: {error}", publisher.name));
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
    let profile_key = plan.key.clone();
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
            profile_key,
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
    let mut env: Vec<(String, std::ffi::OsString)> = vec![(
        "TGG_REPLAY_PLAYER_FILE".to_owned(),
        path.as_os_str().to_owned(),
    )];
    if let Some(comp) = crate::sets::actions::start_companion(app) {
        env.push(comp);
    }
    let (Some(port), Disc::Found(disc)) = (app.mods.port(), &app.mods.disc) else {
        return;
    };
    let log = app.data.ports.with_file_name("game.log");
    let running = tgg_mod::ports::launch_with_env(
        port,
        disc,
        &app.data.mods,
        std::iter::empty::<&str>(),
        &env,
        Some(&log),
    );
    started(
        app,
        crate::sets::Session {
            how: crate::sets::How::Offline,
            name: tgg_mod::sets::INSTALLED_NAME.to_owned(),
            set_key: tgg_mod::sets::INSTALLED.to_owned(),
            profile_key: None,
        },
        running,
    );
}

/// Play the replay at `path` in its prepared folder.
fn launch_prepared(app: &mut App, path: &Path, prepared: &tgg_mod::Prepared, profile_key: String) {
    let mut env: Vec<(String, std::ffi::OsString)> = Vec::new();
    if let Some(comp) = crate::sets::actions::start_companion(app) {
        env.push(comp);
    }
    let Disc::Found(disc) = &app.mods.disc else {
        return;
    };
    let log = app.data.ports.with_file_name("game.log");
    let running = tgg_mod::launch_profile_with_env(
        &prepared.port,
        disc,
        prepared,
        Some(path),
        &env,
        Some(&log),
    );
    started(
        app,
        crate::sets::Session {
            how: crate::sets::How::Offline,
            name: "Replay".to_owned(),
            set_key: String::new(),
            profile_key: Some(profile_key),
        },
        running,
    );
}

fn started(
    app: &mut App,
    session: crate::sets::Session,
    running: Result<tgg_mod::Running, tgg_mod::Error>,
) {
    match running {
        Ok(running) => {
            crate::mods::actions::watch(app, running);
            app.sets.session = Some(session);
            app.tell("Watching the replay in tgg-melee.", false);
        }
        Err(error) => {
            app.companion = None;
            app.tell(format!("The replay couldn't start: {error}."), true);
        }
    }
}

/// Show the replay at `path`'s details beside the list, or hide them when
/// they show already; its stats are read the first time.
pub fn select(app: &mut App, path: &Path) {
    if app.replays.selected.as_deref() == Some(path) {
        app.replays.selected = None;
        return;
    }
    app.replays.selected = Some(path.to_owned());
    if !app.replays.stats.contains_key(path) {
        app.replays.stats.insert(path.to_owned(), None);
        let owned = path.to_owned();
        app.jobs.spawn("replay stats", move || {
            let result = tgg_app::replay_stats::replay_stats(&owned);
            Job::Replays(Box::new(Done::Stats {
                path: owned,
                result,
            }))
        });
    }
}

pub fn close_details(app: &mut App) {
    app.replays.selected = None;
}

/// Show only the replays from the folder named `source`, or all.
pub fn set_source(app: &mut App, source: Option<String>) {
    app.replays.source = source;
}

/// The costume `costume` (Slippi's number, the game's color order) of
/// `character`, as a slot of the player's game.
pub fn costume_slot(
    app: &App,
    character: melee_dat::Character,
    costume: u8,
) -> Option<melee_dat::MeleeSlot> {
    let fighter = app
        .melee
        .fighters
        .iter()
        .find(|fighter| fighter.character == character)?;
    let costume = fighter
        .costumes
        .get(usize::from(costume))
        .or_else(|| fighter.costumes.first())?;
    Some(costume.slot(fighter))
}

/// Ask for a picture of each fighter's costume and each stage the replays
/// show, once each.
fn request_renders(app: &mut App) {
    let Some(references) = &app.game else { return };
    let iso = references.game().path().to_owned();
    let mut slots: Vec<melee_dat::MeleeSlot> = Vec::new();
    for item in &app.replays.items {
        let Some(info) = &item.info else { continue };
        slots.extend(
            info.stage
                .and_then(tgg_app::replays::stage)
                .map(melee_dat::MeleeSlot::Stage),
        );
        for player in &info.players {
            if let Some(character) = player.character.and_then(tgg_app::replays::character) {
                slots.extend(costume_slot(app, character, player.costume.unwrap_or(0)));
            }
        }
    }
    slots.sort();
    slots.dedup();
    for slot in slots {
        app.renders.request(&iso, slot);
    }
}
