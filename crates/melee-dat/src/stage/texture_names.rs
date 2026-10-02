//! The names HAL gave a stage's textures: root symbols
//! `Grd<Name>_<FORMAT>_image`, keyed by the image data they point at.

use dat_parser::DatFile;
use std::collections::HashMap;

/// HAL's own names for a stage's images.
///
/// A stock stage DAT exports each image's pixel data as a root symbol,
/// `Grd<Name>_<FORMAT>_image` (`GrdBattleWall0_C8_image`). The name here is
/// the part that tells images apart: `BattleWall0`. All 29 unmodified versus
/// stages on a 1.02 disc name every image they draw. A stage rebuilt by a
/// modding tool may export none, or keep symbols for data it no longer
/// draws; its images then have no name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageTextureNames {
    by_data_offset: HashMap<u32, String>,
}

impl StageTextureNames {
    pub fn read(dat: &DatFile) -> Self {
        Self {
            by_data_offset: dat
                .roots
                .iter()
                .filter_map(|root| Some((root.data_offset, image_name(&root.name)?)))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.by_data_offset.is_empty()
    }

    /// The name of the image whose pixel data is at `data_offset`.
    pub fn name_of(&self, data_offset: u32) -> Option<&str> {
        self.by_data_offset.get(&data_offset).map(String::as_str)
    }
}

/// `GrdBattleWall0_C8_image` is `BattleWall0`; a symbol that is not an image
/// is `None`. HAL spells the prefix `Ged` in Kongo Jungle N64.
fn image_name(symbol: &str) -> Option<String> {
    const FORMATS: [&str; 11] = [
        "I4", "I8", "IA4", "IA8", "RGB565", "RGB5A3", "RGBA8", "C4", "C8", "C14X2", "CMPR",
    ];
    let name = symbol.strip_suffix("_image")?;
    let name = name
        .rsplit_once('_')
        .filter(|(_, format)| FORMATS.contains(format))
        .map_or(name, |(name, _)| name);
    let name = ["Grd", "Ged"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .filter(|rest| rest.starts_with(char::is_uppercase))
        .unwrap_or(name);
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::test_support::dat_with_roots_and_relocations;
    use dat_parser::raw::root::RootNode;

    #[test]
    fn image_symbols_name_their_pixel_data() {
        let dat = dat_with_roots_and_relocations(
            vec![0u8; 0x40],
            [
                (0x10, "GrdBattleWall0_C8_image"),
                (0x20, "GedOldkongoBuilding10_CMPR_image"),
                // A custom name keeps what it has.
                (0x24, "Skybox_image"),
                // Palettes and other roots are not images.
                (0x28, "GrdBattleWall0_tlut"),
                (0x30, "map_head"),
            ]
            .into_iter()
            .map(|(data_offset, name)| RootNode {
                data_offset,
                name: name.into(),
            })
            .collect(),
            Vec::new(),
        );
        let names = StageTextureNames::read(&dat);
        assert_eq!(names.name_of(0x10), Some("BattleWall0"));
        assert_eq!(names.name_of(0x20), Some("OldkongoBuilding10"));
        assert_eq!(names.name_of(0x24), Some("Skybox"));
        assert_eq!(names.name_of(0x28), None);
        assert_eq!(names.name_of(0x30), None);
        assert!(!names.is_empty());
    }
}
