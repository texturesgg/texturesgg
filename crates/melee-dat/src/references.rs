//! The fighter and animation files a costume plays with. They are original
//! game files: callers supply them, and each is admitted only by its catalog
//! size and SHA-256.

use crate::catalog::{MeleeReferenceCatalog, ReferenceAsset};
use crate::error::{Result, playback_error};
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
    /// of `read` by its file name (`PlFc.dat`): a disc, a folder, a fetch.
    /// `None` when the catalog doesn't recognize the costume. A file `read`
    /// can't supply is left out, and playback says which one it misses.
    pub fn for_costume(
        catalog: &MeleeReferenceCatalog,
        scene: &HsdScene,
        read: impl FnMut(&str) -> Option<Vec<u8>>,
    ) -> Option<Self> {
        let names = catalog.idle_reference_files(scene)?;
        Some(Self::from_bytes(
            catalog,
            names.into_iter().filter_map(read),
        ))
    }

    pub(crate) fn load(&self, asset: &ReferenceAsset) -> Result<Vec<u8>> {
        self.entries.get(&asset.sha256).cloned().ok_or_else(|| {
            playback_error(format!(
                "reference {} ({}) is not available",
                asset.file_name, asset.sha256
            ))
        })
    }
}

fn catalog_hash(sizes: &HashMap<usize, Vec<&str>>, bytes: &[u8]) -> Option<String> {
    let candidates = sizes.get(&bytes.len())?;
    let hash = hex(&Sha256::digest(bytes));
    candidates.contains(&hash.as_str()).then_some(hash)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
