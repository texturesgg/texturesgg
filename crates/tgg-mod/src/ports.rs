//! tgg-melee releases and installs: the release mirror, the ports folder,
//! and launching the game.
//!
//! The mirror holds a folder per release, `<version>/`, with a
//! `release-<target>.json` for each system it was built for and the archives
//! they name; `latest.json` names the newest release, and
//! `latest-prerelease.json` the newest prerelease. A release from before
//! Windows and macOS builds has only `release.json`, its Linux build. Every
//! archive is checked against the size and SHA-256 its release JSON gives
//! before it is unpacked.
//!
//! An installed port is the release's game folder unpacked to
//! `<ports>/<version>/` with its `release.json` beside it, written last; the
//! file `<ports>/current` holds the version in use.

use crate::error::{Error, OnProgress, Progress};
use crate::lock;
use crate::net::{Net, NetError};
use crate::port::Port;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// The game's executable in its folder.
pub const EXECUTABLE: &str = if cfg!(windows) {
    "tgg-melee.exe"
} else {
    "tgg-melee"
};
/// Its debug info, beside it (Linux).
const DEBUG_INFO: &str = "tgg-melee.debug";
const CURRENT: &str = "current";
const RELEASE: &str = "release.json";
/// The longest release JSON read.
const JSON_LIMIT: u64 = 1024 * 1024;

/// The build of tgg-melee for this system, if there is one: the target
/// triple its SDK and mods name.
pub fn target() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("x86_64-linux-gnu")
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Some("x86_64-windows-gnu")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("aarch64-apple-darwin")
    } else {
        None
    }
}

/// This system, as people name it.
pub fn system() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86-64",
        "aarch64" if cfg!(target_os = "macos") => "Apple silicon",
        "aarch64" => "arm64",
        other => other,
    };
    format!("{os} on {arch}")
}

/// The build for this system, or the error saying there is none.
pub fn this_target() -> Result<&'static str, Error> {
    target().ok_or_else(|| Error::UnsupportedSystem(system()))
}

/// One release of tgg-melee for one system, as its release JSON gives it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortRelease {
    pub version: String,
    pub commit: String,
    pub api: String,
    pub api_version: String,
    pub game_abi: String,
    pub target: String,
    pub glibc: String,
    pub toolchain: Toolchain,
    pub files: ReleaseFiles,
    /// Found through `latest-prerelease.json`, there being no release yet.
    #[serde(skip)]
    pub prerelease: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    pub gcc: String,
    /// The nixpkgs revision whose GCC built the game.
    pub nixpkgs: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseFiles {
    pub game: ReleaseFile,
    pub sdk: ReleaseFile,
    /// The debug info gdb reads; Linux builds only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<ReleaseFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}

impl ReleaseFile {
    /// The folder at the top of the archive: its name without `.tar.gz`,
    /// and without `-debug` for the debug archive, whose folder is the
    /// game's so it unpacks over it.
    fn top(&self) -> Result<&str, Error> {
        let stem = self
            .name
            .strip_suffix(".tar.gz")
            .ok_or_else(|| Error::BadRelease(format!("{} is not a .tar.gz", self.name)))?;
        Ok(stem.strip_suffix("-debug").unwrap_or(stem))
    }
}

impl PortRelease {
    /// The game this release installs, as mods see it.
    pub fn port(&self, executable: PathBuf) -> Port {
        Port {
            executable,
            game_abi: self.game_abi.clone(),
            name: "tgg-melee".into(),
            target: self.target.clone(),
            version: self.version.clone(),
        }
    }

    /// The archives' combined download size for playing: the game's.
    pub fn size(&self) -> u64 {
        self.files.game.size
    }
}

#[derive(Deserialize)]
struct Latest {
    version: String,
}

/// `path` of the mirror as JSON; `None` if it isn't there.
fn mirror_json<T: serde::de::DeserializeOwned>(
    net: &dyn Net,
    path: &str,
) -> Result<Option<T>, Error> {
    let url = format!("{}/{path}", net.releases_url());
    match net.get(&url, JSON_LIMIT) {
        Ok(body) => serde_json::from_slice(&body)
            .map(Some)
            .map_err(|error| Error::BadRelease(format!("{path}: {error}"))),
        Err(NetError::NotFound { .. }) => Ok(None),
        Err(error) => Err(Error::from_net(error)),
    }
}

/// The newest release's version, or before the first release the newest
/// prerelease's (`true`).
fn latest(net: &dyn Net) -> Result<(String, bool), Error> {
    if let Some(latest) = mirror_json::<Latest>(net, "latest.json")? {
        return Ok((latest.version, false));
    }
    match mirror_json::<Latest>(net, "latest-prerelease.json")? {
        Some(pre) => Ok((pre.version, true)),
        None => Err(Error::NoReleases(net.releases_url().to_owned())),
    }
}

/// `version`'s release for this system, or the newest for `latest`.
pub fn port_release(net: &dyn Net, version: &str) -> Result<PortRelease, Error> {
    let target = this_target()?;
    let (version, prerelease) = if version == "latest" {
        latest(net)?
    } else {
        (version.trim_start_matches('v').to_owned(), false)
    };
    let mut release = mirror_json::<PortRelease>(net, &format!("{version}/release-{target}.json"))?;
    // Releases from before the Windows and macOS builds name only Linux's.
    if release.is_none() && target == "x86_64-linux-gnu" {
        release = mirror_json(net, &format!("{version}/release.json"))?;
    }
    let mut release = match release {
        Some(release) => release,
        None if mirror_json::<PortRelease>(net, &format!("{version}/release.json"))?.is_some() => {
            return Err(Error::NoBuild {
                version,
                system: system(),
            });
        }
        None => {
            return Err(Error::NoRelease {
                version,
                mirror: net.releases_url().to_owned(),
            });
        }
    };
    if release.version != version || release.target != target {
        return Err(Error::BadRelease(format!(
            "{version}/release-{target}.json is for version {} on {}",
            release.version, release.target
        )));
    }
    if release.api != crate::API {
        return Err(Error::ReleaseApi {
            version,
            api: release.api,
        });
    }
    release.prerelease = prerelease;
    Ok(release)
}

/// The releases a player can install on this system, newest first. The
/// mirror names only its newest release, so this is that one (or, before
/// the first release, the newest prerelease).
pub fn port_releases(net: &dyn Net) -> Result<Vec<PortRelease>, Error> {
    Ok(vec![port_release(net, "latest")?])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn verified(bytes: &[u8], file: &ReleaseFile) -> bool {
    bytes.len() as u64 == file.size && hex(&Sha256::digest(bytes)) == file.sha256
}

/// Download `file` of `release`, checked against its size and SHA-256. With
/// a `cache` folder, a copy there with the right hash is used instead, and a
/// download is kept there as `<cache>/<version>/<name>`.
pub fn fetch_release_file(
    net: &dyn Net,
    release: &PortRelease,
    file: &ReleaseFile,
    cache: Option<&Path>,
    on_bytes: &mut dyn FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> Result<Vec<u8>, Error> {
    let cached = cache.map(|cache| cache.join(&release.version).join(&file.name));
    if let Some(path) = &cached
        && let Ok(bytes) = std::fs::read(path)
        && verified(&bytes, file)
    {
        return Ok(bytes);
    }
    let url = format!("{}/{}/{}", net.releases_url(), release.version, file.name);
    let bytes = net
        .download(&url, file.size, on_bytes, cancel)
        .map_err(Error::from_net)?;
    if !verified(&bytes, file) {
        return Err(Error::ReleaseChecksum {
            name: file.name.clone(),
        });
    }
    if let Some(path) = &cached {
        let folder = path.parent().expect("a cached file has a folder");
        std::fs::create_dir_all(folder).map_err(Error::io(folder))?;
        let partial = folder.join(format!("{}.partial", file.name));
        std::fs::write(&partial, &bytes).map_err(Error::io(&partial))?;
        std::fs::rename(&partial, path).map_err(Error::io(path))?;
    }
    Ok(bytes)
}

/// Unpack the archive `bytes` of `file`, whose entries all sit in its top
/// folder, into `dest`, without that folder. Only plain files and folders
/// are taken, and none may leave `dest`.
pub fn unpack(bytes: &[u8], file: &ReleaseFile, dest: &Path) -> Result<(), Error> {
    let top = file.top()?;
    let bad = |what: String| Error::BadRelease(format!("{} {what}", file.name));
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    std::fs::create_dir_all(dest).map_err(Error::io(dest))?;
    let entries = archive
        .entries()
        .map_err(|error| bad(format!("isn't readable: {error}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|error| bad(format!("isn't readable: {error}")))?;
        let name = entry
            .path()
            .map_err(|error| bad(format!("isn't readable: {error}")))?
            .into_owned();
        let relative = name
            .strip_prefix(top)
            .map_err(|_| bad(format!("holds {}, outside {top}/", name.display())))?
            .to_owned();
        if !relative
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(bad(format!("holds {}", name.display())));
        }
        let target = dest.join(&relative);
        match entry.header().entry_type() {
            tar::EntryType::Directory => {
                std::fs::create_dir_all(&target).map_err(Error::io(&target))?;
            }
            tar::EntryType::Regular => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(Error::io(parent))?;
                }
                entry.unpack(&target).map_err(Error::io(&target))?;
            }
            other => {
                return Err(bad(format!(
                    "holds {} of type {other:?}; releases hold only files and folders",
                    name.display()
                )));
            }
        }
    }
    Ok(())
}

/// Unpack the archive `bytes` of `file` into `dest`, beside it first and
/// moved in once whole, so an interrupted install leaves nothing behind.
pub fn install_archive(bytes: &[u8], file: &ReleaseFile, dest: &Path) -> Result<(), Error> {
    let parent = dest.parent().expect("an install folder has a parent");
    let name = dest
        .file_name()
        .expect("an install folder has a name")
        .to_string_lossy();
    let staging = parent.join(format!(".{name}.partial"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(Error::io(&staging))?;
    }
    unpack(bytes, file, &staging)?;
    std::fs::rename(&staging, dest).map_err(Error::io(dest))
}

/// An installed version of the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortInstall {
    pub version: String,
    pub folder: PathBuf,
    /// The release it was installed from.
    pub release: PortRelease,
    /// Whether it is the version in use.
    pub current: bool,
}

impl PortInstall {
    pub fn executable(&self) -> PathBuf {
        self.folder.join(EXECUTABLE)
    }

    /// The game as mods see it, from its release.
    pub fn port(&self) -> Port {
        self.release.port(self.executable())
    }

    pub fn has_debug_info(&self) -> bool {
        self.folder.join(DEBUG_INFO).is_file()
    }

    fn open(ports: &Path, version: &str, current: Option<&str>) -> Result<Option<Self>, Error> {
        let folder = ports.join(version);
        let path = folder.join(RELEASE);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(Error::io(path)(error)),
        };
        let release = serde_json::from_slice(&bytes)
            .map_err(|error| Error::BadRelease(format!("{}: {error}", path.display())))?;
        Ok(Some(Self {
            version: version.to_owned(),
            folder,
            release,
            current: current == Some(version),
        }))
    }
}

fn sort_key(version: &str) -> (Option<semver::Version>, String) {
    (semver::Version::parse(version).ok(), version.to_owned())
}

/// The version `<ports>/current` names, if any: a file holding the version
/// (a link to its folder from older tgg).
pub fn current_port(ports: &Path) -> Result<Option<String>, Error> {
    let path = ports.join(CURRENT);
    if let Ok(target) = std::fs::read_link(&path) {
        return Ok(Some(target.to_string_lossy().into_owned()));
    }
    match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => Ok(Some(text.trim().to_owned())),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(path)(error)),
    }
}

/// Every installed version, oldest first.
pub fn installed_ports(ports: &Path) -> Result<Vec<PortInstall>, Error> {
    let entries = match std::fs::read_dir(ports) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(Error::io(ports)(error)),
    };
    let current = current_port(ports)?;
    let mut installs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(Error::io(ports))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == CURRENT {
            continue;
        }
        if let Some(install) = PortInstall::open(ports, &name, current.as_deref())? {
            installs.push(install);
        }
    }
    installs.sort_by_key(|install| sort_key(&install.version));
    Ok(installs)
}

/// The installed `version`.
pub fn installed_port(ports: &Path, version: &str) -> Result<PortInstall, Error> {
    let version = version.trim_start_matches('v');
    let current = current_port(ports)?;
    PortInstall::open(ports, version, current.as_deref())?
        .ok_or_else(|| Error::PortNotInstalled(version.to_owned()))
}

/// Make the installed `version` the one in use, replacing `current` in one
/// step.
pub fn use_port(ports: &Path, version: &str) -> Result<PortInstall, Error> {
    let mut install = installed_port(ports, version)?;
    let staging = ports.join(".current.new");
    std::fs::write(&staging, format!("{}\n", install.version)).map_err(Error::io(&staging))?;
    let current = ports.join(CURRENT);
    // A link from an older tgg
    if std::fs::symlink_metadata(&current).is_ok_and(|meta| meta.file_type().is_symlink()) {
        std::fs::remove_file(&current).map_err(Error::io(&current))?;
    }
    std::fs::rename(&staging, &current).map_err(Error::io(current))?;
    install.current = true;
    Ok(install)
}

/// The newest of `releases` that is newer than every installed version.
pub fn port_update(installed: &[PortInstall], releases: &[PortRelease]) -> Option<PortRelease> {
    let newest = installed
        .iter()
        .map(|install| sort_key(&install.version))
        .max();
    releases
        .iter()
        .filter(|release| {
            newest
                .as_ref()
                .is_none_or(|newest| sort_key(&release.version) > *newest)
        })
        .max_by_key(|release| sort_key(&release.version))
        .cloned()
}

/// Install `release` into `ports` without changing the version in use;
/// nothing to do when it is installed. With a `cache` folder, the archive is
/// taken from or kept there as [`fetch_release_file`] does.
pub fn add_port(
    net: &dyn Net,
    release: &PortRelease,
    ports: &Path,
    cache: Option<&Path>,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<PortInstall, Error> {
    if let Ok(install) = installed_port(ports, &release.version) {
        return Ok(install);
    }
    std::fs::create_dir_all(ports).map_err(Error::io(ports))?;
    let folder = ports.join(&release.version);
    let game = &release.files.game;
    let downloading = format!("Downloading tgg-melee {}", release.version);
    progress(Progress {
        step: 0,
        steps: 2,
        label: downloading.clone(),
        bytes: 0,
        total: Some(game.size),
    });
    let bytes = fetch_release_file(
        net,
        release,
        game,
        cache,
        &mut |bytes, _| {
            progress(Progress {
                step: 0,
                steps: 2,
                label: downloading.clone(),
                bytes,
                total: Some(game.size),
            })
        },
        cancel,
    )?;
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    progress(Progress {
        step: 1,
        steps: 2,
        label: format!("Installing tgg-melee {}", release.version),
        bytes: 0,
        total: None,
    });
    // release.json is written last, so a folder without it is an install
    // that didn't finish.
    if folder.exists() {
        std::fs::remove_dir_all(&folder).map_err(Error::io(&folder))?;
    }
    install_archive(&bytes, game, &folder)?;
    let json = serde_json::to_string_pretty(release).expect("a release serializes") + "\n";
    let path = folder.join(RELEASE);
    std::fs::write(&path, json).map_err(Error::io(path))?;
    installed_port(ports, &release.version)
}

/// Install `release` into `ports` and make it the version in use. The
/// previous version stays installed until [`remove_port`].
pub fn install_port(
    net: &dyn Net,
    release: &PortRelease,
    ports: &Path,
    progress: OnProgress,
    cancel: &AtomicBool,
) -> Result<PortInstall, Error> {
    add_port(net, release, ports, None, progress, cancel)?;
    use_port(ports, &release.version)
}

/// Remove the installed `version`. When it was the one in use, the newest
/// remaining version becomes the one in use, if there is one.
pub fn remove_port(ports: &Path, version: &str) -> Result<(), Error> {
    let install = installed_port(ports, version)?;
    std::fs::remove_dir_all(&install.folder).map_err(Error::io(&install.folder))?;
    if install.current {
        let current = ports.join(CURRENT);
        std::fs::remove_file(&current).map_err(Error::io(current))?;
        if let Some(newest) = installed_ports(ports)?.last() {
            use_port(ports, &newest.version)?;
        }
    }
    Ok(())
}

/// A game [`launch`] started. While it runs, changes to its mods folder
/// return [`Error::GameRunning`], from any tgg, through the pid file in the
/// mods folder; the pid file is removed once the game is seen to end.
#[derive(Debug)]
pub struct Running {
    child: std::process::Child,
    pid_file: PathBuf,
    /// A profile game's data folder and pid, to clear its mark when it ends.
    pub(crate) profile: Option<(PathBuf, u32)>,
}

impl Running {
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// The game's exit status once it has ended.
    pub fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.ended();
        }
        Ok(status)
    }

    fn ended(&mut self) {
        let _ = std::fs::remove_file(&self.pid_file);
        if let Some((data, pid)) = self.profile.take() {
            crate::prepare::finished(&data, pid);
        }
    }

    /// Wait for the game to end.
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        let status = self.child.wait()?;
        self.ended();
        Ok(status)
    }

    /// End the game now.
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()?;
        self.wait().map(|_| ())
    }
}

/// Start `port` with the disc image `iso`, loading mods from `mods`.
pub fn launch(port: &PortInstall, iso: &Path, mods: &Path) -> Result<Running, Error> {
    launch_with_args(port, iso, mods, std::iter::empty::<&OsStr>(), None)
}

/// [`launch`], passing `args` on to the game after the disc image, and
/// writing the game's output (stdout and stderr) to the file `log` instead
/// of passing it through, when there is one.
pub fn launch_with_args<I, S>(
    port: &PortInstall,
    iso: &Path,
    mods: &Path,
    args: I,
    log: Option<&Path>,
) -> Result<Running, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if lock::game_running(mods) {
        return Err(Error::GameRunning);
    }
    spawn(port, iso, mods, args, &[], log)
}

/// [`launch_with_args`] with the variables `env` set for the game too.
pub fn launch_with_env<I, S>(
    port: &PortInstall,
    iso: &Path,
    mods: &Path,
    args: I,
    env: &[(String, std::ffi::OsString)],
    log: Option<&Path>,
) -> Result<Running, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if lock::game_running(mods) {
        return Err(Error::GameRunning);
    }
    spawn(port, iso, mods, args, env, log)
}

/// Start `port` with `iso`, `args` and the environment `env` beside
/// `TGG_MODS_DIR`, and write its pid file into `mods`.
pub(crate) fn spawn<I, S>(
    port: &PortInstall,
    iso: &Path,
    mods: &Path,
    args: I,
    env: &[(String, std::ffi::OsString)],
    log: Option<&Path>,
) -> Result<Running, Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let executable = port.executable();
    let mut command = std::process::Command::new(&executable);
    command.arg(iso).args(args).env("TGG_MODS_DIR", mods);
    command.envs(env.iter().map(|(name, value)| (name, value)));
    if let Some(log) = log {
        if let Some(folder) = log.parent() {
            std::fs::create_dir_all(folder).map_err(Error::io(folder))?;
        }
        let file = std::fs::File::create(log).map_err(Error::io(log))?;
        let copy = file.try_clone().map_err(Error::io(log))?;
        command.stdout(file).stderr(copy);
    }
    let child = command.spawn().map_err(Error::io(&executable))?;
    let pid_file = lock::write_pid_file(mods, child.id(), &executable)?;
    Ok(Running {
        child,
        pid_file,
        profile: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A release archive of `entries`, each a path and an entry type.
    fn archive(entries: &[(&str, tar::EntryType)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (name, kind) in entries {
            let mut header = tar::Header::new_gnu();
            // Written raw, so a hostile path reaches the reader as it would.
            header.as_gnu_mut().expect("gnu").name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(*kind);
            let data: &[u8] = if *kind == tar::EntryType::Regular {
                b"hi"
            } else {
                b""
            };
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, data).expect("append");
        }
        builder
            .into_inner()
            .expect("finish")
            .finish()
            .expect("gzip")
    }

    #[test]
    fn unpacking_takes_only_files_and_folders_inside_the_release_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = ReleaseFile {
            name: "tgg-melee-0.1.0-x86_64-linux.tar.gz".into(),
            size: 0,
            sha256: String::new(),
        };
        let top = "tgg-melee-0.1.0-x86_64-linux";
        let good = archive(&[
            (top, tar::EntryType::Directory),
            (&format!("{top}/lib/libz.so.1"), tar::EntryType::Regular),
        ]);
        unpack(&good, &file, &dir.path().join("ok")).expect("unpack");
        assert!(dir.path().join("ok/lib/libz.so.1").is_file());

        for (name, kind) in [
            (format!("{top}/../escape"), tar::EntryType::Regular),
            ("elsewhere/x".to_owned(), tar::EntryType::Regular),
            (format!("{top}/link"), tar::EntryType::Symlink),
        ] {
            let bad = archive(&[(&name, kind)]);
            assert!(
                unpack(&bad, &file, &dir.path().join("bad")).is_err(),
                "{name}"
            );
        }
        assert!(!dir.path().join("escape").exists());
    }
}
