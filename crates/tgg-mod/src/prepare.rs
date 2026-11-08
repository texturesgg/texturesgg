//! Preparing a resolved profile (its packages fetched into the cache, the
//! port version it needs installed alongside the others, its own mods
//! folder built), launching the game with it for one session, and the
//! global mark that says a profile game is running.
//!
//! A profile's folder is `<data>/tgg/profiles/<key>/`: a folder per mod,
//! copied from the package cache (never from the player's mods folder, so a
//! running profile game can't lock the player's own libraries), the
//! resolved `profile.json`, and `settings.json` with the profile's counted
//! settings. `<data>/tgg/running.json` lists the profile games running:
//!
//! ```json
//! [{ "pid": 1234, "executable": "/…/tgg-melee", "key": "3f9a…", "folder": "/…/profiles/3f9a…" }]
//! ```

use crate::cache::{self, tgg_dir};
use crate::error::{Error, OnProgress, Progress};
use crate::install::ModsDir;
use crate::net::Net;
use crate::package::Package;
use crate::ports::{self, PortInstall, PortRelease, Running};
use crate::profile::{BUILTINS, GameNeed, Profile, ProfilePlan, Settings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const PROFILES: &str = "profiles";
const RUNNING: &str = "running.json";
/// The resolved profile in a profile's folder.
pub const PROFILE_FILE: &str = "profile.json";
/// A profile's counted settings in its folder.
pub const SETTINGS_FILE: &str = "settings.json";
/// Releases tgg has seen, in the ports folder, so a plan can give a size
/// offline.
const RELEASES: &str = ".releases";

/// A profile ready to launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prepared {
    pub key: String,
    /// Its mods folder.
    pub folder: PathBuf,
    /// Its settings file.
    pub settings: PathBuf,
    /// The port it runs on.
    pub port: PortInstall,
    /// The resolved profile.
    pub profile: Profile,
    /// The data folder it was prepared in.
    pub data: PathBuf,
}

impl Prepared {
    /// The environment the game starts with: its mods folder, its settings
    /// file, its counted settings for this run (until the port reads
    /// `TGG_SETTINGS`), and each built-in module that is off.
    pub fn env(&self) -> Vec<(String, OsString)> {
        let mut env = vec![
            (
                "TGG_MODS_DIR".to_owned(),
                self.folder.clone().into_os_string(),
            ),
            (
                "TGG_SETTINGS".to_owned(),
                self.settings.clone().into_os_string(),
            ),
        ];
        let set = set_variable(&self.profile);
        if !set.is_empty() {
            env.push(("TGG_SET".to_owned(), set.into()));
        }
        for (id, variable) in BUILTINS {
            if self.profile.game.settings.get(*id) == Some(&false) {
                env.push(((*variable).to_owned(), "0".into()));
            }
        }
        env
    }
}

/// `TGG_SET` for `profile`'s settings: `<id>:<key>=<value>;…`. A value the
/// variable can't carry (one holding `;`, or not a string, number or bool)
/// is left to the settings file.
fn set_variable(profile: &Profile) -> String {
    let mut parts = Vec::new();
    for entry in &profile.mods {
        for (key, value) in &entry.settings {
            let text = match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            if text.contains(';') || key.contains(['=', ';']) {
                continue;
            }
            parts.push(format!("{}:{key}={text}", entry.id));
        }
    }
    parts.join(";")
}

fn releases_dir(ports: &Path) -> PathBuf {
    ports.join(RELEASES)
}

/// The release of `version` for this system as tgg last saw it, if it has.
pub(crate) fn seen_release(ports: &Path, version: &str) -> Option<PortRelease> {
    let path = releases_dir(ports).join(format!("{version}.json"));
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn remember_release(ports: &Path, release: &PortRelease) {
    let json = serde_json::to_vec_pretty(release).expect("a release serializes");
    let path = releases_dir(ports).join(format!("{}.json", release.version));
    let _ = cache::write_atomic(&path, &json);
}

/// `<data>/tgg/profiles/<key>`.
pub fn profile_folder(data: &Path, key: &str) -> PathBuf {
    tgg_dir(data).join(PROFILES).join(key)
}

/// Fetch what `plan` needs into the package cache, install the port version
/// it needs alongside the others (the version in use stays, unless there
/// was none), and build its folder by copying from the cache. A folder
/// already built for the same key is reused. [`Error::NeedsTrust`] when a
/// mod still needs the player's yes: a front end that got one for this
/// session clears the mod's `needs_trust`, or calls
/// [`crate::remember_trust`] and resolves again.
pub fn prepare_profile(
    net: &dyn Net,
    plan: &ProfilePlan,
    data: &Path,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<Prepared, Error> {
    if let Some(m) = plan.mods.iter().find(|m| m.needs_trust) {
        return Err(Error::NeedsTrust {
            name: m.name.clone(),
            publisher: m.publisher.name.clone(),
        });
    }
    let ports_dir = tgg_dir(data).join("ports");
    let port_steps = usize::from(matches!(plan.game, GameNeed::Available { .. })) * 2;
    let steps = port_steps + plan.mods.len() + 1;
    let port = match &plan.game {
        GameNeed::Installed(install) => install.clone(),
        GameNeed::Unavailable { version } => return Err(Error::GameUnavailable(version.clone())),
        GameNeed::Available { version, .. } => {
            let release = ports::port_release(net, version)?;
            remember_release(&ports_dir, &release);
            let shifted = |p: Progress| progress(Progress { steps, ..p });
            let install = ports::add_port(net, &release, &ports_dir, None, &shifted, cancel)?;
            if ports::current_port(&ports_dir)?.is_none() {
                ports::use_port(&ports_dir, &install.version)?
            } else {
                install
            }
        }
    };
    let packages = cache::packages_dir(data);
    let mut opened = Vec::new();
    for (index, m) in plan.mods.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let step = port_steps + index;
        let label = format!("Downloading {}", m.name);
        let report = |bytes: u64| {
            progress(Progress {
                step,
                steps,
                label: label.clone(),
                bytes,
                total: Some(m.package.size),
            })
        };
        report(0);
        let bytes = cache::fetch_package(
            net,
            Some(&packages),
            &m.id,
            &m.package,
            &mut |bytes, _| report(bytes),
            cancel,
        )?;
        let package = Package::from_zip(&bytes).map_err(|source| Error::Package {
            id: m.id.clone(),
            source,
        })?;
        if package.manifest.id != m.id || package.manifest.version != m.version {
            return Err(Error::PackageMismatch {
                id: m.id.clone(),
                version: m.version.clone(),
            });
        }
        opened.push(package);
    }
    progress(Progress {
        step: steps - 1,
        steps,
        label: "Building the profile".into(),
        bytes: 0,
        total: None,
    });
    let folder = profile_folder(data, &plan.key);
    if !folder.join(PROFILE_FILE).is_file() {
        build_folder(&folder, &opened, plan)?;
    }
    Ok(Prepared {
        key: plan.key.clone(),
        settings: folder.join(SETTINGS_FILE),
        folder,
        port,
        profile: plan.profile.clone(),
        data: data.to_owned(),
    })
}

/// Build the folder of `plan` beside `folder`, then move it in. The
/// profile file is written last, so a folder with one is whole.
fn build_folder(folder: &Path, packages: &[Package], plan: &ProfilePlan) -> Result<(), Error> {
    let parent = folder.parent().expect("a profile folder has a parent");
    std::fs::create_dir_all(parent).map_err(Error::io(parent))?;
    let name = folder.file_name().expect("a name").to_string_lossy();
    let staging = parent.join(format!(".{name}.{}.partial", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(Error::io(&staging))?;
    }
    std::fs::create_dir_all(&staging).map_err(Error::io(&staging))?;
    let dir = ModsDir::new(staging.clone());
    for package in packages {
        dir.install(package).map_err(Error::io(&staging))?;
    }
    let settings: BTreeMap<&str, &Settings> = plan
        .mods
        .iter()
        .filter(|m| !m.settings.is_empty())
        .map(|m| (m.id.as_str(), &m.settings))
        .collect();
    let json = serde_json::to_string_pretty(&settings).expect("settings serialize") + "\n";
    let path = staging.join(SETTINGS_FILE);
    std::fs::write(&path, json).map_err(Error::io(&path))?;
    let path = staging.join(PROFILE_FILE);
    std::fs::write(&path, plan.profile.to_json()).map_err(Error::io(&path))?;
    if folder.exists() {
        // An unfinished folder, or one another tgg built meanwhile.
        if folder.join(PROFILE_FILE).is_file() {
            let _ = std::fs::remove_dir_all(&staging);
            return Ok(());
        }
        std::fs::remove_dir_all(folder).map_err(Error::io(folder))?;
    }
    std::fs::rename(&staging, folder).map_err(Error::io(folder))
}

/// A profile game in `running.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningProfile {
    pub pid: u32,
    pub executable: PathBuf,
    pub key: String,
    pub folder: PathBuf,
}

impl RunningProfile {
    fn alive(&self) -> bool {
        crate::process::runs(self.pid, &self.executable)
    }
}

fn running_path(data: &Path) -> PathBuf {
    tgg_dir(data).join(RUNNING)
}

fn read_running(data: &Path) -> Vec<RunningProfile> {
    std::fs::read(running_path(data))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_running(data: &Path, running: &[RunningProfile]) -> Result<(), Error> {
    let path = running_path(data);
    if running.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                Err(Error::io(path)(error))
            }
            _ => Ok(()),
        };
    }
    let json = serde_json::to_string_pretty(running).expect("running serializes") + "\n";
    cache::write_atomic(&path, json.as_bytes())
}

/// The profile games running, by `<data>/tgg/running.json`.
pub fn profile_games(data: &Path) -> Vec<RunningProfile> {
    read_running(data)
        .into_iter()
        .filter(RunningProfile::alive)
        .collect()
}

/// Start `port` with the disc image `iso` and `prepared`'s folder and
/// settings for one session, playing `replay` when given (through
/// `tgg.replay-player`'s `TGG_REPLAY_PLAYER_FILE`), with the game's output
/// written to `log` when given. The game is marked running in
/// `<data>/tgg/running.json` until it ends; then the mark is cleared and
/// the folder deleted, unless another running game uses it.
pub fn launch_profile(
    port: &PortInstall,
    iso: &Path,
    prepared: &Prepared,
    replay: Option<&Path>,
    log: Option<&Path>,
) -> Result<Running, Error> {
    if !profile_games(&prepared.data).is_empty() {
        return Err(Error::GameRunning);
    }
    let mut env = prepared.env();
    env.retain(|(name, _)| name != "TGG_MODS_DIR");
    if let Some(replay) = replay {
        env.push((
            "TGG_REPLAY_PLAYER_FILE".to_owned(),
            replay.as_os_str().to_owned(),
        ));
    }
    let mut running = ports::spawn(
        port,
        iso,
        &prepared.folder,
        std::iter::empty::<&OsStr>(),
        &env,
        log,
    )?;
    let mark = RunningProfile {
        pid: running.id(),
        executable: port.executable(),
        key: prepared.key.clone(),
        folder: prepared.folder.clone(),
    };
    let mut marks = read_running(&prepared.data);
    marks.retain(RunningProfile::alive);
    marks.push(mark);
    if let Err(error) = write_running(&prepared.data, &marks) {
        let _ = running.kill();
        return Err(error);
    }
    running.profile = Some((prepared.data.clone(), running.id()));
    Ok(running)
}

/// Clear the marks of profile games that have ended, and delete their
/// folders unless a running one uses them: for a front end that lost the
/// [`Running`] handle. Whether a profile game still runs.
pub fn finish_profile(data: &Path) -> Result<bool, Error> {
    let marks = read_running(data);
    let (live, ended): (Vec<_>, Vec<_>) = marks.into_iter().partition(RunningProfile::alive);
    for mark in &ended {
        if !live.iter().any(|other| other.key == mark.key) {
            remove_folder(data, mark);
        }
    }
    write_running(data, &live)?;
    Ok(!live.is_empty())
}

/// The game `pid` ended: clear its mark and its folder.
pub(crate) fn finished(data: &Path, pid: u32) {
    let mut marks = read_running(data);
    let Some(position) = marks.iter().position(|mark| mark.pid == pid) else {
        return;
    };
    let mark = marks.remove(position);
    marks.retain(RunningProfile::alive);
    if !marks.iter().any(|other| other.key == mark.key) {
        remove_folder(data, &mark);
    }
    let _ = write_running(data, &marks);
}

/// Remove a mark's folder, when it is a profile folder of `data`.
fn remove_folder(data: &Path, mark: &RunningProfile) {
    let folder = profile_folder(data, &mark.key);
    if folder == mark.folder {
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// The packages every profile folder names, by SHA-256, for pruning.
pub(crate) fn profile_packages(data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(tgg_dir(data).join(PROFILES)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            crate::profile::profile_from_file(&entry.path().join(PROFILE_FILE)).ok()
        })
        .flat_map(|profile| profile.mods.into_iter().filter_map(|m| m.sha256))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counted_settings_pass_as_tgg_set_when_they_can() {
        let mut profile = Profile::new("0.5.0");
        let mut settings = Settings::new();
        settings.insert("stocks".into(), 4.into());
        settings.insert("items".into(), "Off".into());
        settings.insert("folder".into(), "a;b".into());
        profile.mods.push(crate::profile::ProfileEntry {
            id: "tgg.competitive-rules".parse().expect("id"),
            version: Some(semver::Version::new(0, 1, 2)),
            range: None,
            sha256: None,
            settings,
        });
        assert_eq!(
            set_variable(&profile),
            "tgg.competitive-rules:items=Off;tgg.competitive-rules:stocks=4"
        );
    }
}
