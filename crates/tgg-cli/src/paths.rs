//! Where `tgg` keeps what it installs and downloads, and its settings, and
//! finding programs on `PATH`.
//!
//! Installed games go in `<data>/tgg/ports/<version>/`, with `current` naming
//! the default, and SDKs in `<data>/tgg/sdks/<game layout>/`; downloads are
//! cached in `<cache>/tgg/downloads/`, and settings live in
//! `<config>/tgg/config.json`. On Linux those folders are `$XDG_DATA_HOME`
//! (`~/.local/share`), `$XDG_CACHE_HOME` (`~/.cache`) and `$XDG_CONFIG_HOME`
//! (`~/.config`); on macOS `~/Library/Application Support`, `~/Library/Caches`
//! and `~/Library/Application Support`; on Windows `%LOCALAPPDATA%` for data
//! and cache and `%APPDATA%` for settings. The game keeps its own data, mods
//! among them, in its own folder ([`tgg_mod::game_data_dir`]).

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

fn under(base: Option<PathBuf>, kind: &str) -> Result<PathBuf> {
    Ok(base
        .ok_or_else(|| anyhow!("this system has no {kind} folder"))?
        .join("tgg"))
}

/// The local data folder tgg keeps `tgg/` in: the `data` tgg_mod's profile
/// functions take.
pub fn data() -> Result<PathBuf> {
    dirs::data_local_dir().ok_or_else(|| anyhow!("this system has no data folder"))
}

pub fn ports() -> Result<PathBuf> {
    Ok(under(dirs::data_local_dir(), "data")?.join("ports"))
}

pub fn sdks() -> Result<PathBuf> {
    Ok(under(dirs::data_local_dir(), "data")?.join("sdks"))
}

/// What tgg remembers between runs that isn't a setting, such as the
/// default mods it has offered.
pub fn state() -> Result<PathBuf> {
    under(dirs::data_local_dir(), "data")
}

pub fn cache() -> Result<PathBuf> {
    Ok(under(dirs::cache_dir(), "cache")?.join("downloads"))
}

pub fn config() -> Result<PathBuf> {
    Ok(under(dirs::config_dir(), "config")?.join("config.json"))
}

/// `program` as a file in a folder of `PATH` (with `.exe` on Windows), if
/// there is one.
pub fn find_program(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| program_in(&dir, program))
}

/// `program` in `dir`, with `.exe` on Windows.
pub fn program_in(dir: &Path, program: &str) -> Option<PathBuf> {
    let file = dir.join(format!("{program}{}", std::env::consts::EXE_SUFFIX));
    file.is_file().then_some(file)
}

pub fn on_path(program: &str) -> bool {
    find_program(program).is_some()
}
