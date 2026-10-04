//! What a mod says about itself, in its package's `manifest.json`.
//!
//! An author writes the identity fields. Packing adds `game_abi`, `hooks`,
//! `exports` and `imports`, read from the library, so they never come from
//! the author.

use crate::decls::Hooks;
use serde::{Deserialize, Serialize};

/// Whether a mod changes the match. Gameplay mods enter the identity peers
/// compare before playing online; cosmetic mods never split players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Netplay {
    Cosmetic,
    Gameplay,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The runtime API the mod is written against: `tgg/1`.
    pub api: String,
    /// Unique and stable: lowercase letters, digits, `.`, `-` and `_`. It
    /// names the mod's folder once installed.
    pub id: String,
    pub name: String,
    pub version: String,
    /// The library in the package.
    #[serde(default = "default_entry")]
    pub entry: String,
    pub netplay: Netplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The game layout the library was built against, from the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_abi: Option<String>,
    /// The functions the library hooks, from the library.
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
    /// The functions the library offers other mods, from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    /// What the library takes from other mods, as `provider-id/export-name`,
    /// from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
}

fn default_entry() -> String {
    "mod.so".into()
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("the manifest targets {0}; this format is {api}", api = crate::API)]
    Api(String),
    #[error(
        "the id {0:?} must be 1 to 64 lowercase letters, digits, '.', '-' or '_', not starting with '.'"
    )]
    Id(String),
    #[error("the manifest needs a name and a version")]
    Missing,
    #[error("the entry {0:?} must be a file name, not a path")]
    Entry(String),
}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a manifest serializes");
        json.push('\n');
        json
    }

    /// Check the fields the runtime and the installer rely on.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.api != crate::API {
            return Err(ManifestError::Api(self.api.clone()));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && !self.id.starts_with('.')
            && self.id.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
            });
        if !id_ok {
            return Err(ManifestError::Id(self.id.clone()));
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(ManifestError::Missing);
        }
        if self.entry.is_empty() || self.entry.contains(['/', '\\']) || self.entry.starts_with('.')
        {
            return Err(ManifestError::Entry(self.entry.clone()));
        }
        Ok(())
    }
}
