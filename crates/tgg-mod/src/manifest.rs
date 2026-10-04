//! What a mod says about itself, in its package's `manifest.json`.
//!
//! An author writes the identity fields. Packing adds `game_abi`, `target`, `hooks`,
//! `exports` and `imports`, read from the library, so they never come from
//! the author.

use crate::decls::Hooks;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Whether a mod changes the match. Gameplay mods enter the identity peers
/// compare before playing online; cosmetic mods never split players.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Netplay {
    Cosmetic,
    Gameplay,
}

/// A mod's id: 1 to 64 lowercase letters, digits, `.`, `-` or `_`, not
/// starting with `.`. It names the mod's folder once installed, so a value of
/// this type is always safe to join onto a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModId(String);

impl ModId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ModId {
    type Error = ManifestError;

    fn try_from(id: String) -> Result<Self, ManifestError> {
        let valid = !id.is_empty()
            && id.len() <= 64
            && !id.starts_with('.')
            && id.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
            });
        if valid {
            Ok(Self(id))
        } else {
            Err(ManifestError::Id(id))
        }
    }
}

impl std::str::FromStr for ModId {
    type Err = ManifestError;

    fn from_str(id: &str) -> Result<Self, ManifestError> {
        id.to_owned().try_into()
    }
}

impl From<ModId> for String {
    fn from(id: ModId) -> Self {
        id.0
    }
}

impl fmt::Display for ModId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for ModId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The runtime API the mod is written against: `tgg/1`.
    pub api: String,
    pub id: ModId,
    pub name: String,
    pub version: Version,
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
    /// The target triple the library was built for, from the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
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

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("manifest.json is not a valid manifest: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manifest.json is not UTF-8")]
    Utf8,
    #[error("the manifest targets {0}; this format is {api}", api = crate::API)]
    Api(String),
    #[error(
        "the id {0:?} must be 1 to 64 lowercase letters, digits, '.', '-' or '_', not starting with '.'"
    )]
    Id(String),
    #[error("the manifest needs a name")]
    Name,
    #[error("the entry {0:?} must be a file name, not a path")]
    Entry(String),
}

impl Manifest {
    /// Read and check a manifest. Every manifest this crate hands out passed
    /// [`Manifest::validate`].
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let json = std::str::from_utf8(bytes).map_err(|_| ManifestError::Utf8)?;
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a manifest serializes");
        json.push('\n');
        json
    }

    /// Check the fields the runtime and installers rely on that the types
    /// alone don't.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.api != crate::API {
            return Err(ManifestError::Api(self.api.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Name);
        }
        if self.entry.is_empty() || self.entry.contains(['/', '\\']) || self.entry.starts_with('.')
        {
            return Err(ManifestError::Entry(self.entry.clone()));
        }
        Ok(())
    }
}
