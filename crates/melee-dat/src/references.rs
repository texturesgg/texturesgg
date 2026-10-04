//! The fighter and animation files a costume plays with. They are original
//! game files: callers supply them, and each is admitted only by its catalog
//! size and SHA-256.

use crate::catalog::{MeleeReferenceCatalog, ReferenceAsset};
use crate::error::{MeleeError, Result};
use dat_parser::DatFile;
use dat_parser::hsd::HsdScene;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Catalog reference DATs by SHA-256, each admitted only when its size and
/// hash are a catalog asset's.
pub struct MeleeReferenceStore {
    entries: HashMap<String, Vec<u8>>,
}

impl MeleeReferenceStore {
    /// Keep the supplied files that match a catalog asset; drop the rest.
    pub fn from_bytes(
        catalog: &MeleeReferenceCatalog,
        files: impl IntoIterator<Item = Vec<u8>>,
    ) -> Self {
        let sizes = catalog.asset_sizes();
        let mut entries = HashMap::new();
        for bytes in files {
            if let Some(hash) = catalog_hash(&sizes, &bytes) {
                entries.entry(hash).or_insert(bytes);
            }
        }
        Self { entries }
    }

    /// The references the costume `scene` describes plays with, each asked
    /// of `read` by its catalog asset (`PlFc.dat` and its hash): a disc, a
    /// folder, a fetch. `None` when the catalog doesn't recognize the
    /// costume. A file `read` can't supply is left out, and playback says
    /// which one it misses.
    pub fn for_costume(
        catalog: &MeleeReferenceCatalog,
        scene: &HsdScene,
        read: impl FnMut(&ReferenceAsset) -> Option<Vec<u8>>,
    ) -> Option<Self> {
        let assets = catalog.idle_reference_assets(scene)?;
        Some(Self::from_bytes(
            catalog,
            assets.into_iter().filter_map(read),
        ))
    }

    /// The supplied file for `asset`, parsed.
    pub(crate) fn load_dat(&self, asset: &ReferenceAsset) -> Result<DatFile> {
        DatFile::parse(&self.load(asset)?).map_err(|source| MeleeError::InvalidReference {
            file_name: asset.file_name.clone(),
            source,
        })
    }

    pub(crate) fn load(&self, asset: &ReferenceAsset) -> Result<Vec<u8>> {
        self.entries
            .get(&asset.sha256)
            .cloned()
            .ok_or_else(|| MeleeError::MissingReference {
                file_name: asset.file_name.clone(),
                sha256: asset.sha256.clone(),
            })
    }
}

fn catalog_hash(sizes: &HashMap<usize, Vec<&str>>, bytes: &[u8]) -> Option<String> {
    let candidates = sizes.get(&bytes.len())?;
    let hash = format!("{:x}", Sha256::digest(bytes));
    candidates.contains(&hash.as_str()).then_some(hash)
}
