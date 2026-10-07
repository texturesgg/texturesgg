//! `tgg port`: install released builds of tgg-melee, pick the one to use,
//! and run it.
//!
//! Each installed version is the release's game folder, unpacked to
//! `ports/<version>/` with its `release.json` beside it; `ports/current`
//! links to the version commands use by default. Every version shares the
//! game's own data: its mods folder, pack and saves.

use crate::releases::{self, Mirror, Release};
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::Subcommand;
use std::path::{Path, PathBuf};
use tgg_mod::Port;

/// The game's executable in its folder.
pub const EXECUTABLE: &str = "tgg-melee";
const CURRENT: &str = "current";
const RELEASE: &str = "release.json";

#[derive(Subcommand)]
pub enum PortCommand {
    /// Download a release of tgg-melee and install it. The first one
    /// installed becomes the one tgg uses.
    Install {
        /// A version such as 0.1.0, or latest.
        #[arg(default_value = "latest")]
        version: String,
        /// Also install its debug info, for gdb (tgg mod dev --gdb).
        #[arg(long)]
        debug: bool,
    },
    /// List the installed versions.
    List,
    /// Use this installed version by default.
    Use { version: String },
    /// Remove an installed version.
    Remove { version: String },
    /// Print the folder of the version in use (or VERSION).
    Path { version: Option<String> },
    /// Run the game with your disc image (tgg config set iso).
    Run {
        /// The installed version to run [default: the one in use]
        #[arg(long)]
        version: Option<String>,
        /// Arguments for the game.
        #[arg(last = true)]
        args: Vec<String>,
    },
}

/// An installed version of the game.
pub struct Installed {
    pub version: String,
    pub folder: PathBuf,
}

impl Installed {
    pub fn executable(&self) -> PathBuf {
        self.folder.join(EXECUTABLE)
    }

    /// The release it was installed from.
    pub fn release(&self) -> Result<Release> {
        let path = self.folder.join(RELEASE);
        let bytes = std::fs::read(&path).with_context(|| path.display().to_string())?;
        serde_json::from_slice(&bytes).with_context(|| path.display().to_string())
    }

    /// The executable's own account of itself.
    pub fn port(&self) -> Result<Port> {
        let executable = self.executable();
        Port::open(&executable).with_context(|| executable.display().to_string())
    }

    pub fn has_debug_info(&self) -> bool {
        self.folder.join(format!("{EXECUTABLE}.debug")).is_file()
    }
}

/// Every installed version, oldest first.
pub fn installed() -> Result<Vec<Installed>> {
    let root = crate::paths::ports()?;
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| root.display().to_string()),
    };
    let mut versions = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name == CURRENT || !entry.path().join(RELEASE).is_file() {
            continue;
        }
        versions.push(Installed {
            folder: entry.path(),
            version: name,
        });
    }
    versions.sort_by_key(|installed| sort_key(&installed.version));
    Ok(versions)
}

fn sort_key(version: &str) -> (Option<semver::Version>, String) {
    (semver::Version::parse(version).ok(), version.to_owned())
}

/// The version `current` names, if any.
pub fn current_version() -> Result<Option<String>> {
    let link = crate::paths::ports()?.join(CURRENT);
    match std::fs::read_link(&link) {
        Ok(target) => Ok(Some(target.to_string_lossy().into_owned())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| link.display().to_string()),
    }
}

/// The installed `version`, or the one in use.
pub fn resolve(version: Option<&str>) -> Result<Installed> {
    let version = match version {
        Some(version) => version.trim_start_matches('v').to_owned(),
        None => current_version()?
            .ok_or_else(|| anyhow!("tgg-melee isn't installed; run tgg port install"))?,
    };
    let folder = crate::paths::ports()?.join(&version);
    ensure!(
        folder.join(RELEASE).is_file(),
        "tgg-melee {version} isn't installed; run tgg port install {version}"
    );
    Ok(Installed { version, folder })
}

/// Point `current` at `version`, replacing the link in one step.
fn set_current(version: &str) -> Result<()> {
    let root = crate::paths::ports()?;
    let staging = root.join(".current.new");
    let _ = std::fs::remove_file(&staging);
    symlink(version, &staging)?;
    std::fs::rename(&staging, root.join(CURRENT)).context("setting the version in use")?;
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &str, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).with_context(|| link.display().to_string())
}

#[cfg(not(unix))]
fn symlink(_: &str, _: &Path) -> Result<()> {
    bail!("tgg-melee releases are for Linux")
}

fn check_host() -> Result<()> {
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "tgg-melee releases are for Linux x86-64 so far"
    );
    Ok(())
}

/// Install `version` (and its debug info), unless it is installed.
pub fn install(version: &str, debug: bool) -> Result<Installed> {
    check_host()?;
    let mirror = Mirror::new();
    let release = mirror.release(version)?;
    ensure!(
        release.target == releases::TARGET,
        "tgg-melee {} is built for {}",
        release.version,
        release.target
    );
    let root = crate::paths::ports()?;
    std::fs::create_dir_all(&root).with_context(|| root.display().to_string())?;
    let folder = root.join(&release.version);
    // release.json is written last, so a folder without it is an install
    // that didn't finish.
    if folder.join(RELEASE).is_file() {
        println!("tgg-melee {} is installed", release.version);
    } else {
        if folder.exists() {
            std::fs::remove_dir_all(&folder).with_context(|| folder.display().to_string())?;
        }
        releases::install(&mirror, &release, &release.files.game, &folder)?;
        let json = serde_json::to_string_pretty(&release).expect("json");
        std::fs::write(folder.join(RELEASE), json + "\n")?;
        println!(
            "Installed tgg-melee {} (game layout {})",
            release.version, release.game_abi
        );
    }
    let installed = Installed {
        version: release.version.clone(),
        folder,
    };
    if debug && !installed.has_debug_info() {
        let archive = mirror.download(&release, &release.files.debug)?;
        releases::unpack(&archive, &release.files.debug, &installed.folder)?;
        println!("Installed tgg-melee {}'s debug info", release.version);
    }
    if current_version()?.is_none() {
        set_current(&release.version)?;
    }
    Ok(installed)
}

pub fn run(command: PortCommand) -> Result<()> {
    match command {
        PortCommand::Install { version, debug } => {
            let installed = install(&version, debug)?;
            if current_version()?.as_deref() != Some(installed.version.as_str()) {
                println!(
                    "tgg uses {}; tgg port use {} to switch",
                    current_version()?.unwrap_or_default(),
                    installed.version
                );
            }
            Ok(())
        }
        PortCommand::List => {
            let current = current_version()?;
            let installed = installed()?;
            if installed.is_empty() {
                println!("tgg-melee isn't installed; run tgg port install");
            }
            for port in installed {
                let layout = port
                    .release()
                    .map(|release| release.game_abi)
                    .unwrap_or_else(|_| "?".into());
                let mut notes = Vec::new();
                if current.as_deref() == Some(port.version.as_str()) {
                    notes.push("in use");
                }
                if port.has_debug_info() {
                    notes.push("debug info");
                }
                let notes = if notes.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", notes.join(", "))
                };
                println!("{} game layout {layout}{notes}", port.version);
            }
            Ok(())
        }
        PortCommand::Use { version } => {
            let installed = resolve(Some(&version))?;
            set_current(&installed.version)?;
            println!("Using tgg-melee {}", installed.version);
            Ok(())
        }
        PortCommand::Remove { version } => {
            let installed = resolve(Some(&version))?;
            let current = current_version()?;
            std::fs::remove_dir_all(&installed.folder)
                .with_context(|| installed.folder.display().to_string())?;
            if current.as_deref() == Some(installed.version.as_str()) {
                let link = crate::paths::ports()?.join(CURRENT);
                std::fs::remove_file(&link).with_context(|| link.display().to_string())?;
                if let Some(newest) = self::installed()?.last() {
                    set_current(&newest.version)?;
                    println!("Using tgg-melee {}", newest.version);
                }
            }
            Ok(())
        }
        PortCommand::Path { version } => {
            println!("{}", resolve(version.as_deref())?.folder.display());
            Ok(())
        }
        PortCommand::Run { version, args } => {
            let installed = resolve(version.as_deref())?;
            let status = crate::signals::run(
                std::process::Command::new(installed.executable())
                    .arg(crate::config::iso()?)
                    .args(&args),
            )
            .with_context(|| format!("running {}", installed.executable().display()))?;
            if !status.success() {
                bail!("tgg-melee exited with {status}");
            }
            Ok(())
        }
    }
}
