//! A stage archive's `map_head`: its model groups and general points.
//!
//! Layout in `docs/hal_dat/ssbm_hal_dat_tables.md`. This reads the fields the
//! scene, stage playback, and stage points use, in one place.

use super::{DatFile, DescriptorParseError, DescriptorReader};
use hal_dat_raw::DatExternError;
use thiserror::Error;

/// Bytes in one model-group record.
const MODEL_GROUP_SIZE: u32 = 0x34;
/// Bytes in one general-point record.
const GENERAL_POINTS_SIZE: u32 = 0x0C;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapHeadError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error(transparent)]
    ExternalFixup(#[from] DatExternError),
    #[error("nonempty stage descriptor table has a null pointer")]
    NullModelGroupTable,
    #[error("stage descriptor count exceeds its data table")]
    ModelGroupCountExceedsData,
    #[error("stage general-point table has a count without a pointer")]
    NullGeneralPointTable,
    #[error("stage general-point count exceeds its data table")]
    GeneralPointCountExceedsData,
}

#[derive(Clone, Copy)]
pub struct MapHead<'a> {
    dat: &'a DatFile,
    source: DescriptorReader<'a>,
}

impl<'a> MapHead<'a> {
    /// The DAT's `map_head`; a DAT with one is a stage.
    pub fn find(dat: &'a DatFile) -> Option<Self> {
        let root = dat.roots.iter().find(|root| root.name == "map_head")?;
        Some(Self {
            dat,
            source: DescriptorReader::new(dat, "MapHead", root.data_offset),
        })
    }

    /// How many model groups the table declares, checked against the data
    /// it must fit in.
    pub fn model_group_count(&self) -> Result<usize, MapHeadError> {
        Ok(self.model_group_table()?.map_or(0, |(_, count)| count))
    }

    /// Every model group the stage owns, in table order.
    ///
    /// Melee's `lbArchive_InitializeDAT` clears declared external fixup
    /// fields, so a record on such a site belongs to another archive and is
    /// left out. Chain words are not ordinary pointers; only validated
    /// members are null here.
    pub fn model_groups(&self) -> Result<Vec<MapModelGroup<'a>>, MapHeadError> {
        let Some((table, count)) = self.model_group_table()? else {
            return Ok(Vec::new());
        };
        let external_sites = self.dat.external_fixup_sites()?;
        Ok((0..count)
            .map(|index| (index, table + index as u32 * MODEL_GROUP_SIZE))
            .filter(|(_, offset)| !external_sites.contains(offset))
            .map(|(index, offset)| MapModelGroup {
                index,
                dat: self.dat,
                source: DescriptorReader::new(self.dat, "MapModelGroup", offset),
            })
            .collect())
    }

    /// The table's offset and record count, `None` for an empty table.
    fn model_group_table(&self) -> Result<Option<(u32, usize)>, MapHeadError> {
        let count = self.source.u32(0x0C)? as usize;
        let table = self.source.pointer("model_groups", 0x08)?;
        if count == 0 {
            return Ok(None);
        }
        let table = table.ok_or(MapHeadError::NullModelGroupTable)?;
        // The extent check also keeps every record offset within `u32`.
        let available =
            self.dat.data.len().saturating_sub(table as usize) / MODEL_GROUP_SIZE as usize;
        if count > available {
            return Err(MapHeadError::ModelGroupCountExceedsData);
        }
        Ok(Some((table, count)))
    }

    /// How many general-point records the table declares. A caller with a
    /// budget checks this before reading them.
    pub fn general_point_count(&self) -> Result<usize, MapHeadError> {
        Ok(self.source.u32(0x04)? as usize)
    }

    /// Every general-point record: a model group's root joint and its
    /// `(joint index, kind)` pairs.
    pub fn general_points(&self) -> Result<Vec<MapGeneralPoints>, MapHeadError> {
        let count = self.general_point_count()?;
        if count == 0 {
            return Ok(Vec::new());
        }
        let table = self
            .source
            .pointer("general_points", 0x00)?
            .ok_or(MapHeadError::NullGeneralPointTable)?;
        // The extent check also keeps every record offset within `u32`.
        let available =
            self.dat.data.len().saturating_sub(table as usize) / GENERAL_POINTS_SIZE as usize;
        if count > available {
            return Err(MapHeadError::GeneralPointCountExceedsData);
        }
        (0..count as u32)
            .map(|index| {
                let record = DescriptorReader::new(
                    self.dat,
                    "MapGeneralPoints",
                    table + index * GENERAL_POINTS_SIZE,
                );
                Ok(MapGeneralPoints {
                    joint: record.pointer("joint", 0x00)?,
                    pairs: record.pointer("pairs", 0x04)?,
                    pair_count: record.u32(0x08)? as usize,
                })
            })
            .collect()
    }
}

/// One general-point record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapGeneralPoints {
    pub joint: Option<u32>,
    pub pairs: Option<u32>,
    pub pair_count: usize,
}

/// One `0x34`-byte model-group record.
#[derive(Clone, Copy)]
pub struct MapModelGroup<'a> {
    /// Its position in the table, counting records left out.
    pub index: usize,
    dat: &'a DatFile,
    source: DescriptorReader<'a>,
}

impl MapModelGroup<'_> {
    /// The group's root joint.
    pub fn root(&self) -> Result<Option<u32>, DescriptorParseError> {
        self.source.pointer("root", 0x00)
    }

    /// The AnimJoint tree of joint animation `animation`.
    pub fn anim_joint(&self, animation: u32) -> Result<Option<u32>, DescriptorParseError> {
        let Some(bank) = self.source.pointer("anim_joints", 0x04)? else {
            return Ok(None);
        };
        // An index too large to address reads past the archive, as any
        // other index past the bank does.
        DescriptorReader::new(self.dat, "MapAnimJointBank", bank)
            .pointer("anim_joint", animation.saturating_mul(4))
    }

    /// Whether the game loops animation `animation`: the group's flag byte
    /// for it is set (`grAnime_801C8138`).
    pub fn loops(&self, animation: u32) -> Result<bool, DescriptorParseError> {
        let Some(flags) = self.source.pointer("loop_flags", 0x28)? else {
            return Ok(false);
        };
        Ok(DescriptorReader::new(self.dat, "MapLoopFlags", flags).u8(animation)? != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::{MapGeneralPoints, MapHead, MapHeadError};
    use crate::DatFile;
    use crate::raw::root::RootNode;

    /// A `map_head` at 0 whose general-point table at 0x10 declares `count`
    /// records and holds one.
    fn stage(count: u32) -> DatFile {
        let mut data = vec![0u8; 0x1C];
        data[0x00..0x04].copy_from_slice(&0x10u32.to_be_bytes());
        data[0x04..0x08].copy_from_slice(&count.to_be_bytes());
        data[0x18..0x1C].copy_from_slice(&7u32.to_be_bytes());
        let root = RootNode {
            data_offset: 0,
            name: "map_head".into(),
        };
        DatFile::from_parts(data, vec![root], vec![0])
    }

    #[test]
    fn general_points_are_read_by_the_count_the_table_can_hold() {
        let dat = stage(1);
        assert_eq!(
            MapHead::find(&dat).unwrap().general_points(),
            Ok(vec![MapGeneralPoints {
                joint: None,
                pairs: None,
                pair_count: 7,
            }])
        );
        let dat = stage(2);
        assert_eq!(
            MapHead::find(&dat).unwrap().general_points(),
            Err(MapHeadError::GeneralPointCountExceedsData)
        );
    }
}
