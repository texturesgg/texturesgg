//! What a mod says about itself, in its package's `manifest.json`.
//!
//! An author writes the identity fields and `depends`. Packing adds
//! `game_abi`, `target`, `api_version`, `state`, `hooks`, `events`, `exports`
//! and `imports`, read from the library, and `files` and `assets`, read from
//! the mod's folders, so they never come from the author.
//!
//! A mod's netplay class is never written down: the game works out whether a
//! mod counts, and refuses a manifest with a `netplay` field. [`crate::netplay`]
//! gives the same answer from a manifest.

use crate::decls::Hooks;
use crate::depends::Range;
use crate::files::ModFile;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

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
    /// The manifest API: `tgg-melee/0`.
    pub api: String,
    pub id: ModId,
    pub name: String,
    pub version: Version,
    /// The library in the package; absent for a mod without one. A package
    /// with a library always names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Every mod this one needs, each with a version range; the game loads
    /// it after them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub depends: BTreeMap<ModId, Range>,
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
    /// The mod API `major.minor` the library was built against, from the
    /// library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    /// Bytes of the mod's own state that roll back with the game, from the
    /// library; absent when it keeps none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<u64>,
    /// The functions the library hooks, from the library.
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
    /// The events the library subscribes to, from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    /// The functions the library offers other mods, from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
    /// What the library takes from other mods, as `provider-id/export-name`,
    /// from the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<String>,
    /// The disc files the package replaces or adds under `files/`, in byte
    /// order of path, from packing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ModFile>,
    /// The new files the package ships under `assets/`, which the game
    /// serves at `/mods/<id>/<path>`, in byte order of path, from packing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ModFile>,
}

/// The library's name when the manifest names none.
pub const DEFAULT_ENTRY: &str = "mod.so";

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
    #[error(
        "manifest.json has a netplay field; remove it, the game works out what counts for netplay"
    )]
    Netplay,
    #[error("the mod depends on itself")]
    DependsOnItself,
}

impl Manifest {
    /// Read and check a manifest. Every manifest this crate hands out passed
    /// [`Manifest::validate`].
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let json = std::str::from_utf8(bytes).map_err(|_| ManifestError::Utf8)?;
        let value: serde_json::Value = serde_json::from_str(json)?;
        // The game refuses the field outright, so a manifest with it never
        // gets as far as a package.
        if value.get("netplay").is_some() {
            return Err(ManifestError::Netplay);
        }
        let manifest: Self = serde_json::from_value(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The library's file name in a package or a mod's folder.
    pub fn library_name(&self) -> &str {
        self.entry.as_deref().unwrap_or(DEFAULT_ENTRY)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a manifest serializes");
        json.push('\n');
        json
    }

    /// Check the fields the game and installers rely on that the types alone
    /// don't.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.api != crate::API {
            return Err(ManifestError::Api(self.api.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::Name);
        }
        if let Some(entry) = &self.entry
            && (entry.is_empty() || entry.contains(['/', '\\']) || entry.starts_with('.'))
        {
            return Err(ManifestError::Entry(entry.clone()));
        }
        if self.depends.contains_key(&self.id) {
            return Err(ManifestError::DependsOnItself);
        }
        Ok(())
    }

    /// A manifest with only the fields an author writes.
    pub fn new(id: ModId, name: String, version: Version) -> Self {
        Self {
            api: crate::API.to_owned(),
            id,
            name,
            version,
            entry: None,
            depends: BTreeMap::new(),
            description: None,
            license: None,
            game_abi: None,
            target: None,
            api_version: None,
            state: None,
            hooks: Hooks::default(),
            events: Vec::new(),
            exports: Vec::new(),
            imports: Vec::new(),
            files: Vec::new(),
            assets: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_with_a_netplay_field_is_refused() {
        let json = br#"{"api": "tgg-melee/0", "id": "me.x", "name": "X", "version": "1.0.0",
            "netplay": "cosmetic"}"#;
        assert!(matches!(Manifest::parse(json), Err(ManifestError::Netplay)));
    }
}
