//! What a mods folder holds, read from disk with no network: each mod, on
//! or off, with its lock entry and whether it fits the game; each installed
//! set with a status; and what stops mods from loading together. Turning
//! mods on and off and uninstalling them.

use crate::catalog::Catalog;
use crate::error::Error;
use crate::install::{self, Conflict, Installed, ModsDir, Overlap};
use crate::lock::{self, Lock, LockEntry};
use crate::manifest::{Manifest, ModId};
use crate::port::Port;
use crate::profile::Settings;
use crate::registry::Set;
use semver::Version;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A mods folder's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Local {
    /// The game the build checks were made against, if any.
    pub port: Option<Port>,
    /// Every installed mod, by id.
    pub mods: Vec<LocalMod>,
    /// Every installed set, in the order they were first installed.
    pub sets: Vec<LocalSet>,
    /// Turned-on mods the game refuses because a mod it loads earlier
    /// replaces the same function: the refused mod's id and the conflict.
    pub conflicts: Vec<(ModId, Conflict)>,
    /// Disc files two turned-on mods ship: the later-loading mod's id and the
    /// overlap.
    pub overlaps: Vec<(ModId, Overlap)>,
    /// Turned-on mods with dependencies or imports no turned-on mod meets.
    pub unmet: Vec<(ModId, Vec<String>)>,
    /// The lock as read.
    pub lock: Lock,
    /// The settings the player saved, by mod id: the game's `settings.json`
    /// ([`settings_file`]).
    pub settings: BTreeMap<String, Settings>,
}

/// The settings file the game reads beside the mods folder `mods`: the
/// game's own (`settings.json` in [`install::game_data_dir`]) for its
/// default mods folder, else `settings.json` in `mods` (as in a profile's
/// folder).
pub fn settings_file(mods: &Path) -> PathBuf {
    let game = install::game_data_dir();
    if mods == game.join("mods") || mods == ModsDir::game().root() {
        game.join(crate::prepare::SETTINGS_FILE)
    } else {
        mods.join(crate::prepare::SETTINGS_FILE)
    }
}

fn read_settings(path: &Path) -> BTreeMap<String, Settings> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalMod {
    pub manifest: Manifest,
    pub enabled: bool,
    /// Absent for a mod tgg didn't install (a dev build, or one put there by
    /// hand).
    pub lock: Option<LockEntry>,
    /// A dev build: the mod's folder is a link (`tgg mod dev --keep`).
    pub dev: bool,
    /// Whether it fits the game: a mod with a library must be built for its
    /// layout and target. `None` without a game to check against.
    pub fits: Option<bool>,
    /// A newer release [`Local::check_updates`] found.
    pub update: Option<Version>,
}

impl LocalMod {
    pub fn id(&self) -> &ModId {
        &self.manifest.id
    }

    /// The installed sets that list it.
    pub fn sets(&self) -> &[String] {
        self.lock.as_ref().map_or(&[], |entry| &entry.sets)
    }

    /// Installed on its own.
    pub fn direct(&self) -> bool {
        self.lock.as_ref().is_some_and(|entry| entry.direct)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSet {
    /// The set as it was installed.
    pub set: Set,
    pub status: SetStatus,
    /// A newer version of the set [`Local::check_updates`] found.
    pub update: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetStatus {
    /// Every mod installed, fitting the game and its range, and on.
    Complete,
    /// Every mod there, but one turned off or at a version outside the set's
    /// range.
    Customized,
    /// A newer set version, or a newer fitting release of one of its mods.
    Outdated,
    /// A mod missing, or not built for the game.
    Incomplete,
}

/// Whether `manifest` fits `port`: a mod with no library fits every game, a
/// stable-tier one every layout on its target whose mod API it loads on
/// ([`api_fits`]), and a game-tier one only its layout.
pub fn fits(manifest: &Manifest, port: &Port) -> bool {
    manifest
        .game_abi
        .as_ref()
        .is_none_or(|abi| *abi == port.game_abi)
        && manifest
            .target
            .as_ref()
            .is_none_or(|target| *target == port.target)
        && api_fits(manifest, port)
}

/// Whether a game offering `port`'s mod API loads `manifest`'s, as the
/// game's loader and the registry's catalog judge it. The major has to
/// match. A script loads on its minor or a newer one. From 1.0 so does a
/// stable-tier library, unless it uses experimental parts; before 1.0, and
/// with experimental parts, the minor has to match. A game-tier library's
/// layout already ties it to one build.
///
/// Without a version on either side (content, a package from before
/// versions were recorded, a game built from source) the game decides when
/// it loads the mod, as before versions were checked here.
pub fn api_fits(manifest: &Manifest, port: &Port) -> bool {
    if manifest.game_abi.is_some() {
        return true;
    }
    let (Some(needs), Some(offers)) = (&manifest.api_version, &port.api_version) else {
        return true;
    };
    let Some(offers) = api_version(offers) else {
        return true;
    };
    // The game refuses a version it can't read.
    let Some(needs) = api_version(needs) else {
        return false;
    };
    if needs.0 != offers.0 {
        return false;
    }
    if !manifest.script && (needs.0 == 0 || manifest.experimental) {
        return needs.1 == offers.1;
    }
    needs.1 <= offers.1
}

/// A mod API version, `major.minor`.
fn api_version(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

impl Local {
    /// A player with nothing installed.
    pub fn empty() -> Self {
        Self {
            port: None,
            mods: Vec::new(),
            sets: Vec::new(),
            conflicts: Vec::new(),
            overlaps: Vec::new(),
            unmet: Vec::new(),
            lock: Lock::default(),
            settings: BTreeMap::new(),
        }
    }

    pub fn get(&self, id: &ModId) -> Option<&LocalMod> {
        self.mods.iter().find(|m| m.id() == id)
    }

    pub fn set(&self, name: &str) -> Option<&LocalSet> {
        self.sets.iter().find(|set| set.set.name == name)
    }

    /// The installed mods as [`crate::install`] takes them.
    pub fn installed(&self) -> Vec<Installed> {
        self.mods
            .iter()
            .map(|m| Installed {
                manifest: m.manifest.clone(),
                enabled: m.enabled,
            })
            .collect()
    }

    /// Mark what `sets` and `catalog` (the registry's newest, for this
    /// game's layout) have newer: each mod's newest fitting release within
    /// the ranges of the sets that list it, and each set's newer version.
    pub fn check_updates(&mut self, sets: &[Set], catalog: &Catalog) {
        let newest_set = |name: &str| sets.iter().find(|set| set.name == name);
        for m in &mut self.mods {
            let ranges: Vec<_> = m
                .sets()
                .iter()
                .filter_map(|name| {
                    newest_set(name)
                        .or_else(|| self.lock.set(name))
                        .and_then(|set| set.range(&m.manifest.id))
                        .cloned()
                })
                .collect();
            m.update = catalog
                .mods
                .iter()
                .map(|entry| &entry.manifest)
                .filter(|other| other.id == m.manifest.id)
                .filter(|other| other.version > m.manifest.version)
                .filter(|other| self.port.as_ref().is_none_or(|port| fits(other, port)))
                .filter(|other| ranges.iter().all(|range| range.matches(&other.version)))
                .map(|other| other.version.clone())
                .max();
        }
        for local in &mut self.sets {
            local.update = newest_set(&local.set.name)
                .filter(|newer| newer.version > local.set.version)
                .map(|newer| newer.version);
        }
        self.statuses();
    }

    fn statuses(&mut self) {
        for local in &mut self.sets {
            let mut status = SetStatus::Complete;
            let mut outdated = local.update.is_some();
            for listed in &local.set.mods {
                let Some(m) = self.mods.iter().find(|m| m.manifest.id == listed.id) else {
                    status = SetStatus::Incomplete;
                    break;
                };
                if m.fits == Some(false) {
                    status = SetStatus::Incomplete;
                    break;
                }
                outdated |= m.update.is_some();
                if !m.enabled || !listed.range.matches(&m.manifest.version) {
                    status = SetStatus::Customized;
                }
            }
            if status != SetStatus::Incomplete && outdated {
                status = SetStatus::Outdated;
            }
            local.status = status;
        }
    }
}

/// The state of the mods folder `mods`, with build checks against `port`
/// when there is one.
pub fn local(mods: &Path, port: Option<&Port>) -> Result<Local, Error> {
    let dir = ModsDir::new(mods.to_owned());
    let installed = dir.list().map_err(Error::io(mods))?;
    let lock = Lock::read(mods)?;
    let local_mods = installed
        .iter()
        .map(|m| {
            let folder = dir.folder(&m.manifest.id, m.enabled);
            LocalMod {
                manifest: m.manifest.clone(),
                enabled: m.enabled,
                lock: lock.mods.get(&m.manifest.id).cloned(),
                dev: std::fs::symlink_metadata(&folder)
                    .is_ok_and(|meta| meta.file_type().is_symlink()),
                fits: port.map(|port| fits(&m.manifest, port)),
                update: None,
            }
        })
        .collect();

    // What stops a turned-on mod loading, in the order the game loads them.
    let on: Vec<&Installed> = installed.iter().filter(|m| m.enabled).collect();
    let manifests: Vec<&Manifest> = on.iter().map(|m| &m.manifest).collect();
    let order = install::load_order(&manifests);
    let (mut conflicts, mut overlaps, mut unmet) = (Vec::new(), Vec::new(), Vec::new());
    for (position, id) in order.iter().enumerate() {
        let manifest = manifests
            .iter()
            .find(|m| m.id == *id)
            .expect("the load order holds loaded mods");
        let earlier: Vec<Installed> = order[..position]
            .iter()
            .filter_map(|earlier| on.iter().find(|m| m.manifest.id == *earlier))
            .map(|m| (*m).clone())
            .collect();
        for conflict in install::conflicts(manifest, &earlier, None) {
            conflicts.push((id.clone(), conflict));
        }
        for overlap in install::overlaps(manifest, &earlier) {
            overlaps.push((id.clone(), overlap));
        }
    }
    for m in &on {
        let missing = install::unmet(&m.manifest, &installed);
        if !missing.is_empty() {
            unmet.push((m.manifest.id.clone(), missing));
        }
    }

    let sets = lock
        .sets
        .iter()
        .map(|set| LocalSet {
            set: set.clone(),
            status: SetStatus::Complete,
            update: None,
        })
        .collect();
    let mut local = Local {
        port: port.cloned(),
        mods: local_mods,
        sets,
        conflicts,
        overlaps,
        unmet,
        lock,
        settings: read_settings(&settings_file(mods)),
    };
    local.statuses();
    Ok(local)
}

/// Turn the installed mod `id` on or off.
pub fn set_enabled(mods: &Path, id: &ModId, on: bool) -> Result<(), Error> {
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    if !dir.folder(id, true).exists() && !dir.folder(id, false).exists() {
        return Err(Error::NotInstalled(id.clone()));
    }
    dir.set_enabled(id, on).map_err(Error::io(mods))
}

/// Remove the mod `id` and its lock entry. The sets that list it show as
/// incomplete.
pub fn uninstall(mods: &Path, id: &ModId) -> Result<(), Error> {
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    let mut lock = Lock::read(mods)?;
    let tracked = lock.mods.remove(id).is_some();
    if !tracked && !dir.folder(id, true).exists() && !dir.folder(id, false).exists() {
        return Err(Error::NotInstalled(id.clone()));
    }
    dir.remove(id).map_err(Error::io(mods))?;
    if tracked {
        lock.write(mods)?;
    }
    Ok(())
}

/// Remove the set `name`, and the mods it installed that no other installed
/// set lists and that weren't installed on their own.
pub fn uninstall_set(mods: &Path, name: &str) -> Result<(), Error> {
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    let mut lock = Lock::read(mods)?;
    let Some(position) = lock.sets.iter().position(|set| set.name == name) else {
        return Err(Error::SetNotInstalled(name.to_owned()));
    };
    lock.sets.remove(position);
    let mut removed = Vec::new();
    for (id, entry) in &mut lock.mods {
        if !entry.sets.iter().any(|set| set == name) {
            continue;
        }
        entry.sets.retain(|set| set != name);
        if entry.sets.is_empty() && !entry.direct {
            removed.push(id.clone());
        }
    }
    for id in &removed {
        dir.remove(id).map_err(Error::io(mods))?;
        lock.mods.remove(id);
    }
    lock.write(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(api_version: Option<&str>) -> Port {
        Port {
            executable: PathBuf::new(),
            game_abi: "layout-b".into(),
            name: "tgg-melee".into(),
            target: "x86_64-linux-gnu".into(),
            version: "1.2.0".into(),
            api_version: api_version.map(str::to_owned),
        }
    }

    /// A stable-tier library built for `api_version`.
    fn library(api_version: &str) -> Manifest {
        let mut manifest = Manifest::new(
            "me.x".parse().expect("id"),
            "X".into(),
            Version::new(1, 0, 0),
        );
        manifest.tier = Some(crate::manifest::Tier::Stable);
        manifest.target = Some("x86_64-linux-gnu".into());
        manifest.api_version = Some(api_version.into());
        manifest
    }

    fn experimental(api_version: &str) -> Manifest {
        Manifest {
            experimental: true,
            ..library(api_version)
        }
    }

    fn script(api_version: &str) -> Manifest {
        Manifest {
            script: true,
            target: None,
            ..library(api_version)
        }
    }

    #[test]
    fn a_stable_library_loads_on_its_minor_and_newer_ones_from_1_0() {
        let on_1_2 = port(Some("1.2"));
        assert!(fits(&library("1.0"), &on_1_2));
        assert!(fits(&library("1.2"), &on_1_2));
        assert!(!fits(&library("1.3"), &on_1_2));
    }

    #[test]
    fn experimental_parts_tie_a_library_to_its_minor() {
        assert!(!fits(&experimental("1.1"), &port(Some("1.2"))));
        assert!(fits(&experimental("1.1"), &port(Some("1.1"))));
    }

    #[test]
    fn before_1_0_a_library_needs_its_exact_minor() {
        assert!(fits(&library("0.4"), &port(Some("0.4"))));
        assert!(!fits(&library("0.3"), &port(Some("0.4"))));
        assert!(!fits(&library("0.5"), &port(Some("0.4"))));
    }

    #[test]
    fn another_major_never_fits() {
        assert!(!fits(&library("1.0"), &port(Some("2.0"))));
        assert!(!fits(&library("2.0"), &port(Some("1.4"))));
        assert!(!fits(&script("1.0"), &port(Some("2.0"))));
    }

    #[test]
    fn a_script_loads_on_its_minor_and_newer_ones() {
        // The game's loader reads a script's names itself, so 0.x scripts
        // load on newer minors too.
        assert!(fits(&script("0.3"), &port(Some("0.4"))));
        assert!(fits(&script("1.0"), &port(Some("1.2"))));
        assert!(!fits(&script("1.3"), &port(Some("1.2"))));
    }

    #[test]
    fn a_game_tier_library_fits_only_its_layout() {
        let mut manifest = library("1.0");
        manifest.tier = Some(crate::manifest::Tier::Game);
        manifest.game_abi = Some("layout-b".into());
        assert!(fits(&manifest, &port(Some("1.2"))));
        manifest.game_abi = Some("layout-a".into());
        assert!(!fits(&manifest, &port(Some("1.2"))));
    }

    #[test]
    fn without_a_version_on_either_side_the_game_decides() {
        assert!(fits(&library("1.3"), &port(None)));
        let mut unversioned = library("1.0");
        unversioned.api_version = None;
        assert!(fits(&unversioned, &port(Some("1.2"))));
        // Disc files only.
        let content = Manifest::new(
            "me.y".parse().expect("id"),
            "Y".into(),
            Version::new(1, 0, 0),
        );
        assert!(fits(&content, &port(Some("1.2"))));
        // A version the game can't read, it refuses.
        assert!(!fits(&library("one"), &port(Some("1.2"))));
    }

    #[test]
    fn another_target_never_fits() {
        let mut manifest = library("1.0");
        manifest.target = Some("x86_64-windows-gnu".into());
        assert!(!fits(&manifest, &port(Some("1.2"))));
    }
}
