//! What the player does on the Mods page, and what comes back from its jobs.

use super::{Disc, Done, Fallback, Filter, NewSet, Playing, Registry, Reload, Review, Work};
use crate::jobs::Done as Job;
use crate::state::App;
use rune::{FileDialogOptions, Tx};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tgg_mod::sets::{self, MySet, Want};
use tgg_mod::{Error, Local, ModId, Plan, Progress};

/// How often the thread watching the game looks whether it's ended, or
/// been asked to stop.
const GAME_CHECK: Duration = Duration::from_millis(250);

/// The most pages of listings fetched at once: 50 mods a page.
const LISTING_PAGES: u32 = 20;

/// The most icons and screenshots kept: each is fetched once.
const MOST_IMAGES: usize = 500;

/// The largest icon or screenshot downloaded.
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;

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
    if app
        .mods
        .chosen_set
        .as_deref()
        .is_some_and(|key| app.sets.get(key).is_none())
    {
        app.mods.chosen_set = None;
    }
    ensure_plan(app);
}

/// Read the installed ports and the mods folder again, and check the mods
/// against what textures.gg last said it has. The player's sets follow
/// (Installed mods is the mods folder).
pub fn read_disk(app: &mut App) {
    let before = (app.mods.ports.clone(), app.mods.local.clone());
    read_disk_now(app);
    let changed = (&app.mods.ports, &app.mods.local) != (&before.0, &before.1);
    // What replays need depends on what's installed: plan them again.
    if changed {
        crate::replays::actions::installs_changed(app);
    }
    if changed || app.sets.list.is_empty() {
        crate::sets::actions::read(app);
    }
}

fn read_disk_now(app: &mut App) {
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
    mods.fallback = fallback(&app.data.mods, mods);
    match tgg_mod::skins(&app.data.skins) {
        Ok(skins) => mods.skins = skins,
        Err(error) => mods.problem = Some(capitalized(&error.to_string())),
    }
}

/// When installed mods aren't built for the current tgg-melee: the newest
/// older one installed that every one of them is built for.
fn fallback(folder: &Path, mods: &super::Mods) -> Option<Fallback> {
    let unfit =
        |local: &tgg_mod::Local| local.mods.iter().filter(|m| m.fits == Some(false)).count();
    let now = unfit(&mods.local);
    if now == 0 {
        return None;
    }
    mods.ports
        .iter()
        .rev()
        .filter(|port| !port.current)
        .find_map(|older| {
            let local = tgg_mod::local(folder, Some(&older.port())).ok()?;
            (unfit(&local) == 0).then(|| Fallback {
                version: older.version.clone(),
                unfit: now,
            })
        })
}

/// Show or hide the versions of tgg-melee.
pub fn show_versions(app: &mut App, shown: bool) {
    app.mods.versions = shown;
}

pub fn hide_versions(app: &mut App) {
    show_versions(app, false);
}

/// Remove the installed tgg-melee `version`, which isn't the one in use.
pub fn remove_port(app: &mut App, version: &str) {
    if app.mods.port().is_some_and(|port| port.version == version) {
        return;
    }
    match tgg_mod::remove_port(&app.data.ports, version) {
        Ok(()) => app.tell(format!("Removed tgg-melee {version}."), false),
        Err(error) => app.tell(
            format!("Couldn't remove tgg-melee {version}: {error}."),
            true,
        ),
    }
    read_disk(app);
}

/// Make the installed tgg-melee `version` the one in use.
pub fn use_port(app: &mut App, version: &str) {
    match tgg_mod::ports::use_port(&app.data.ports, version) {
        Ok(port) => app.tell(format!("Using tgg-melee {}.", port.version), false),
        Err(error) => app.tell(format!("Couldn't switch tgg-melee: {error}."), true),
    }
    read_disk(app);
    // Its layout has a catalog of its own.
    if app.mods.registry != Registry::Asking {
        app.mods.registry = Registry::Answered;
        fetch(app);
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
    let installed = app.mods.ports.clone();
    app.jobs.spawn("textures.gg", move || {
        let releases = tgg_mod::port_releases(&*net);
        // Before tgg-melee is installed, what's built for its newest
        // release, so the player can look before they install.
        let layout = port.clone().or_else(|| {
            releases
                .as_ref()
                .ok()?
                .first()
                .map(|release| release.port(PathBuf::new()))
        });
        // A newer tgg-melee with a layout of its own: what's built for it,
        // to say before updating which mods would have to wait.
        // Newer than the version in use, offered or installed already.
        let current: Vec<tgg_mod::PortInstall> = installed
            .iter()
            .filter(|install| install.current)
            .cloned()
            .collect();
        let known: Vec<tgg_mod::PortRelease> = releases
            .as_ref()
            .map(Clone::clone)
            .unwrap_or_default()
            .into_iter()
            .chain(installed.iter().map(|install| install.release.clone()))
            .collect();
        let next = tgg_mod::port_update(&current, &known)
            .filter(|_| !current.is_empty())
            .filter(|release| {
                port.as_ref()
                    .is_some_and(|port| port.game_abi != release.game_abi)
            })
            .map(|release| tgg_mod::catalog(&*net, &release.port(PathBuf::new())));
        let catalog = layout.map(|port| tgg_mod::catalog(&*net, &port));
        // Nothing built for the version in use: the newest older one
        // installed that has mods, to offer going back to.
        let nothing = match &catalog {
            Some(Ok(catalog)) => catalog.mods.is_empty(),
            Some(Err(Error::NoCatalog(_))) => true,
            _ => false,
        };
        let older = (nothing && port.is_some())
            .then(|| {
                installed
                    .iter()
                    .rev()
                    .filter(|older| !older.current)
                    .find_map(|older| {
                        let catalog = tgg_mod::catalog(&*net, &older.port()).ok()?;
                        let mut ids: Vec<&tgg_mod::ModId> = catalog
                            .mods
                            .iter()
                            .map(|entry| &entry.manifest.id)
                            .collect();
                        ids.sort();
                        ids.dedup();
                        (!ids.is_empty()).then(|| (older.version.clone(), ids.len()))
                    })
            })
            .flatten();
        let listings = all_listings(&*net);
        Job::Mods(Box::new(Done::Fetched {
            releases,
            sets: tgg_mod::sets(&*net),
            catalog,
            next,
            older,
            listings,
        }))
    });
}

/// Work out the plan of the set the page shows, when it has none yet.
pub fn ensure_plan(app: &mut App) {
    let Some(key) = super::current_set(app).map(|set| set.key.clone()) else {
        return;
    };
    if !app.sets.plans.contains_key(&key) {
        crate::sets::actions::plan(app, &key);
    }
}

/// Work out the plan of the set the page shows again, after its mods or the
/// folders changed.
fn replan(app: &mut App) {
    if let Some(key) = super::current_set(app).map(|set| set.key.clone()) {
        crate::sets::actions::plan(app, &key);
    }
}

/// Show the set `key` on the right, its plan worked out.
pub fn choose_set(app: &mut App, key: &str) {
    app.mods.chosen_set = Some(key.to_owned());
    app.mods.selected = None;
    ensure_plan(app);
}

/// Show the mods the filter `filter` keeps.
pub fn set_filter(app: &mut App, filter: Filter) {
    app.mods.filter = filter;
}

/// Show the mod `id`'s details: from a link, or You's mods.
pub fn show_mod(app: &mut App, id: &ModId) {
    app.mods.filter = Filter::All;
    if app.mods.selected.as_ref() != Some(id) {
        select(app, id);
    }
}

/// Show the mod `id`'s details beside the list, or hide them when they
/// show already.
pub fn select(app: &mut App, id: &ModId) {
    app.mods.selected = if app.mods.selected.as_ref() == Some(id) {
        None
    } else {
        Some(id.clone())
    };
    app.mods.expanded.clear();
    if app.mods.selected.is_some() {
        request_details(app, id);
    }
}

pub fn close_details(app: &mut App) {
    app.mods.selected = None;
}

/// Show what installing the mod `id` on its own will do: its newest
/// release, and what it needs.
pub fn review_mod(app: &mut App, id: &ModId) {
    let Some(catalog) = &app.mods.catalog else {
        return;
    };
    let Ok(any) = "*".parse() else { return };
    let plan = tgg_mod::plan_mods(&[(id.clone(), any)], catalog, &app.mods.local);
    app.mods.review = Some(Review {
        title: format!("Install {}", app.mods.name_of(id)),
        plan,
    });
}

/// Run `change` on a set on a job of its own, then hand back the sets as
/// they are now: a set change always ends in `Done::SetChanged`, so the
/// cards, the panel and the plans follow disk.
fn change(
    app: &mut App,
    job: &'static str,
    change: impl FnOnce(&Path, &Local) -> Result<(Option<String>, Option<String>), String>
    + Send
    + 'static,
) {
    let (data, local) = (app.data.tgg.clone(), app.mods.local.clone());
    app.jobs.spawn(job, move || {
        let result = change(&data, &local);
        let list = sets::list(&data, &local)
            .map(|list| (list, sets::active(&data)))
            .map_err(|error| error.to_string());
        Job::Mods(Box::new(Done::SetChanged(Box::new(Reload {
            list,
            result,
        }))))
    });
}

/// Make the set `key` the one Play starts.
pub fn use_set(app: &mut App, key: &str) {
    let (for_job, show) = (key.to_owned(), key.to_owned());
    change(app, "use a set", move |data, _| {
        sets::activate(data, &for_job).map_err(|error| error.to_string())?;
        Ok((None, Some(show)))
    });
}

/// Make a set of the player's named "My <name>" where `locked` was, which
/// builds on it.
fn own_from(
    net: &dyn tgg_mod::Net,
    data: &Path,
    local: &Local,
    locked: &MySet,
) -> Result<MySet, String> {
    let link = locked
        .builds_on()
        .next()
        .ok_or_else(|| format!("{} doesn't build on a set", locked.name))?
        .to_owned();
    let wanted = format!("My {}", locked.name);
    let mine = match sets::new(data, &wanted) {
        Ok(mine) => mine,
        // Its name is taken, or too long with "My" before it.
        Err(_) => sets::copy(data, local, &locked.key, None).map_err(|error| error.to_string())?,
    };
    sets::build_on(net, data, local, &mine.key, &link).map_err(|error| error.to_string())
}

/// Make a set of the player's from the locked `key`, and show it.
pub fn make_own(app: &mut App, key: &str) {
    let Some(locked) = app.sets.get(key).cloned().filter(|set| set.locked) else {
        return;
    };
    let net = app.mods.net.clone();
    change(app, "make your own set", move |data, local| {
        let mine = own_from(&*net, data, local, &locked)?;
        Ok((Some(format!("Made {}.", mine.name)), Some(mine.key)))
    });
}

/// Add the mod `id` to the set the page shows: a line of the player's set,
/// an install into the mods folder for Installed mods, or a set of the
/// player's made from the locked one first.
pub fn add(app: &mut App, id: &ModId) {
    let Some(set) = super::current_set(app).cloned() else {
        return;
    };
    if set.is_installed() {
        if app.mods.local.get(id).is_some() {
            set_on(app, id, true);
        } else {
            install(app, id);
        }
        return;
    }
    let name = app.mods.name_of(id);
    if set.locked {
        let (net, id) = (app.mods.net.clone(), id.clone());
        let old = set.name.clone();
        change(app, "make your own set", move |data, local| {
            let mut mine = own_from(&*net, data, local, &set)?;
            mine.add(id, Want::Latest);
            sets::save(data, &mine).map_err(|error| error.to_string())?;
            Ok((
                Some(format!("Made {} from {old} and added {name}.", mine.name)),
                Some(mine.key),
            ))
        });
        return;
    }
    let (key, id) = (set.key.clone(), id.clone());
    change(app, "add a mod", move |data, local| {
        let mut set = sets::load(data, local, &key).map_err(|error| error.to_string())?;
        set.add(id, Want::Latest);
        sets::save(data, &set).map_err(|error| error.to_string())?;
        Ok((None, Some(key)))
    });
}

/// Install the mod `id` into the mods folder, as the page always has: a
/// script at once, a native mod through the review that says what it does.
fn install(app: &mut App, id: &ModId) {
    let Some(entry) = app.mods.newest(id).cloned() else {
        return;
    };
    let Ok(any) = "*".parse() else { return };
    if !entry.manifest.script {
        review_mod(app, id);
        return;
    }
    let Some(catalog) = &app.mods.catalog else {
        return;
    };
    let plan = tgg_mod::plan_mods(&[(id.clone(), any)], catalog, &app.mods.local);
    apply_plan(
        app,
        format!("Installing {}", entry.manifest.name),
        plan,
        None,
    );
}

/// Take the mod `id`'s line out of the set the page shows.
pub fn take_out(app: &mut App, id: &ModId) {
    let Some(set) = super::current_set(app).cloned() else {
        return;
    };
    if set.locked || set.is_installed() {
        return;
    }
    let (key, id) = (set.key.clone(), id.clone());
    change(app, "take a mod out", move |data, local| {
        let mut set = sets::load(data, local, &key).map_err(|error| error.to_string())?;
        set.drop_line(id.as_str());
        sets::save(data, &set).map_err(|error| error.to_string())?;
        Ok((None, Some(key)))
    });
}

/// Leave the mod `id` of a set the shown set builds on out, or put it back.
pub fn leave_out(app: &mut App, id: &ModId, out: bool) {
    let Some(set) = super::current_set(app).cloned() else {
        return;
    };
    if set.locked || set.is_installed() {
        return;
    }
    let (key, id) = (set.key.clone(), id.clone());
    change(app, "leave a mod out", move |data, local| {
        let mut set = sets::load(data, local, &key).map_err(|error| error.to_string())?;
        set.leave_out(&id, out);
        sets::save(data, &set).map_err(|error| error.to_string())?;
        Ok((None, Some(key)))
    });
}

/// Turn the mod `id` of the mods folder on or off.
pub fn set_on(app: &mut App, id: &ModId, on: bool) {
    set_enabled(app, id, on);
    replan(app);
}

/// Open the New set sheet.
pub fn open_new_set(app: &mut App) {
    app.mods.rename = None;
    app.mods.building_on = false;
    app.mods.new_set = Some(NewSet::opened(&app.mods.sets));
}

pub fn close_new_set(app: &mut App) {
    app.mods.new_set = None;
}

/// Tick or untick the textures.gg set `link` for the new set to build on.
pub fn toggle_new_set_set(app: &mut App, link: &str) {
    let Some(sheet) = &mut app.mods.new_set else {
        return;
    };
    match sheet.builds_on.iter().position(|have| have == link) {
        Some(at) => {
            sheet.builds_on.remove(at);
        }
        None => sheet.builds_on.push(link.to_owned()),
    }
}

/// Make the set filled in on the New set sheet, and show it.
pub fn create_new_set(app: &mut App) {
    let Some(sheet) = app.mods.new_set.take() else {
        return;
    };
    let name = sheet.name.text.trim().to_owned();
    if name.is_empty() {
        return;
    }
    let links = sheet.builds_on.clone();
    let online = app
        .mods
        .sets
        .iter()
        .any(|set| set.name == sets::ONLINE_PLAY);
    let say = (!online).then(|| "textures.gg has no Online play set yet".to_owned());
    let net = app.mods.net.clone();
    change(app, "new set", move |data, local| {
        let mut mine = sets::new(data, &name).map_err(|error| error.to_string())?;
        for link in &links {
            mine = sets::build_on(&*net, data, local, &mine.key, link)
                .map_err(|error| error.to_string())?;
        }
        Ok((say, Some(mine.key)))
    });
}

/// Open the Rename sheet on the set the page shows.
pub fn open_rename(app: &mut App) {
    let Some(set) = super::current_set(app) else {
        return;
    };
    if set.locked || set.is_installed() {
        return;
    }
    let mut name = rune::TextEdit::default();
    name.text = set.name.clone();
    app.mods.new_set = None;
    app.mods.building_on = false;
    app.mods.rename = Some(name);
}

pub fn close_rename(app: &mut App) {
    app.mods.rename = None;
}

/// Rename the set the page shows to what's typed.
pub fn rename(app: &mut App) {
    let Some(name) = app
        .mods
        .rename
        .take()
        .map(|edit| edit.text.trim().to_owned())
    else {
        return;
    };
    let Some(set) = super::current_set(app) else {
        return;
    };
    if name.is_empty() || name == set.name {
        return;
    }
    let (key, name) = (set.key.clone(), name);
    change(app, "rename a set", move |data, local| {
        let renamed = sets::rename(data, local, &key, &name).map_err(|error| error.to_string())?;
        Ok((None, Some(renamed.key)))
    });
}

/// Delete the set the page shows. Its mods stay in the mods folder.
pub fn delete_set(app: &mut App, key: &str) {
    let key = key.to_owned();
    let for_job = key.clone();
    change(app, "delete a set", move |data, _| {
        sets::remove(data, &for_job).map_err(|error| error.to_string())?;
        Ok((None, None))
    });
    if app.mods.chosen_set.as_deref() == Some(key.as_str()) {
        app.mods.chosen_set = None;
    }
}

/// Open the sheet that builds the shown set on another one.
pub fn open_build_on(app: &mut App) {
    app.mods.new_set = None;
    app.mods.rename = None;
    app.mods.building_on = true;
}

pub fn close_build_on(app: &mut App) {
    app.mods.building_on = false;
}

/// Build the shown set on the textures.gg set `link`.
pub fn build_on(app: &mut App, link: &str) {
    app.mods.building_on = false;
    let Some(set) = super::current_set(app) else {
        return;
    };
    if set.locked || set.is_installed() {
        return;
    }
    let (key, link) = (set.key.clone(), link.to_owned());
    let net = app.mods.net.clone();
    change(app, "build on a set", move |data, local| {
        sets::build_on(&*net, data, local, &key, &link).map_err(|error| error.to_string())?;
        Ok((None, Some(key)))
    });
}

/// Add the textures.gg set `link` to the player's sets, as it is, and show
/// it: a link from textures.gg opens this way.
pub fn add_set(app: &mut App, link: &str) {
    let (net, link) = (app.mods.net.clone(), link.to_owned());
    change(app, "add a set", move |data, _| {
        let set = sets::get(&*net, data, &link).map_err(|error| error.to_string())?;
        Ok((None, Some(set.key)))
    });
}

/// A job finished. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Fetched {
            releases,
            sets,
            catalog,
            next,
            older,
            listings,
        } => fetched(app, releases, sets, catalog, next, older, listings),
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
                    let mut parts = Vec::new();
                    if new > 0 {
                        parts.push(format!("installed {}", count(new, "mod")));
                    }
                    if updated > 0 {
                        parts.push(format!("updated {}", count(updated, "mod")));
                    }
                    let said = match parts.split_last() {
                        None => "Your mods are up to date.".to_owned(),
                        Some((last, [])) => format!("{}.", capitalized(last)),
                        Some((last, rest)) => {
                            format!("{} and {last}.", capitalized(&rest.join(", ")))
                        }
                    };
                    app.tell(said, false);
                }
                Err(Error::Cancelled) => app.tell(
                    "Stopped installing; each mod is as it was or fully installed.",
                    false,
                ),
                Err(error) => app.tell(format!("Couldn't install: {error}."), true),
            }
            read_disk(app);
            replan(app);
        }
        Done::Disc(found) => app.mods.disc = found.map_or(Disc::Missing, Disc::Found),
        Done::SetChanged(reload) => {
            let Reload { list, result } = *reload;
            match list {
                Ok((list, active)) => {
                    // Plans of sets that are gone go with them.
                    app.sets
                        .plans
                        .retain(|key, _| list.iter().any(|set| set.key == *key));
                    app.sets.list = list;
                    app.sets.active = active;
                }
                Err(why) => app.tell(capitalized(&why), true),
            }
            if app
                .mods
                .chosen_set
                .as_deref()
                .is_some_and(|key| app.sets.get(key).is_none())
            {
                app.mods.chosen_set = None;
            }
            match result {
                Ok((say, show)) => {
                    if let Some(show) = show {
                        app.mods.chosen_set = Some(show);
                        app.mods.selected = None;
                    }
                    if let Some(say) = say {
                        app.tell(say, false);
                    }
                }
                Err(why) => app.tell(capitalized(&why), true),
            }
            // The set showing, and the active one Play starts, follow
            // their new lines.
            let mut keys = vec![app.sets.active.clone()];
            keys.extend(app.mods.chosen_set.clone());
            keys.sort();
            keys.dedup();
            for key in keys {
                if app.sets.get(&key).is_some() {
                    crate::sets::actions::plan(app, &key);
                }
            }
        }
        Done::Details(id, result) => {
            let details = result
                .inspect_err(|error| tgg_app::log(&format!("the details of {id}: {error}")))
                .ok();
            let screenshots: Vec<String> = details
                .iter()
                .flat_map(|details| &details.screenshots)
                .map(|image| image.url.clone())
                .collect();
            app.mods.details.insert(id, details);
            for url in screenshots {
                request_image(app, &url);
            }
        }
        Done::GameEnded(result) => {
            app.mods.playing = None;
            app.sets.session = None;
            // A profile game's folder goes once nothing runs on it.
            if let Err(error) = tgg_mod::finish_profile(&app.data.tgg) {
                tgg_app::log(&format!("finishing profile games: {error}"));
            }
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
    next: Option<Result<tgg_mod::Catalog, Error>>,
    older: Option<(String, usize)>,
    listings: Result<Vec<tgg_mod::ModSummary>, Error>,
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
    mods.older_with_mods = older;
    // The listings come from endpoints that may not be there yet: without
    // them the page shows what the catalog has.
    if let Ok(listings) = listings {
        mods.listings = listings
            .into_iter()
            .map(|summary| (summary.id.clone(), summary))
            .collect();
    }
    let icons: Vec<String> = mods
        .listings
        .values()
        .filter_map(|summary| Some(summary.icon.as_ref()?.url.clone()))
        .collect();
    // Nothing built for the newer tgg-melee reads as an empty list; failing
    // to ask, as not knowing.
    mods.next_built = match next {
        Some(Ok(catalog)) => Some(
            catalog
                .mods
                .into_iter()
                .map(|entry| entry.manifest.id)
                .collect(),
        ),
        Some(Err(Error::NoCatalog(_))) => Some(Vec::new()),
        Some(Err(_)) | None => None,
    };
    mods.registry = if offline {
        Registry::Offline
    } else if let Some(why) = failed {
        Registry::Failed(why)
    } else {
        Registry::Answered
    };
    for url in icons {
        request_image(app, &url);
    }
    read_disk(app);
    replan(app);
}

/// Every mod the registry lists, page by page, up to `LISTING_PAGES`.
fn all_listings(net: &dyn tgg_mod::Net) -> Result<Vec<tgg_mod::ModSummary>, Error> {
    let first = tgg_mod::mods(net, 1)?;
    let pages = first.total_pages.min(LISTING_PAGES);
    let mut listings = first.items;
    for page in 2..=pages {
        listings.extend(tgg_mod::mods(net, page)?.items);
    }
    Ok(listings)
}

/// Fetch the icon or screenshot at `url` once, on a thread of its own; it
/// shows wherever it's drawn as soon as it's decoded. Up to `MOST_IMAGES`.
fn request_image(app: &mut App, url: &str) {
    if app.mods.images.contains_key(url) || app.mods.images.len() >= MOST_IMAGES {
        return;
    }
    let pixels = rune::Pixels::new();
    app.mods.images.insert(url.to_owned(), pixels.clone());
    let (net, url) = (app.mods.net.clone(), url.to_owned());
    std::thread::Builder::new()
        .name("mod image".into())
        .spawn(move || {
            let decoded = net
                .get(&url, IMAGE_LIMIT)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    image::load_from_memory(&bytes).map_err(|error| error.to_string())
                });
            match decoded {
                Ok(image) => {
                    let rgba = image.to_rgba8();
                    pixels.set(rgba.width(), rgba.height(), rgba.into_raw());
                }
                Err(error) => tgg_app::log(&format!("the image {url}: {error}")),
            }
        })
        .expect("start an image's thread");
}

/// Ask the registry for the mod `id`'s page, once.
fn request_details(app: &mut App, id: &ModId) {
    if app.mods.details.contains_key(id) {
        return;
    }
    app.mods.details.insert(id.clone(), None);
    let (net, id) = (app.mods.net.clone(), id.clone());
    app.jobs.spawn("mod details", move || {
        let details = tgg_mod::mod_details(&*net, &id);
        Job::Mods(Box::new(Done::Details(id, details)))
    });
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
    install_release(app, release);
}

/// Install tgg-melee `version` from textures.gg and make it the one in use;
/// when it's installed already, just use it.
pub fn install_version(app: &mut App, version: &str) {
    let Some(release) = app
        .mods
        .releases
        .iter()
        .find(|release| release.version == version)
        .cloned()
    else {
        return;
    };
    install_release(app, release);
}

fn install_release(app: &mut App, release: tgg_mod::PortRelease) {
    if app.mods.work.is_some() {
        return;
    }
    if app.mods.has_port(&release.version) {
        use_port(app, &release.version);
        return;
    }
    let (net, ports) = (app.mods.net.clone(), app.data.ports.clone());
    let work = start(app, format!("Installing tgg-melee {}", release.version));
    app.jobs.spawn("tgg-melee install", move || {
        let report = reporter(&work);
        Job::Mods(Box::new(Done::PortInstalled(tgg_mod::install_port(
            &*net,
            &release,
            &ports,
            &report,
            &work.cancel,
        ))))
    });
}

/// Show what updating the mod `id` alone will do: its newest release that
/// still fits its collections, and anything new that release needs. Its
/// collections' own versions stay as they are, as does every other mod.
pub fn review_mod_update(app: &mut App, id: &ModId) {
    let Some(catalog) = &app.mods.catalog else {
        return;
    };
    let mut plan = tgg_mod::plan_updates(&app.mods.sets, catalog, &app.mods.local);
    plan.steps
        .retain(|step| step.id == *id || step.action == tgg_mod::Action::Install);
    plan.sets.clear();
    if !plan
        .steps
        .iter()
        .any(|step| step.id == *id && matches!(step.action, tgg_mod::Action::Update { .. }))
    {
        return app.tell(format!("{} is up to date.", app.mods.name_of(id)), false);
    }
    // Nothing but that mod changes: no review, it updates in its row.
    if plan.steps.len() == 1 && plan.problems.is_empty() {
        if app.mods.work.is_none() {
            let doing = format!("Updating {}", app.mods.name_of(id));
            apply_plan(app, doing, plan, Some(id.clone()));
        }
        return;
    }
    app.mods.review = Some(Review {
        title: format!("Update {}", app.mods.name_of(id)),
        plan,
    });
}

pub fn cancel_review(app: &mut App) {
    app.mods.review = None;
}

/// Apply the plan being reviewed.
pub fn confirm_review(app: &mut App) {
    if app.mods.work.is_some() {
        return;
    }
    let Some(review) = app.mods.review.take() else {
        return;
    };
    apply_plan(app, review.title, review.plan, None);
}

/// Apply `plan` on a job, named `doing`; `target` when it updates that one
/// mod alone.
fn apply_plan(app: &mut App, doing: String, plan: Plan, target: Option<ModId>) {
    let (net, mods) = (app.mods.net.clone(), app.data.mods.clone());
    // Packages come from, and go into, tgg's cache, shared with profiles.
    let packages = tgg_mod::packages_dir(&app.data.tgg);
    let work = start(app, doing);
    if let Some(work) = &mut app.mods.work {
        work.target = target;
    }
    app.jobs.spawn("mods install", move || {
        let report = reporter(&work);
        Job::Mods(Box::new(Done::Applied(tgg_mod::apply_cached(
            &*net,
            &plan,
            &mods,
            Some(&packages),
            &report,
            &work.cancel,
        ))))
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
        app.tell(
            format!(
                "Couldn't turn it {}: {error}.",
                if on { "on" } else { "off" }
            ),
            true,
        );
    }
    read_disk(app);
}

/// Uninstall the mod `id`.
pub fn remove(app: &mut App, id: &ModId) {
    let name = app
        .mods
        .local
        .get(id)
        .map_or_else(|| id.to_string(), |m| m.manifest.name.clone());
    match tgg_mod::uninstall(&app.data.mods, id) {
        Ok(()) => app.tell(format!("Removed {name}."), false),
        Err(error) => app.tell(format!("Couldn't remove {name}: {error}."), true),
    }
    read_disk(app);
}

/// Find the disc tgg-melee plays: the one the player chose for it, else
/// their Slippi ISO, else any Melee in their game folders. Skins in it don't
/// matter: tgg-melee plays the game as it shipped from any copy. Outside
/// tests only, where the player's folders are looked through.
pub fn find_disc(app: &mut App) {
    app.mods.disc = Disc::Looking;
    let chosen: Vec<PathBuf> = app
        .settings
        .play_iso
        .iter()
        .cloned()
        .chain(
            app.game
                .as_ref()
                .map(|references| references.game().path().to_owned()),
        )
        .collect();
    let everywhere = app.persist;
    app.jobs.spawn("disc", move || {
        let found = chosen
            .into_iter()
            .find(|path| tgg_app::Game::open(path).is_ok())
            .or_else(|| {
                everywhere
                    .then(|| {
                        tgg_app::discover_games()
                            .into_iter()
                            .next()
                            .map(|game| game.path)
                    })
                    .flatten()
            });
        Job::Mods(Box::new(Done::Disc(found)))
    });
}

/// Ask for the disc to play.
pub fn choose_disc(_: &mut App, tx: &mut Tx) {
    let options = FileDialogOptions {
        title: "Choose a Melee disc".into(),
        ..Default::default()
    }
    .filter("Melee NTSC 1.02 disc image", &["iso", "gcm"]);
    tx.open_file(options, |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => use_disc(app, path),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Play the disc at `path`, if it's Melee NTSC 1.02, and remember it.
pub fn use_disc(app: &mut App, path: PathBuf) {
    match tgg_app::Game::open(&path) {
        Ok(_) => {
            let remembered = path.clone();
            app.change_settings(move |settings| settings.play_iso = Some(remembered.clone()));
            app.mods.disc = Disc::Found(path);
        }
        Err(error) => app.tell(capitalized(&error.to_string()), true),
    }
}

/// Watch the game `running` until it ends or Stop ends it: Play becomes
/// Stop meanwhile, from wherever it was started (here, or a replay).
pub fn watch(app: &mut App, mut running: tgg_mod::Running) {
    let stop = Arc::new(AtomicBool::new(false));
    app.mods.playing = Some(Playing { stop: stop.clone() });
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
    app.mods.work = Some(Work {
        doing,
        progress: shared.progress.clone(),
        cancel: shared.cancel.clone(),
        target: None,
    });
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
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// `line` with its first letter capitalized, as a sentence on its own.
fn capitalized(line: &str) -> String {
    let mut characters = line.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// Show all of a part of the chosen mod's details, or only its start again.
pub fn toggle_expanded(app: &mut App, part: super::Expand) {
    if app.mods.expanded.contains(&part) {
        app.mods.expanded.retain(|shown| *shown != part);
    } else {
        app.mods.expanded.push(part);
    }
}
