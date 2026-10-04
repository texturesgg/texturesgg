//! The list of packages a registry offers: each package's manifest, where to
//! download it, and the SHA-256 it must have.

use crate::manifest::Manifest;
use serde::{Deserialize, Serialize};

/// The catalog format this crate reads and writes.
pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    pub mods: Vec<CatalogEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub package: PackageRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRef {
    /// Absolute, or relative to the catalog's own location.
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("the catalog is not valid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the catalog is format {0}; this app reads format {SCHEMA}")]
    Schema(u32),
}

impl Catalog {
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
