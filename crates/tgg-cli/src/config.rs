//! `tgg config`: settings kept in `config.json` ([`crate::paths::config`]).

use anyhow::{Context, Result, anyhow, ensure};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    /// The Melee disc image the game runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iso: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Key {
    /// Your Melee disc image (NTSC 1.02), which `tgg port run` and
    /// `tgg mod dev` start the game with. TGG_MELEE_ISO overrides it.
    Iso,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Set a setting.
    Set { key: Key, value: PathBuf },
    /// Print a setting.
    Get { key: Key },
    /// Forget a setting.
    Unset { key: Key },
}

impl Config {
    pub fn read() -> Result<Self> {
        let path = crate::paths::config()?;
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| path.display().to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| path.display().to_string()),
        }
    }

    fn write(&self) -> Result<()> {
        let path = crate::paths::config()?;
        let folder = path.parent().expect("the config file has a folder");
        std::fs::create_dir_all(folder).with_context(|| folder.display().to_string())?;
        let mut json = serde_json::to_string_pretty(self).expect("json");
        json.push('\n');
        std::fs::write(&path, json).with_context(|| path.display().to_string())
    }
}

/// The disc image to run the game with: `TGG_MELEE_ISO`, else the one set
/// with `tgg config set iso`.
pub fn iso() -> Result<PathBuf> {
    let iso = match std::env::var_os("TGG_MELEE_ISO").filter(|iso| !iso.is_empty()) {
        Some(iso) => PathBuf::from(iso),
        None => Config::read()?.iso.ok_or_else(|| {
            anyhow!("tgg needs your Melee disc image (NTSC 1.02): tgg config set iso <path>")
        })?,
    };
    ensure!(iso.is_file(), "{} isn't a file", iso.display());
    Ok(iso)
}

/// The disc image, as [`iso`] finds it, or else asked for in the terminal
/// and remembered.
pub fn iso_or_ask() -> Result<PathBuf> {
    let error = match iso() {
        Ok(iso) => return Ok(iso),
        Err(error) => error,
    };
    if !std::io::stdin().is_terminal() || std::env::var_os("TGG_MELEE_ISO").is_some() {
        return Err(error);
    }
    loop {
        print!("Your Melee disc image (NTSC 1.02, .iso), or Enter to skip: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line)? == 0 {
            return Err(error);
        }
        // Pasted paths come quoted ("Copy as path" on Windows) or escaped
        // (dragged into a macOS terminal).
        let line = line.trim().trim_matches(['"', '\'']).replace("\\ ", " ");
        if line.is_empty() {
            println!("Later: tgg config set iso <path>");
            return Err(error);
        }
        let path = expand_home(&line);
        match check_disc(&path) {
            Ok(()) => {
                let path = std::path::absolute(&path)?;
                let mut config = Config::read()?;
                config.iso = Some(path.clone());
                config.write()?;
                println!("Saved; tgg uses it from now on");
                return Ok(path);
            }
            Err(problem) => println!("{problem:#}"),
        }
    }
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// Check `path` is Melee NTSC 1.02. Only a plain disc image (`.iso`, `.gcm`)
/// is read; the game also opens compressed ones (`.rvz`, `.ciso`, ...).
fn check_disc(path: &Path) -> Result<()> {
    ensure!(path.is_file(), "{} isn't a file", path.display());
    let plain = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("iso") || ext.eq_ignore_ascii_case("gcm"));
    if !plain {
        return Ok(());
    }
    let mut header = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .with_context(|| path.display().to_string())?;
    ensure!(
        &header[..6] == b"GALE01",
        "{} isn't Melee NTSC (its game id is {})",
        path.display(),
        String::from_utf8_lossy(&header[..6])
    );
    ensure!(
        header[7] == 2,
        "{} is Melee NTSC 1.0{}; tgg-melee needs 1.02",
        path.display(),
        header[7]
    );
    Ok(())
}

pub fn run(command: ConfigCommand) -> Result<()> {
    let mut config = Config::read()?;
    match command {
        ConfigCommand::Set {
            key: Key::Iso,
            value,
        } => {
            check_disc(&value)?;
            let value = std::path::absolute(&value)?;
            println!("iso = {}", value.display());
            config.iso = Some(value);
            config.write()
        }
        ConfigCommand::Get { key: Key::Iso } => {
            if let Some(iso) = &config.iso {
                println!("{}", iso.display());
            }
            Ok(())
        }
        ConfigCommand::Unset { key: Key::Iso } => {
            config.iso = None;
            config.write()
        }
    }
}
