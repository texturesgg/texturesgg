//! `tgg config`: settings kept in `config.json` ([`crate::paths::config`]).

use anyhow::{Context, Result, anyhow, ensure};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

pub fn run(command: ConfigCommand) -> Result<()> {
    let mut config = Config::read()?;
    match command {
        ConfigCommand::Set {
            key: Key::Iso,
            value,
        } => {
            ensure!(value.is_file(), "{} isn't a file", value.display());
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
