//! `tgg port`: install released builds of tgg-melee, pick the one to use,
//! and run it, through [`tgg_mod::ports`].
//!
//! Each installed version is the release's game folder, unpacked to
//! `ports/<version>/` with its `release.json` beside it; `ports/current`
//! names the version commands use by default. Every version shares the
//! game's own data: its mods folder, pack and saves.

use crate::releases::{self, Mirror, Release};
use anyhow::{Context, Result, anyhow, bail, ensure};
use clap::Subcommand;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tgg_mod::{Port, ports};

/// The game's executable in its folder.
pub use tgg_mod::ports::EXECUTABLE;
/// Its debug info, beside it (Linux).
const DEBUG_INFO: &str = "tgg-melee.debug";
/// The mods a release ships for players, in the game's folder.
const DEFAULT_MODS: &str = "default-mods";
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
        /// Show the game's own output as it runs. Otherwise it goes to
        /// game.log in tgg's data folder, and its end is shown if the game
        /// fails.
        #[arg(short, long)]
        verbose: bool,
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
    Ok(ports::installed_ports(&crate::paths::ports()?)?
        .into_iter()
        .map(|install| Installed {
            version: install.version,
            folder: install.folder,
        })
        .collect())
}

/// The version `current` names, if any.
pub fn current_version() -> Result<Option<String>> {
    Ok(ports::current_port(&crate::paths::ports()?)?)
}

/// The installed `version`, or the one in use.
pub fn resolve(version: Option<&str>) -> Result<Installed> {
    let version = match version {
        Some(version) => version.trim_start_matches('v').to_owned(),
        None => current_version()?
            .ok_or_else(|| anyhow!("tgg-melee isn't installed; run tgg port install"))?,
    };
    match ports::installed_port(&crate::paths::ports()?, &version) {
        Ok(install) => Ok(Installed {
            version: install.version,
            folder: install.folder,
        }),
        Err(tgg_mod::Error::PortNotInstalled(_)) => {
            bail!("tgg-melee {version} isn't installed; run tgg port install {version}")
        }
        Err(error) => Err(error.into()),
    }
}

/// Make `version` the one in use.
fn set_current(version: &str) -> Result<()> {
    ports::use_port(&crate::paths::ports()?, version).context("setting the version in use")?;
    Ok(())
}

/// Install `version` (and its debug info), unless it is installed.
pub fn install(version: &str, debug: bool) -> Result<Installed> {
    let mirror = Mirror::new();
    let release = mirror.release(version)?;
    // On stdout: PowerShell shows a program's stderr as an error.
    if let Some(note) = releases::prerelease_note(&release) {
        println!("{note}");
    }
    let root = crate::paths::ports()?;
    if ports::installed_port(&root, &release.version).is_ok() {
        println!("tgg-melee {} is installed", release.version);
    } else {
        let told = AtomicBool::new(false);
        let progress = |progress: tgg_mod::Progress| {
            if progress.step == 0 && progress.bytes > 0 && !told.swap(true, Ordering::Relaxed) {
                println!("{}", releases::downloading(&release.files.game));
            }
        };
        ports::add_port(
            mirror.net(),
            &release,
            &root,
            Some(&crate::paths::cache()?),
            &progress,
            &AtomicBool::new(false),
        )?;
        println!(
            "Installed tgg-melee {} (game layout {})",
            release.version, release.game_abi
        );
    }
    let installed = Installed {
        version: release.version.clone(),
        folder: root.join(&release.version),
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
        ports::unpack(&archive, file, &installed.folder)?;
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
            match crate::config::iso_or_ask() {
                Ok(_) if runtime.is_ok() => {
                    println!("Ready: tgg port run");
                    println!("Sets of mods to add: tgg mod sets");
                }
                Ok(_) => {}
                // In a terminal the question already said how to set it later.
                Err(error) if !std::io::IsTerminal::is_terminal(&std::io::stdin()) => {
                    println!("{error:#}")
                }
                Err(_) => {}
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
            ports::remove_port(&crate::paths::ports()?, &installed.version)?;
            if current.as_deref() == Some(installed.version.as_str())
                && let Some(newest) = current_version()?
            {
                println!("Using tgg-melee {newest}");
            }
            Ok(())
        }
        PortCommand::Path { version } => {
            println!("{}", resolve(version.as_deref())?.folder.display());
            Ok(())
        }
        PortCommand::Run {
            version,
            verbose,
            args,
        } => {
            let installed = resolve(version.as_deref())?;
            check_runtime(&installed)?;
            let iso = crate::config::iso_or_ask()?;
            let install = ports::installed_port(&crate::paths::ports()?, &installed.version)?;
            let log = crate::paths::state()?.join(GAME_LOG);
            println!("Starting tgg-melee {}", installed.version);
            let signals = crate::signals::handle()?;
            let mut running = ports::launch_with_args(
                &install,
                &iso,
                &tgg_mod::default_mods_dir(),
                &args,
                (!verbose).then_some(log.as_path()),
            )?;
            signals.set_game(Some(running.id()));
            let status = running.wait();
            signals.set_game(None);
            let status =
                status.with_context(|| format!("running {}", installed.executable().display()))?;
            if !status.success() && !verbose && !signals.stopping() {
                print_tail(&log);
            }
            check_exit(status)
        }
    }
}

/// The game's output from the last `tgg port run`, in tgg's data folder.
const GAME_LOG: &str = "game.log";
/// The lines of it shown when the game fails.
const TAIL: usize = 20;

/// Show the end of the game's log, and where the rest is.
fn print_tail(log: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(log) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    for line in &lines[lines.len().saturating_sub(TAIL)..] {
        eprintln!("{line}");
    }
    eprintln!("The game's full output: {}", log.display());
}

/// The Microsoft C++ runtime DLLs the Windows build's libraries (Dawn, the
/// shader compiler, nod) link, which Windows doesn't always have.
const VC_RUNTIME: &[&str] = &[
    "msvcp140.dll",
    "msvcp140_atomic_wait.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
];
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
