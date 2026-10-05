//! Where `tgg` keeps what it installs and downloads, and its settings.
//!
//! Installed games go in `<data>/tgg/ports/<version>/`, with `current` naming
//! the default, and SDKs in `<data>/tgg/sdks/<game layout>/`; downloads are
//! cached in `<cache>/tgg/downloads/`, and settings live in
//! `<config>/tgg/config.json`. On Linux those folders are `$XDG_DATA_HOME`
//! (`~/.local/share`), `$XDG_CACHE_HOME` (`~/.cache`) and `$XDG_CONFIG_HOME`
//! (`~/.config`). The game keeps its own data, mods among them, in its own
//! folder ([`tgg_mod::ModsDir::game`]).

use anyhow::{Result, anyhow};
use std::path::PathBuf;

fn under(base: Option<PathBuf>, kind: &str) -> Result<PathBuf> {
    Ok(base
        .ok_or_else(|| anyhow!("this system has no {kind} folder"))?
        .join("tgg"))
}

pub fn ports() -> Result<PathBuf> {
    Ok(under(dirs::data_dir(), "data")?.join("ports"))
}

pub fn sdks() -> Result<PathBuf> {
    Ok(under(dirs::data_dir(), "data")?.join("sdks"))
}

pub fn cache() -> Result<PathBuf> {
    Ok(under(dirs::cache_dir(), "cache")?.join("downloads"))
}

pub fn config() -> Result<PathBuf> {
    Ok(under(dirs::config_dir(), "config")?.join("config.json"))
}
