//! Reading the player's sets, working out their plans, making one active,
//! and starting a game from the active set or a queue.

use super::{Done, How, Launching, Planned, Session, Sets, Source, Trust};
use crate::jobs::Done as Job;
use crate::mods::Disc;
use crate::state::App;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tgg_mod::sets::{self, MySet, SetPlan};
use tgg_mod::{Profile, ProfileEntry, Progress};

/// Read the player's sets and which is active, then plan the active one.
pub fn read(app: &mut App) {
    let (data, local) = (app.data.tgg.clone(), app.mods.local.clone());
    app.jobs.spawn("sets", move || {
        let result = sets::list(&data, &local)
            .map(|list| (list, sets::active(&data)))
            .map_err(|error| error.to_string());
        Job::Sets(Box::new(Done::Listed(result)))
    });
}

/// Work out `key`'s plan (what loads, what's skipped and why, its gameplay
/// mods), off the window's thread. It asks textures.gg, so it's for when a
/// set is shown or chosen, not every frame. Installed mods' plan is its
/// counted mods resolved like a profile, for its queue.
pub fn plan(app: &mut App, key: &str) {
    let Some(set) = app.sets.get(key).cloned() else {
        return;
    };
    app.sets.plans.insert(key.to_owned(), Planned::Working);
    let (net, data, mods) = (
        app.mods.net.clone(),
        app.data.tgg.clone(),
        app.data.mods.clone(),
    );
    let (local, skins) = (app.mods.local.clone(), skins_on(app));
    let key = key.to_owned();
    app.jobs.spawn("set plan", move || {
        let result = if set.is_installed() {
            let profile = tgg_mod::profile_from_local(&local);
            tgg_mod::plan_profile(&*net, &data, &mods, &profile).map(|(plan, offline)| SetPlan {
                plan,
                offline,
                mods: Vec::new(),
                notices: Vec::new(),
            })
        } else {
            sets::plan(&*net, &data, &mods, &set, skins)
        };
        Job::Sets(Box::new(Done::Planned {
            key,
            result: result.map_err(|error| error.to_string()),
        }))
    });
}

/// Make `key` the active set: what Play starts.
pub fn activate(app: &mut App, key: &str) {
    if let Err(error) = sets::activate(&app.data.tgg, key) {
        app.tell(capitalized(&error.to_string()), true);
        return;
    }
    app.sets.active = key.to_owned();
    if !app.sets.plans.contains_key(key) {
        plan(app, key);
    }
}

/// Whether the game will load the skin loader: the player has a skin on.
fn skins_on(app: &App) -> bool {
    !app.mods.skins.slots.is_empty()
}

/// Start a game `how` says, from the active set, or from a queue's mods.
/// Planning and fetching run off the window's thread; native code from
/// publishers the player hasn't trusted asks first.
pub fn launch(app: &mut App, how: How) {
    if app.mods.playing.is_some() {
        app.tell("tgg-melee is already running.", true);
        return;
    }
    if app.sets.busy() {
        return;
    }
    if app.mods.port().is_none() || !matches!(app.mods.disc, Disc::Found(_)) {
        app.tell(
            "tgg-melee and your Melee disc have to be set up first.",
            true,
        );
        return;
    }
    let (net, data, mods) = (
        app.mods.net.clone(),
        app.data.tgg.clone(),
        app.data.mods.clone(),
    );
    if let How::Queue { label, mods: queue } = &how {
        let mut profile = Profile::new(tgg_mod::profile::ANY);
        profile.label = Some(label.clone());
        profile.mods = queue
            .iter()
            .map(|m| ProfileEntry {
                id: m.id.clone(),
                version: Some(m.version.clone()),
                range: None,
                sha256: None,
                settings: Default::default(),
            })
            .collect();
        start_work(app, format!("Getting {label}'s mods"));
        app.jobs.spawn("queue plan", move || {
            let result = tgg_mod::plan_profile(&*net, &data, &mods, &profile)
                .map(|(plan, _)| Source::Queue {
                    plan: Box::new(plan),
                })
                .map_err(|error| error.to_string());
            Job::Sets(Box::new(Done::ReadyToPrepare { how, result }))
        });
        return;
    }
    let Some(set) = app.sets.active().cloned() else {
        app.tell("Your sets haven't been read yet.", true);
        return;
    };
    if set.is_installed() {
        let port = app.mods.port().map(tgg_mod::PortInstall::port);
        start_work(app, "Checking your mods".to_owned());
        app.jobs.spawn("withdrawn", move || {
            let notices = port
                .map(|port| tgg_mod::withdraw::check_withdrawn(&*net, &data, &mods, &port))
                .transpose()
                .unwrap_or_else(|error| {
                    tgg_app::log(&format!("checking withdrawn releases: {error}"));
                    None
                })
                .unwrap_or_default();
            Job::Sets(Box::new(Done::Checked { how, notices }))
        });
        return;
    }
    let skins = skins_on(app);
    start_work(app, format!("Getting {}'s mods", set.name));
    app.jobs.spawn("set plan", move || {
        let result = sets::plan(&*net, &data, &mods, &set, skins)
            .map(|plan| Source::Set {
                set,
                plan: Arc::new(plan),
            })
            .map_err(|error| error.to_string());
        Job::Sets(Box::new(Done::ReadyToPrepare { how, result }))
    });
}

/// Show `doing` while a game is got ready.
fn start_work(app: &mut App, doing: String) {
    app.sets.launching = Some(Launching {
        doing,
        progress: Arc::new(Mutex::new(None)),
        cancel: Arc::new(AtomicBool::new(false)),
    });
}

/// Stop getting a game ready.
pub fn cancel(app: &mut App) {
    if let Some(launching) = app.sets.launching.take() {
        launching.cancel.store(true, Ordering::Relaxed);
    }
}

/// Ask about native code the plan needs, or go on and fetch its mods.
fn planned(app: &mut App, how: How, source: Source) {
    let plan = source.profile_plan();
    let publishers: Vec<(String, String, Vec<String>)> = plan
        .untrusted()
        .into_iter()
        .map(|publisher| {
            let key = publisher
                .id
                .clone()
                .unwrap_or_else(|| publisher.name.clone());
            let names = plan.needing(publisher).map(|m| m.name.clone()).collect();
            (key, publisher.name.clone(), names)
        })
        .collect();
    if publishers.is_empty() {
        prepare(app, how, source);
        return;
    }
    app.sets.trust = Some(Trust {
        how,
        source,
        publishers,
        remember: true,
    });
}

/// Run the native code the trust sheet asked about, remembering the yes
/// when the player left that on.
pub fn trust_yes(app: &mut App) {
    let Some(trust) = app.sets.trust.take() else {
        return;
    };
    let Trust {
        how,
        mut source,
        publishers,
        remember,
    } = trust;
    let keys: Vec<String> = publishers.iter().map(|(key, _, _)| key.clone()).collect();
    if remember {
        let plan = source.profile_plan();
        for publisher in plan.untrusted() {
            if let Err(error) = tgg_mod::remember_trust(&app.data.tgg, publisher, true) {
                tgg_app::log(&format!("remembering trust in {}: {error}", publisher.name));
            }
        }
    }
    match &mut source {
        Source::Set { plan, .. } => Arc::make_mut(plan).plan.trust_for_session(&keys),
        Source::Queue { plan } => plan.trust_for_session(&keys),
    }
    prepare(app, how, source);
}

/// Don't run it: no game starts.
pub fn trust_no(app: &mut App) {
    app.sets.trust = None;
    app.sets.launching = None;
}

pub fn set_remember(app: &mut App, remember: bool) {
    if let Some(trust) = &mut app.sets.trust {
        trust.remember = remember;
    }
}

/// Fetch what the game needs into its own folder, off the window's thread.
fn prepare(app: &mut App, how: How, source: Source) {
    let (net, data, waker) = (app.mods.net.clone(), app.data.tgg.clone(), app.jobs.waker());
    let name = match &source {
        Source::Set { set, .. } => set.name.clone(),
        Source::Queue { .. } => match &how {
            How::Queue { label, .. } => label.clone(),
            _ => String::new(),
        },
    };
    if app.sets.launching.is_none() {
        start_work(app, format!("Getting {name}'s mods"));
    }
    let Some(launching) = &app.sets.launching else {
        return;
    };
    let (progress, cancel) = (launching.progress.clone(), launching.cancel.clone());
    app.jobs.spawn("set prepare", move || {
        let report = move |step: Progress| {
            if let Ok(mut latest) = progress.lock() {
                *latest = Some(step);
            }
            if let Some(waker) = &waker {
                waker();
            }
        };
        let result = match &source {
            Source::Set { set, plan } => sets::prepare(&*net, plan, &data, set, &report, &cancel),
            Source::Queue { plan } => {
                tgg_mod::prepare_profile(&*net, plan, &data, &report, &cancel)
            }
        };
        Job::Sets(Box::new(Done::Prepared {
            how,
            name,
            result: result.map_err(|error| error.to_string()),
        }))
    });
}

/// Where the game writes what it says: Play reads it for a room's code.
pub fn game_log(app: &App) -> std::path::PathBuf {
    app.data.ports.with_file_name("game.log")
}

/// The game started: watch it until it ends.
fn started(
    app: &mut App,
    how: How,
    name: String,
    running: Result<tgg_mod::Running, tgg_mod::Error>,
) {
    app.sets.launching = None;
    match running {
        Ok(running) => {
            crate::mods::actions::watch(app, running);
            app.sets.session = Some(Session {
                how,
                name,
                started: Instant::now(),
            });
        }
        Err(error) => app.tell(
            format!(
                "tgg-melee couldn't start: {}.",
                capitalized(&error.to_string())
            ),
            true,
        ),
    }
}

/// What a set job found. True when anything that shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Listed(Ok((list, active))) => {
            app.sets.problem = None;
            // Plans of sets that are gone go with them.
            app.sets
                .plans
                .retain(|key, _| list.iter().any(|set| set.key == *key));
            app.sets.list = list;
            app.sets.active = active.clone();
            if !app.sets.plans.contains_key(&active) {
                plan(app, &active);
            }
        }
        Done::Listed(Err(why)) => app.sets.problem = Some(why),
        Done::Planned { key, result } => {
            let planned = match result {
                Ok(plan) => Planned::Ready(Arc::new(plan)),
                Err(why) => Planned::Failed(why),
            };
            app.sets.plans.insert(key, planned);
        }
        Done::ReadyToPrepare { how, result } => {
            // Cancelled meanwhile.
            if app.sets.launching.is_none() {
                return true;
            }
            match result {
                Ok(source) => planned(app, how, source),
                Err(why) => {
                    app.sets.launching = None;
                    app.tell(capitalized(&why), true);
                }
            }
        }
        Done::Checked { how, notices } => {
            if app.sets.launching.is_none() {
                return true;
            }
            app.sets.said = notices.iter().map(|n| withdrawn_words(app, n)).collect();
            let (Some(port), Disc::Found(disc)) = (app.mods.port(), &app.mods.disc) else {
                app.sets.launching = None;
                return true;
            };
            let log = game_log(app);
            let running = tgg_mod::ports::launch_with_env(
                port,
                disc,
                &app.data.mods,
                std::iter::empty::<&str>(),
                &how.env(),
                Some(&log),
            );
            started(app, how, tgg_mod::sets::INSTALLED_NAME.to_owned(), running);
            crate::mods::actions::read_disk(app);
        }
        Done::Prepared { how, name, result } => {
            if app.sets.launching.is_none() {
                return true;
            }
            match result {
                Ok(prepared) => {
                    let Disc::Found(disc) = &app.mods.disc else {
                        app.sets.launching = None;
                        return true;
                    };
                    let log = game_log(app);
                    let running = tgg_mod::launch_profile_with_env(
                        &prepared.port,
                        disc,
                        &prepared,
                        None,
                        &how.env(),
                        Some(&log),
                    );
                    started(app, how, name, running);
                }
                Err(why) => {
                    app.sets.launching = None;
                    app.tell(capitalized(&why), true);
                }
            }
        }
    }
    true
}

/// What Play says before a game from the set `key`: what changed in the
/// sets it builds on, and each mod it plays without and why. For
/// Installed mods, what the last check of withdrawals turned off.
pub fn notices(app: &App, key: &str) -> Vec<String> {
    let set_is_installed = app.sets.get(key).is_some_and(MySet::is_installed);
    if set_is_installed {
        return app.sets.said.clone();
    }
    let Some(plan) = app.sets.plan(key) else {
        return Vec::new();
    };
    plan.notices
        .iter()
        .cloned()
        .chain(plan.skipped().map(|m| {
            let why = m.skipped.as_deref().unwrap_or_default();
            format!("Playing without {}: {why}", mod_name(app, &m.id))
        }))
        .collect()
}

/// A withdrawal, in the player's words.
fn withdrawn_words(app: &App, notice: &tgg_mod::WithdrawNotice) -> String {
    let name = mod_name(app, &notice.id);
    match notice.action {
        tgg_mod::WithdrawAction::TurnedOff => {
            format!("Playing without {name}: withdrawn ({})", notice.reason)
        }
        tgg_mod::WithdrawAction::Warned => {
            format!("{name} was withdrawn ({}). It's still on.", notice.reason)
        }
        tgg_mod::WithdrawAction::TurnedOn => {
            format!("{name} is back on. textures.gg restored it.")
        }
    }
}

/// A mod's name, as textures.gg lists it, else its id.
pub fn mod_name(app: &App, id: &tgg_mod::ModId) -> String {
    app.mods
        .listings
        .get(id)
        .map(|listing| listing.name.clone())
        .or_else(|| {
            app.mods
                .local
                .mods
                .iter()
                .find(|m| m.manifest.id == *id)
                .map(|m| m.manifest.name.clone())
        })
        .unwrap_or_else(|| id.to_string())
}

/// `line` with its first letter capitalized, as a sentence on its own.
fn capitalized(line: &str) -> String {
    let mut characters = line.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

impl Sets {
    /// Whether a set's game is running: its mods can't change until it ends.
    pub fn running(&self, key: &str) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| self.get(key).is_some_and(|set| set.name == session.name))
    }
}
