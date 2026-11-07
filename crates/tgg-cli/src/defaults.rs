//! The mods a release of tgg-melee comes with: built mod folders in the
//! game's `default-mods/`, installed into the game's mods folder when that
//! release is the one in use.
//!
//! A default mod is installed once. One the player removes stays removed:
//! `default-mods.json` in tgg's data folder lists every one tgg has offered.
//! One already installed is replaced when the release has a newer version or
//! one for another game layout, and keeps its on or off state; a dev build
//! (`tgg mod dev --keep`, a link) is left alone.

use crate::build;
use crate::ports::Installed;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::PathBuf;
use tgg_mod::{ModsDir, conflicts};

const STATE: &str = "default-mods.json";

#[derive(Default, Serialize, Deserialize)]
struct Offered {
    #[serde(default)]
    offered: BTreeSet<String>,
}

fn state_path() -> Result<PathBuf> {
    Ok(crate::paths::state()?.join(STATE))
}

fn read_state() -> Result<Offered> {
    let path = state_path()?;
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).with_context(|| path.display().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Offered::default()),
        Err(error) => Err(error).with_context(|| path.display().to_string()),
    }
}

fn write_state(state: &Offered) -> Result<()> {
    let path = state_path()?;
    let folder = path.parent().expect("the state file has a folder");
    std::fs::create_dir_all(folder).with_context(|| folder.display().to_string())?;
    let json = serde_json::to_string_pretty(state).expect("json") + "\n";
    std::fs::write(&path, json).with_context(|| path.display().to_string())
}

/// Install the default mods of `installed` that the mods folder lacks or has
/// an older build of.
pub fn install(installed: &Installed) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(installed.default_mods()) else {
        return Ok(());
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    let mods = ModsDir::game();
    let mut state = read_state()?;
    let (mut added, mut updated) = (Vec::new(), Vec::new());
    for folder in folders {
        let package = match build::pack_folder(&folder) {
            Ok(package) => package,
            Err(error) => {
                eprintln!("Skipped {}: {error:#}", folder.display());
                continue;
            }
        };
        let manifest = &package.manifest;
        let id = manifest.id.clone();
        let list = mods.list()?;
        match list.iter().find(|m| m.manifest.id == id) {
            Some(have) => {
                let current = have.manifest.game_abi == manifest.game_abi
                    && have.manifest.version >= manifest.version;
                let link = std::fs::symlink_metadata(mods.folder(&id, have.enabled))
                    .is_ok_and(|meta| meta.file_type().is_symlink());
                if current || link {
                    state.offered.insert(id.to_string());
                    continue;
                }
            }
            None if state.offered.contains(id.as_str()) => continue,
            None => {}
        }
        let clashes = conflicts(manifest, &list, None);
        if let Some(clash) = clashes.first() {
            eprintln!(
                "Skipped {id}: it replaces {}, and so does {}",
                clash.replaces, clash.with
            );
            continue;
        }
        let replacing = list.iter().any(|m| m.manifest.id == id);
        mods.install(&package)
            .with_context(|| format!("installing {id}"))?;
        state.offered.insert(id.to_string());
        if replacing { &mut updated } else { &mut added }.push(id.to_string());
    }
    write_state(&state)?;
    if !added.is_empty() {
        println!("Added {} mods: {}", added.len(), added.join(", "));
    }
    if !updated.is_empty() {
        println!("Updated {} mods: {}", updated.len(), updated.join(", "));
    }
    if !added.is_empty() || !updated.is_empty() {
        println!("tgg mod list shows them; tgg mod disable <id> turns one off");
    }
    Ok(())
}
