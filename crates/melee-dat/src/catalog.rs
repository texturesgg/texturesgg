//! The reference catalog: the checked-in, source-verified table of every
//! fighter's files, joint hierarchy and idle setup
//! (`data/reference-catalog.json`), and the costumes it recognizes.

use crate::fighter::playback::MeleeFighterPlayback;
use crate::fighter::{CostumeIndex, FighterKind};
use crate::file_names::{Character, CostumeColor, MeleeSlot};
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::scene::HsdJoint;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The text of the checked-in catalog, for a host that serves the same table
/// from a copy of its own to check the copy against. Its layout is the
/// file's, not an interface: read the catalog through
/// [`MeleeReferenceCatalog`].
pub const CATALOG_JSON: &str = include_str!("../data/reference-catalog.json");

/// The file as it is written, read once into [`MeleeReferenceCatalog`].
mod file {
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Catalog {
        pub assets: Vec<Asset>,
        pub common_key: String,
        pub fighters: Vec<Fighter>,
        pub roster_idle_profiles: Vec<Profile>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Asset {
        pub key: String,
        pub file_name: String,
        pub sha256: String,
        pub byte_length: usize,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Fighter {
        pub fighter_kind: u8,
        pub label: String,
        pub fighter_key: String,
        pub animations_key: String,
        pub animation_count: usize,
        pub primary_idle: PrimaryIdle,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct PrimaryIdle {
        pub animation_index: usize,
        pub record_fighter_kind: Option<u8>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Profile {
        pub fighter_kind: u8,
        pub root_symbols: Vec<String>,
        pub initialization: Initialization,
        pub idle: Idle,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Initialization {
        pub model_scaling: f64,
        pub scale_receiver_index: usize,
        pub joint_parents: Vec<Option<u32>>,
        pub root_scale: [f64; 3],
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Idle {
        pub end_frame: f32,
        pub flags: u32,
    }
}

/// The checked-in table. Every reference it holds between its own entries (a
/// fighter's files, a profile's fighter, Nana's use of Popo's records) is
/// resolved when it is read, so a lookup within it cannot miss.
#[derive(Debug)]
pub struct MeleeReferenceCatalog {
    assets: Vec<ReferenceAsset>,
    /// `PlCo.dat`, as an index into `assets`.
    common: usize,
    fighters: Vec<ReferenceFighter>,
    profiles: Vec<IdleProfile>,
}

/// One original game file the catalog names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceAsset {
    /// A stable key for the file, unique in the catalog: a host that serves
    /// or caches reference files can store them under it.
    pub key: String,
    pub file_name: String,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Debug)]
pub(crate) struct ReferenceFighter {
    pub(crate) kind: FighterKind,
    pub(crate) label: String,
    /// The fighter's data file (`PlXx.dat`), as an index into the assets.
    data: usize,
    /// The fighter's animation archive (`PlXxAJ.dat`).
    animations: usize,
    pub(crate) animation_count: usize,
    /// The index of Wait1 in the fighter's animation table.
    pub(crate) idle_animation: usize,
    /// The fighter whose records and archive this one plays: itself, or Popo
    /// for Nana (ftData_80085FD4). An index into the fighters.
    record: usize,
}

#[derive(Debug)]
pub(crate) struct IdleProfile {
    /// An index into the fighters.
    fighter: usize,
    pub(crate) root_symbols: Vec<String>,
    pub(crate) initialization: IdleInitialization,
    pub(crate) idle: IdleMetadata,
}

#[derive(Debug)]
pub(crate) struct IdleInitialization {
    pub(crate) model_scaling: f64,
    pub(crate) scale_receiver_index: usize,
    /// Each joint's parent, in the costume's joint order; the root has none.
    pub(crate) joint_parents: Vec<Option<u32>>,
    pub(crate) root_scale: [f64; 3],
}

#[derive(Debug)]
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
    /// The catalog this crate ships.
    pub fn checked_in() -> &'static Self {
        static CATALOG: OnceLock<MeleeReferenceCatalog> = OnceLock::new();
        CATALOG.get_or_init(|| {
            let file: file::Catalog =
                serde_json::from_str(CATALOG_JSON).expect("the checked-in catalog parses");
            Self::resolve(file).expect("the checked-in catalog is consistent")
        })
    }

    /// Resolve the file's keys and kinds; `None` when one names nothing.
    fn resolve(file: file::Catalog) -> Option<Self> {
        let asset = |key: &str| file.assets.iter().position(|asset| asset.key == key);
        let fighter = |kind: u8| {
            file.fighters
                .iter()
                .position(|fighter| fighter.fighter_kind == kind)
        };
        let fighters = file
            .fighters
            .iter()
            .map(|entry| {
                Some(ReferenceFighter {
                    kind: FighterKind::new(entry.fighter_kind)?,
                    label: entry.label.clone(),
                    data: asset(&entry.fighter_key)?,
                    animations: asset(&entry.animations_key)?,
                    animation_count: entry.animation_count,
                    idle_animation: entry.primary_idle.animation_index,
                    record: fighter(
                        entry
                            .primary_idle
                            .record_fighter_kind
                            .unwrap_or(entry.fighter_kind),
                    )?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let profiles = file
            .roster_idle_profiles
            .into_iter()
            .map(|profile| {
                Some(IdleProfile {
                    fighter: fighter(profile.fighter_kind)?,
                    root_symbols: profile.root_symbols,
                    initialization: IdleInitialization {
                        model_scaling: profile.initialization.model_scaling,
                        scale_receiver_index: profile.initialization.scale_receiver_index,
                        joint_parents: profile.initialization.joint_parents,
                        root_scale: profile.initialization.root_scale,
                    },
                    idle: IdleMetadata {
                        end_frame: profile.idle.end_frame,
                        flags: profile.idle.flags,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            common: asset(&file.common_key)?,
            assets: file
                .assets
                .into_iter()
                .map(|asset| ReferenceAsset {
                    key: asset.key,
                    file_name: asset.file_name,
                    sha256: asset.sha256,
                    byte_length: asset.byte_length,
                })
                .collect(),
            fighters,
            profiles,
        })
    }

    /// The fighter a profile describes.
    pub(crate) fn fighter(&self, profile: &IdleProfile) -> &ReferenceFighter {
        &self.fighters[profile.fighter]
    }

    /// The fighter whose records and animation archive `fighter` plays.
    pub(crate) fn record_fighter(&self, fighter: &ReferenceFighter) -> &ReferenceFighter {
        &self.fighters[fighter.record]
    }

    /// The fighter's data file (`PlXx.dat`).
    pub(crate) fn data_asset(&self, fighter: &ReferenceFighter) -> &ReferenceAsset {
        &self.assets[fighter.data]
    }

    /// The fighter's animation archive (`PlXxAJ.dat`).
    pub(crate) fn animations_asset(&self, fighter: &ReferenceFighter) -> &ReferenceAsset {
        &self.assets[fighter.animations]
    }

    /// `PlCo.dat`, the data every fighter shares.
    pub(crate) fn common_asset(&self) -> &ReferenceAsset {
        &self.assets[self.common]
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
    /// `None` when no profile recognizes its root. Lets a host fetch only
    /// those before [`crate::MeleeReferenceStore::from_bytes`].
    pub fn idle_reference_files(&self, contract: &HsdScene) -> Option<Vec<&str>> {
        self.idle_reference_assets(contract).map(|assets| {
            assets
                .into_iter()
                .map(|asset| asset.file_name.as_str())
                .collect()
        })
    }

    /// The reference assets a costume's idle needs, or `None` when no profile
    /// recognizes its root.
    pub fn idle_reference_assets(&self, contract: &HsdScene) -> Option<Vec<&ReferenceAsset>> {
        let (profile, _, _) = MeleeFighterPlayback::profile_for(contract, self)?;
        let fighter = self.fighter(profile);
        let record = self.record_fighter(fighter);
        let mut assets = vec![
            self.data_asset(fighter),
            self.common_asset(),
            self.animations_asset(record),
        ];
        if record.kind != fighter.kind {
            assets.push(self.data_asset(record));
        }
        Some(assets)
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
    ) -> Option<(&'c IdleProfile, usize, CostumeIndex)> {
        let mut vanilla = Vec::new();
        let mut expansion = Vec::new();
        for profile in &self.profiles {
            for (index, name) in roots.clone().enumerate() {
                let Some(name) = name else {
                    continue;
                };
                if let Some(costume) = profile.root_symbols.iter().position(|root| root == name) {
                    vanilla.push((profile, index, CostumeIndex(costume)));
                } else if is_expansion_root(profile, name) {
                    expansion.push((profile, index, CostumeIndex(0)));
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

    /// The costume slot a DAT was made for, from a root the catalog names:
    /// `PlyFalco5KRe_Share_joint` is Falco's Red slot, and
    /// `PlyFalco5K_Share_joint` his Neutral one. Root names alone decide it,
    /// so a costume with a custom model still has its slot.
    pub fn costume_slot<'n>(&self, roots: impl IntoIterator<Item = &'n str>) -> Option<MeleeSlot> {
        roots.into_iter().find_map(|name| {
            let stem = name.strip_suffix("_Share_joint")?;
            self.profiles.iter().find_map(|profile| {
                let base = profile.root_symbols.first()?.strip_suffix("_Share_joint")?;
                let color = match stem.strip_prefix(base)? {
                    "" => CostumeColor::NEUTRAL,
                    // Root names spell the code as file names do.
                    code => CostumeColor::from_code(code).filter(|color| color.code() == code)?,
                };
                let file = &self.data_asset(self.fighter(profile)).file_name;
                let character =
                    Character::from_code(file.strip_prefix("Pl")?.strip_suffix(".dat")?)?;
                Some(MeleeSlot::Costume { character, color })
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
            fighter_kind: self.fighter(profile).kind,
            costume,
            root_index,
        })
    }

    /// The fighter of a costume [`Self::recognize`] admitted.
    pub(crate) fn fighter_of_kind(&self, kind: FighterKind) -> &ReferenceFighter {
        self.fighters
            .iter()
            .find(|fighter| fighter.kind == kind)
            .expect("a recognized costume's fighter is in the catalog")
    }
}

/// A costume [`MeleeReferenceCatalog::recognize`] admitted.
pub(crate) struct RecognizedCostume {
    pub fighter_kind: FighterKind,
    pub costume: CostumeIndex,
    pub root_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `checked_in` resolves every key and kind the file holds, so reading it
    /// at all is the check that none dangles.
    #[test]
    fn checked_in_catalog_resolves_every_profile() {
        let catalog = MeleeReferenceCatalog::checked_in();
        assert_eq!(catalog.profiles.len(), 27);
        assert_eq!(catalog.common_asset().file_name, "PlCo.dat");
        let nana = catalog
            .fighters
            .iter()
            .find(|fighter| fighter.kind == FighterKind::NANA)
            .expect("Nana");
        assert_eq!(catalog.record_fighter(nana).kind, FighterKind::POPO);
    }

    #[test]
    fn root_names_give_the_costume_slot() {
        let catalog = MeleeReferenceCatalog::checked_in();
        let slot = |roots: &[&str]| {
            catalog
                .costume_slot(roots.iter().copied())
                .map(|slot| slot.file_name())
        };
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
        let catalog = MeleeReferenceCatalog::checked_in();
        let purin = catalog
            .profiles
            .iter()
            .find(|profile| catalog.fighter(profile).kind.index() == 0x0F)
            .expect("Jigglypuff profile");
        assert!(is_expansion_root(purin, "PlyPurin5KWh_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurin5KRe_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurin5KWhite_Share_joint"));
        assert!(!is_expansion_root(purin, "PlyPurinReHat_TopN_joint"));
    }
}
