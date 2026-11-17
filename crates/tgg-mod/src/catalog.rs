//! The list of packages a registry offers: each package's manifest, where to
//! download it, and the SHA-256 it must have.

use crate::manifest::{Manifest, ModId};
use crate::netplay::Netplay;
use crate::registry::Publisher;
use serde::{Deserialize, Serialize};

/// The catalog format this crate reads and writes.
pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    /// An id for this state of a registry's catalog; absent in a catalog
    /// written by hand or by `tgg mod catalog`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    pub mods: Vec<CatalogEntry>,
    /// Every mod id the registry has, whether or not it has a build in
    /// this catalog, so a plan can tell a mod not built for this game from
    /// one that doesn't exist. Empty when the registry doesn't say.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<ModId>,
    /// Releases a moderator pulled after approval, with every package they
    /// had on any layout; [`crate::withdraw`] acts on installed copies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub withdrawn: Vec<Withdrawn>,
}

/// A release the registry withdrew: it is no longer offered, and what tgg
/// does with an installed copy is its `mode`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withdrawn {
    pub id: ModId,
    pub version: String,
    /// Why, in a few words players read.
    pub reason: String,
    #[serde(default)]
    pub mode: WithdrawMode,
    /// Every package of the release, on every layout.
    pub sha256: Vec<String>,
}

/// What tgg does with an installed copy of a withdrawn release.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WithdrawMode {
    /// Leave it on and say why: broken, not harmful.
    Warn,
    /// Turn it off and say why. A mode this tgg doesn't know reads as this.
    #[default]
    #[serde(other)]
    Off,
}

impl Catalog {
    /// The withdrawn release a package with this SHA-256 belongs to.
    pub fn withdrawn(&self, sha256: &str) -> Option<&Withdrawn> {
        self.withdrawn
            .iter()
            .find(|entry| entry.sha256.iter().any(|hash| hash == sha256))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    #[serde(flatten)]
    pub manifest: Manifest,
    /// The class the package's contents give it.
    pub netplay: Netplay,
    pub package: PackageRef,
    /// Who publishes the mod, as the registry's mods list gives it; absent
    /// in a catalog that doesn't say (an unofficial mod by "unknown").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    /// Whether the build changes how the game plays. The registry will fill
    /// it from each build's synctest results; absent reads as `true`.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub changes_game: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRef {
    /// Absolute, or relative to the catalog's own location.
    pub url: String,
    pub sha256: String,
    pub size: u64,
    /// The registry's signature over the package, when it signs packages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("the catalog is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the catalog is format {0}, but this app reads format {SCHEMA}")]
    Schema(u32),
}

impl Catalog {
    /// A catalog with no mods.
    pub fn empty() -> Self {
        Self {
            schema: SCHEMA,
            snapshot: None,
            mods: Vec::new(),
            ids: Vec::new(),
            withdrawn: Vec::new(),
        }
    }

    pub fn parse(json: &str) -> Result<Self, CatalogError> {
        let catalog: Self = serde_json::from_str(json)?;
        if catalog.schema != SCHEMA {
            return Err(CatalogError::Schema(catalog.schema));
        }
        Ok(catalog)
    }

    pub fn to_json(&self) -> String {
        let mut json = serde_json::to_string_pretty(self).expect("a catalog serializes");
        json.push('\n');
        json
    }
}

impl PackageRef {
    /// The package's location, with a relative `url` resolved against the
    /// catalog's location `base` (a URL or a file path).
    pub fn location(&self, base: &str) -> String {
        if self.url.contains("://") || self.url.starts_with('/') {
            return self.url.clone();
        }
        match base.rfind('/') {
            Some(slash) => format!("{}/{}", &base[..slash], self.url),
            None => self.url.clone(),
        }
    }
}
