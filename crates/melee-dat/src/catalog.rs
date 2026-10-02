//! The reference catalog: the checked-in, source-verified table of every
//! fighter's files, joint hierarchy and idle setup
//! (`data/reference-catalog.json`, which the site reads too), and the costumes
//! it recognizes.

use crate::error::{MeleeError, Result, playback_error};
use crate::fighter::playback::MeleeFighterPlayback;
use crate::references::MeleeReferenceStore;
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::scene::HsdJoint;
use serde::Deserialize;
use std::collections::HashMap;

pub const CATALOG_JSON: &str = include_str!("../data/reference-catalog.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeleeReferenceCatalog {
    pub(crate) assets: Vec<ReferenceAsset>,
    pub(crate) common_key: String,
    pub(crate) fighters: Vec<ReferenceFighter>,
    pub(crate) roster_idle_profiles: Vec<IdleProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceAsset {
    /// Storage key under which the site serves this object.
    pub key: String,
    pub file_name: String,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceFighter {
    pub(crate) fighter_kind: u8,
    pub(crate) label: String,
    pub(crate) fighter_key: String,
    pub(crate) animations_key: String,
    pub(crate) animation_count: usize,
    pub(crate) primary_idle: PrimaryIdle,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrimaryIdle {
    pub(crate) animation_index: usize,
    pub(crate) record_fighter_kind: Option<u8>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdleProfile {
    pub(crate) fighter_kind: u8,
    pub(crate) root_symbols: Vec<String>,
    pub(crate) initialization: IdleInitialization,
    pub(crate) idle: IdleMetadata,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdleInitialization {
    pub(crate) model_scaling: f64,
    pub(crate) scale_receiver_index: usize,
    /// Each joint's parent, in the costume's joint order; the root has none.
    pub(crate) joint_parents: Vec<Option<u32>>,
    pub(crate) root_scale: [f64; 3],
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IdleMetadata {
    pub(crate) end_frame: f32,
    pub(crate) flags: u32,
}

impl IdleInitialization {
    /// Whether `joints` form the fighter's hierarchy: the same parents, and
    /// under each the same children in the same order (the table's joints
    /// are numbered as the tree is walked, so a parent's children are the
    /// joints naming it, in order).
    pub(crate) fn has_hierarchy(&self, joints: &[HsdJoint]) -> bool {
        let parents = &self.joint_parents;
        joints.len() == parents.len()
            && joints.iter().enumerate().all(|(index, joint)| {
                let children = parents
                    .iter()
                    .enumerate()
                    .filter(|(_, parent)| **parent == Some(index as u32))
                    .map(|(child, _)| child);
                joint.parent.map(|parent| parent.0 as u32) == parents[index]
                    && joint.children.iter().map(|child| child.0).eq(children)
            })
    }
}

impl MeleeReferenceCatalog {
    pub fn checked_in() -> Result<Self> {
        serde_json::from_str(CATALOG_JSON).map_err(|error| MeleeError::Catalog(error.to_string()))
    }

    pub(crate) fn asset(&self, key: &str) -> Result<&ReferenceAsset> {
        self.assets
            .iter()
            .find(|asset| asset.key == key)
            .ok_or_else(|| playback_error(format!("reference asset {key} is not in the catalog")))
    }

    pub(crate) fn asset_sizes(&self) -> HashMap<usize, Vec<&str>> {
        let mut sizes: HashMap<usize, Vec<&str>> = HashMap::new();
        for asset in &self.assets {
            sizes
                .entry(asset.byte_length)
                .or_default()
                .push(&asset.sha256);
        }
        sizes
    }

    /// Catalog file names of the references a costume's idle needs, or
    /// `None` when no profile recognizes its root. Lets a caller without a
    /// filesystem (a browser) fetch only those before [`MeleeReferenceStore::from_bytes`].
    pub fn idle_reference_files(&self, contract: &HsdScene) -> Option<Vec<&str>> {
        self.idle_reference_assets(contract).map(|assets| {
            assets
                .into_iter()
                .map(|asset| asset.file_name.as_str())
                .collect()
        })
    }

    /// The reference assets a costume's idle needs, or `None` when no profile
    /// recognizes its root. Browsers fetch them by `key`.
    pub fn idle_reference_assets(&self, contract: &HsdScene) -> Option<Vec<&ReferenceAsset>> {
        let (profile, _, _) = MeleeFighterPlayback::profile_for(contract, self)?;
        let entry = self.fighter(profile.fighter_kind).ok()?;
        let record_kind = entry
            .primary_idle
            .record_fighter_kind
            .unwrap_or(entry.fighter_kind);
        let record = self.fighter(record_kind).ok()?;
        let mut keys = vec![&entry.fighter_key, &self.common_key, &record.animations_key];
        if record_kind != entry.fighter_kind {
            keys.push(&record.fighter_key);
        }
        keys.into_iter().map(|key| self.asset(key).ok()).collect()
    }

    pub(crate) fn fighter(&self, kind: u8) -> Result<&ReferenceFighter> {
        self.fighters
            .iter()
            .find(|fighter| fighter.fighter_kind == kind)
            .ok_or_else(|| playback_error(format!("fighter kind {kind} is not in the catalog")))
    }
}

/// A root named like the fighter's costumes (`Ply<Name>5K<Code>_Share_joint`,
/// after the neutral root `Ply<Name>5K_Share_joint`) with a two-letter code
/// that is not one of its vanilla costumes.
pub(crate) fn is_expansion_root(profile: &IdleProfile, name: &str) -> bool {
    const SUFFIX: &str = "_Share_joint";
    let Some(base) = profile
        .root_symbols
        .first()
        .and_then(|neutral| neutral.strip_suffix(SUFFIX))
    else {
        return false;
    };
    name.strip_prefix(base)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some_and(|code| {
            code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        && !profile.root_symbols.iter().any(|root| root == name)
}

impl MeleeReferenceCatalog {
    /// The idle profile whose costume root is among `roots` (names in root
    /// order), that root's index, and the costume ID; see
    /// `MeleeFighterPlayback::profile_for`.
    pub(crate) fn profile_for_roots<'c, 'n>(
        &'c self,
        roots: impl Iterator<Item = Option<&'n str>> + Clone,
    ) -> Option<(&'c IdleProfile, usize, usize)> {
        let mut vanilla = Vec::new();
        let mut expansion = Vec::new();
        for profile in &self.roster_idle_profiles {
            for (index, name) in roots.clone().enumerate() {
                let Some(name) = name else {
                    continue;
                };
                if let Some(costume) = profile.root_symbols.iter().position(|root| root == name) {
                    vanilla.push((profile, index, costume));
                } else if is_expansion_root(profile, name) {
                    expansion.push((profile, index, 0));
                }
            }
        }
        let unique = |matches: Vec<_>| match matches.as_slice() {
            [only] => Some(*only),
            _ => None,
        };
        if vanilla.is_empty() {
            unique(expansion)
        } else {
            unique(vanilla)
        }
    }

    /// The costume slot a DAT was made for (`PlFcRe.dat`), from a root the
    /// catalog names: `PlyFalco5KRe_Share_joint` is Falco's Red slot, and
    /// `PlyFalco5K_Share_joint` his Neutral one. Root names alone decide it,
    /// so a costume with a custom model still has its slot.
    pub fn costume_slot<'n>(&self, roots: impl IntoIterator<Item = &'n str>) -> Option<String> {
        roots.into_iter().find_map(|name| {
            let stem = name.strip_suffix("_Share_joint")?;
            self.roster_idle_profiles.iter().find_map(|profile| {
                let base = profile.root_symbols.first()?.strip_suffix("_Share_joint")?;
                let code = match stem.strip_prefix(base)? {
                    "" => "Nr",
                    code => code,
                };
                crate::COSTUMES
                    .iter()
                    .any(|(known, _)| *known == code)
                    .then_some(())?;
                let fighter = self.fighter(profile.fighter_kind).ok()?;
                let file = &self.asset(&fighter.fighter_key).ok()?.file_name;
                let character = file.strip_prefix("Pl")?.strip_suffix(".dat")?;
                Some(format!("Pl{character}{code}.dat"))
            })
        })
    }

    /// A stock fighter costume in `scene`: a root the catalog names whose
    /// joint hierarchy matches the fighter's, so fighter tables that address
    /// joints and display objects by position apply to it.
    pub(crate) fn recognize(&self, scene: &HsdScene) -> Option<RecognizedCostume> {
        let (profile, root_index, costume) =
            self.profile_for_roots(scene.roots.iter().map(|root| root.name.as_deref()))?;
        let root = &scene.roots[root_index];
        let matches = profile.initialization.has_hierarchy(&root.joints);
        matches.then_some(RecognizedCostume {
            fighter_kind: profile.fighter_kind,
            costume,
            root_index,
        })
    }

    /// `PlCo.dat`, the data every fighter shares.
    pub(crate) fn load_common(&self, store: &MeleeReferenceStore) -> Result<Vec<u8>> {
        store.load(self.asset(&self.common_key)?)
    }

    /// The fighter's own data file (`PlXx.dat`).
    pub(crate) fn load_fighter(
        &self,
        store: &MeleeReferenceStore,
        fighter_kind: u8,
    ) -> Result<Vec<u8>> {
        store.load(self.asset(&self.fighter(fighter_kind)?.fighter_key)?)
    }
}

/// A costume [`MeleeReferenceCatalog::recognize`] admitted.
pub(crate) struct RecognizedCostume {
    pub fighter_kind: u8,
    pub costume: usize,
    pub root_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_catalog_parses_with_every_profile_resolvable() {
        let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
        assert!(!catalog.roster_idle_profiles.is_empty());
        catalog.asset(&catalog.common_key).expect("common asset");
        for profile in &catalog.roster_idle_profiles {
            let fighter = catalog.fighter(profile.fighter_kind).expect("fighter");
            catalog.asset(&fighter.fighter_key).expect("fighter asset");
            let record_kind = fighter
                .primary_idle
                .record_fighter_kind
                .unwrap_or(fighter.fighter_kind);
            let record = catalog.fighter(record_kind).expect("record fighter");
            catalog
                .asset(&record.animations_key)
                .expect("animation asset");
        }
    }

    #[test]
    fn root_names_give_the_costume_slot() {
        let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
        let slot = |roots: &[&str]| catalog.costume_slot(roots.iter().copied());
        assert_eq!(
            slot(&["PlyFalco5KRe_Share_joint"]).as_deref(),
            Some("PlFcRe.dat")
        );
        assert_eq!(
            slot(&["x", "PlyMario5K_Share_joint"]).as_deref(),
            Some("PlMrNr.dat")
        );
        assert_eq!(slot(&["PlyFalco5KZz_Share_joint"]), None);
        assert_eq!(slot(&["GrdPStadium_TopN_joint"]), None);
    }

    #[test]
    fn expansion_roots_follow_the_fighter_naming_only() {
        let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
        let purin = catalog
            .roster_idle_profiles
            .iter()
            .find(|profile| profile.fighter_kind == 0x0F)
            .expect("Jigglypuff profile");
        assert!(is_expansion_root(purin, "PlyPurin5KWh_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurin5KRe_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurin5KWhite_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurinReHat_TopN_joint"));
    }
}
