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
use std::path::PathBuf;
use tgg_mod::Port;

/// The game's executable in its folder.
pub const EXECUTABLE: &str = if cfg!(windows) {
    "tgg-melee.exe"
} else {
    "tgg-melee"
};
/// Its debug info, beside it (Linux).
const DEBUG_INFO: &str = "tgg-melee.debug";
/// The mods a release ships for players, in the game's folder.
const DEFAULT_MODS: &str = "default-mods";
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
        self.folder.join(DEBUG_INFO).is_file()
    }

    /// The folder of the mods the release ships for players.
    pub fn default_mods(&self) -> PathBuf {
        self.folder.join(DEFAULT_MODS)
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

/// The version `current` names, if any: a file holding the version (a link
/// to its folder from older tgg).
pub fn current_version() -> Result<Option<String>> {
    let path = crate::paths::ports()?.join(CURRENT);
    if let Ok(target) = std::fs::read_link(&path) {
        return Ok(Some(target.to_string_lossy().into_owned()));
    }
    match std::fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => Ok(Some(text.trim().to_owned())),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| path.display().to_string()),
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

/// Make `version` the one in use, replacing `current` in one step.
fn set_current(version: &str) -> Result<()> {
    let root = crate::paths::ports()?;
    let staging = root.join(".current.new");
    std::fs::write(&staging, format!("{version}\n"))
        .with_context(|| staging.display().to_string())?;
    let current = root.join(CURRENT);
    // A link from an older tgg
    if std::fs::symlink_metadata(&current).is_ok_and(|meta| meta.file_type().is_symlink()) {
        std::fs::remove_file(&current).with_context(|| current.display().to_string())?;
    }
    std::fs::rename(&staging, current).context("setting the version in use")?;
    Ok(())
}

/// Install `version` (and its debug info), unless it is installed.
pub fn install(version: &str, debug: bool) -> Result<Installed> {
    let mirror = Mirror::new();
    let release = mirror.release(version)?;
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
        let Some(file) = &release.files.debug else {
            bail!(
                "tgg-melee {} has no debug info for {}",
                release.version,
                releases::system()
            );
        };
        let archive = mirror.download(&release, file)?;
        releases::unpack(&archive, file, &installed.folder)?;
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
                return Ok(());
            }
            crate::defaults::install(&installed)?;
            let runtime = check_runtime(&installed);
            if let Err(error) = &runtime {
                println!("{error:#}");
            }
            if crate::config::iso_or_ask().is_ok() && runtime.is_ok() {
                println!("Ready: tgg port run");
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
            crate::defaults::install(&installed)
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
            check_runtime(&installed)?;
            let iso = crate::config::iso_or_ask()?;
            let status = crate::signals::run(
                std::process::Command::new(installed.executable())
                    .arg(iso)
                    .args(&args),
            )
            .with_context(|| format!("running {}", installed.executable().display()))?;
            check_exit(status)
        }
    }
}

/// The Microsoft C++ runtime DLLs the Windows build's libraries (Dawn, the
/// shader compiler, nod) link, which Windows doesn't always have.
const VC_RUNTIME: &[&str] = &["msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll"];
/// How to get them.
pub const VC_RUNTIME_FIX: &str = "winget install Microsoft.VCRedist.2015+.x64";

/// The C++ runtime DLLs missing from both the game's folder and Windows'
/// own; none anywhere else.
pub fn missing_runtime(folder: Option<&std::path::Path>) -> Vec<&'static str> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let system = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32");
    VC_RUNTIME
        .iter()
        .copied()
        .filter(|dll| {
            !system.join(dll).is_file() && !folder.is_some_and(|folder| folder.join(dll).is_file())
        })
        .collect()
}

/// Fail with the fix when the game can't start for want of a runtime.
pub fn check_runtime(installed: &Installed) -> Result<()> {
    let missing = missing_runtime(Some(&installed.folder));
    ensure!(
        missing.is_empty(),
        "tgg-melee needs the Microsoft Visual C++ runtime ({} missing): {VC_RUNTIME_FIX}",
        missing.join(", ")
    );
    Ok(())
}

/// Windows' "a DLL was not found" status, which a game missing a library
/// exits with before it prints anything.
const STATUS_DLL_NOT_FOUND: i32 = 0xC000_0135_u32 as i32;

/// Fail when the game didn't exit cleanly, saying why if tgg can tell.
pub fn check_exit(status: std::process::ExitStatus) -> Result<()> {
    if status.success() {
        return Ok(());
    }
    if cfg!(windows) && status.code() == Some(STATUS_DLL_NOT_FOUND) {
        bail!(
            "tgg-melee couldn't start: a DLL it needs is missing. {VC_RUNTIME_FIX} installs the usual one"
        );
    }
    bail!("tgg-melee exited with {status}")
}
