//! `tgg sdk`: the SDK mods build against, one per game layout.
//!
//! A release's SDK unpacks to `sdks/<game layout>/`. Releases that share a
//! layout (a patch release keeps its minor's) share an SDK, so a mod built
//! for 0.3.0 loads on 0.3.2. Commands that build a mod install the SDK of
//! the game in use when it's missing.

use crate::ports::{self, Installed};
use crate::releases::{self, Mirror, Release};
use anyhow::{Context, Result, bail, ensure};
use clap::{Args, Subcommand};
use std::path::{Path, PathBuf};
use tgg_mod::{Port, Sdk};

#[derive(Subcommand)]
pub enum SdkCommand {
    /// Install the SDK for a release, or for the game in use.
    Install {
        #[command(flatten)]
        which: Which,
    },
    /// List the installed SDKs.
    List,
    /// Print an SDK's folder, installing it if it's missing, as for
    /// `cmake -DTGG_SDK=$(tgg sdk path)`.
    Path {
        #[command(flatten)]
        which: Which,
    },
}

/// Which game an SDK is for.
#[derive(Args)]
pub struct Which {
    /// A release version, such as 0.1.0 [default: the game in use]
    version: Option<String>,
    /// A game executable: the SDK of the release it is.
    #[arg(long, conflicts_with = "version")]
    port: Option<PathBuf>,
}

impl Which {
    fn folder(&self) -> Result<PathBuf> {
        match (&self.version, &self.port) {
            (Some(version), _) => {
                let mirror = Mirror::new();
                for_release(&mirror, &mirror.release(version)?)
            }
            (None, Some(executable)) => for_executable(executable),
            (None, None) => for_installed(&ports::resolve(None)?),
        }
    }
}

/// The folder an SDK for `game_abi` installs to.
fn folder(game_abi: &str) -> Result<PathBuf> {
    Ok(crate::paths::sdks()?.join(game_abi))
}

/// The SDK for `release`, installed if it's missing.
pub fn for_release(mirror: &Mirror, release: &Release) -> Result<PathBuf> {
    let folder = folder(&release.game_abi)?;
    if !folder.join(tgg_mod::sdk::FILE).is_file() {
        let root = crate::paths::sdks()?;
        std::fs::create_dir_all(&root).with_context(|| root.display().to_string())?;
        releases::install(mirror, release, &release.files.sdk, &folder)?;
        eprintln!(
            "Installed the SDK for tgg-melee {} (game layout {})",
            release.version, release.game_abi
        );
    }
    let sdk = Sdk::open(&folder)?;
    ensure!(
        sdk.game_abi == release.game_abi && sdk.target == release.target,
        "{} holds the SDK for game layout {} on {}, not {} on {}",
        folder.display(),
        sdk.game_abi,
        sdk.target,
        release.game_abi,
        release.target
    );
    Ok(folder)
}

/// The SDK for an installed game.
pub fn for_installed(installed: &Installed) -> Result<PathBuf> {
    for_release(&Mirror::new(), &installed.release()?)
}

/// The SDK for the game at `executable`: the release whose version it
/// reports, as long as the layouts agree.
fn for_executable(executable: &Path) -> Result<PathBuf> {
    let port = Port::open(executable).with_context(|| executable.display().to_string())?;
    if semver::Version::parse(&port.version).is_err() {
        bail!(
            "{} is a {} build, not a release; its SDK is the tgg-melee-sdk folder of its build",
            executable.display(),
            port.version
        );
    }
    let mirror = Mirror::new();
    let release = mirror.release(&port.version)?;
    ensure!(
        release.game_abi == port.game_abi && release.target == port.target,
        "{} is game layout {} on {}, but tgg-melee {} is {} on {}",
        executable.display(),
        port.game_abi,
        port.target,
        release.version,
        release.game_abi,
        release.target
    );
    for_release(&mirror, &release)
}

pub fn run(command: SdkCommand) -> Result<()> {
    match command {
        SdkCommand::Install { which } => {
            let folder = which.folder()?;
            let sdk = Sdk::open(&folder)?;
            println!(
                "SDK for tgg-melee {} (game layout {}) in {}",
                sdk.version,
                sdk.game_abi,
                folder.display()
            );
            Ok(())
        }
        SdkCommand::List => {
            let root = crate::paths::sdks()?;
            let entries = match std::fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    println!("No SDKs installed");
                    return Ok(());
                }
                Err(error) => return Err(error).with_context(|| root.display().to_string()),
            };
            for entry in entries {
                let path = entry?.path();
                if let Ok(sdk) = Sdk::open(&path) {
                    println!(
                        "{} tgg-melee {} ({})",
                        sdk.game_abi,
                        sdk.version,
                        path.display()
                    );
                }
            }
            Ok(())
        }
        SdkCommand::Path { which } => {
            println!("{}", which.folder()?.display());
            Ok(())
        }
    }
}
