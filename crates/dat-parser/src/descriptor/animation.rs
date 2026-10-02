//! Loss-aware serialized FigaTree and FObj descriptors.
//!
//! This module owns bounded pointer traversal and preserves serialized values and
//! archive-local provenance. It does not assign receiver semantics or evaluate
//! packed FObj operations.

use crate::{DatFile, DatPointerError};
use thiserror::Error;

pub const FIGA_TREE_SIZE: usize = 0x14;
pub const FIGA_TRACK_SIZE: usize = 0x0c;
pub const MAX_FIGA_COUNT_LIST_ENTRIES: usize = 1_024;
pub const MAX_FIGA_TRACKS: usize = 16_384;

#[derive(Clone, Copy, Debug)]
pub struct RawFigaTreeLimits {
    pub max_count_list_entries: usize,
    pub max_tracks: usize,
}

impl Default for RawFigaTreeLimits {
    fn default() -> Self {
        Self {
            max_count_list_entries: MAX_FIGA_COUNT_LIST_ENTRIES,
            max_tracks: MAX_FIGA_TRACKS,
        }
    }
}

/// One serialized FigaTree rooted at an archive-local data-section offset.
#[derive(Debug)]
pub struct RawFigaTree<'a> {
    pub source_offset: u32,
    pub tree_type: u32,
    pub flags: u32,
    pub end_frame: f32,
    pub nodes_offset: u32,
    pub tracks_offset: u32,
    /// Signed serialized per-part counts, excluding the `-1` terminator.
    pub track_counts: Vec<i8>,
    pub tracks: Vec<RawFObjTrack<'a>>,
}

/// One serialized FObj descriptor and its exact bounded packed stream.
#[derive(Clone, Copy, Debug)]
pub struct RawFObjTrack<'a> {
    pub count_list_ordinal: usize,
    pub descriptor_offset: u32,
    pub packed_data_offset: u32,
    pub length: u16,
    pub start_frame: u16,
    pub object_type: u8,
    pub frac_value: u8,
    pub frac_slope: u8,
    pub reserved: u8,
    pub packed_data: &'a [u8],
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawFigaTreeError {
    #[error("FigaTree descriptor range overflows")]
    DescriptorRangeOverflow,
    #[error("FigaTree descriptor is out of bounds")]
    DescriptorOutOfBounds,
    #[error("FigaTree {resource} pointer field overflows")]
    PointerFieldOverflow { resource: &'static str },
    #[error("FigaTree {resource} pointer at data offset {field_offset:#x} is invalid: {error:?}")]
    InvalidPointer {
        resource: &'static str,
        field_offset: u32,
        error: DatPointerError,
    },
    #[error(
        "FObj packed-data pointer at data offset {field_offset:#x} is invalid for count-list ordinal {count_list_ordinal}: {error:?}"
    )]
    InvalidTrackPointer {
        count_list_ordinal: usize,
        field_offset: u32,
        error: DatPointerError,
    },
    #[error("FigaTree track-count list is unterminated")]
    UnterminatedTrackCounts,
    #[error(
        "FigaTree count-list ordinal {count_list_ordinal} has unsupported negative track count {value}"
    )]
    NegativeTrackCount {
        count_list_ordinal: usize,
        value: i8,
    },
    #[error("FigaTree exceeds the {resource} budget of {limit}")]
    ResourceLimit {
        resource: &'static str,
        limit: usize,
    },
    #[error("FigaTree track count overflows")]
    TrackCountOverflow,
    #[error("FigaTree track descriptor byte count overflows")]
    TrackDescriptorBytesOverflow,
    #[error("FigaTree track descriptors are out of bounds")]
    TrackDescriptorsOutOfBounds,
    #[error("FObj packed-data range overflows for count-list ordinal {count_list_ordinal}")]
    PackedDataRangeOverflow { count_list_ordinal: usize },
    #[error("FObj packed data is out of bounds for count-list ordinal {count_list_ordinal}")]
    PackedDataOutOfBounds { count_list_ordinal: usize },
}

impl<'a> RawFigaTree<'a> {
    pub fn parse(dat: &'a DatFile, source_offset: u32) -> Result<Self, RawFigaTreeError> {
        Self::parse_with_limits(dat, source_offset, RawFigaTreeLimits::default())
    }

    pub fn parse_with_limits(
        dat: &'a DatFile,
        source_offset: u32,
        limits: RawFigaTreeLimits,
    ) -> Result<Self, RawFigaTreeError> {
        let tree_start = source_offset as usize;
        let tree_end = tree_start
            .checked_add(FIGA_TREE_SIZE)
            .ok_or(RawFigaTreeError::DescriptorRangeOverflow)?;
        let tree = dat
            .data
            .get(tree_start..tree_end)
            .ok_or(RawFigaTreeError::DescriptorOutOfBounds)?;

        let tree_type = read_u32_be(tree, 0).expect("bounded FigaTree type");
        let flags = read_u32_be(tree, 4).expect("bounded FigaTree flags");
        let end_frame = f32::from_bits(read_u32_be(tree, 8).expect("bounded FigaTree end frame"));
        let nodes_field = source_offset
            .checked_add(0x0c)
            .ok_or(RawFigaTreeError::PointerFieldOverflow { resource: "nodes" })?;
        let tracks_field = source_offset
            .checked_add(0x10)
            .ok_or(RawFigaTreeError::PointerFieldOverflow { resource: "tracks" })?;
        let nodes_offset = resolve_pointer(dat, "nodes", nodes_field)?;
        let tracks_offset = resolve_pointer(dat, "tracks", tracks_field)?;

        let mut track_counts = Vec::new();
        let mut terminated = false;
        for &raw in dat.data.get(nodes_offset as usize..).unwrap_or_default() {
            let value = raw as i8;
            if value == -1 {
                terminated = true;
                break;
            }
            if value < 0 {
                return Err(RawFigaTreeError::NegativeTrackCount {
                    count_list_ordinal: track_counts.len(),
                    value,
                });
            }
            if track_counts.len() >= limits.max_count_list_entries {
                return Err(RawFigaTreeError::ResourceLimit {
                    resource: "count-list entry",
                    limit: limits.max_count_list_entries,
                });
            }
            track_counts.push(value);
        }
        if !terminated {
            return Err(RawFigaTreeError::UnterminatedTrackCounts);
        }

        let track_count = track_counts.iter().try_fold(0usize, |sum, &count| {
            sum.checked_add(count as usize)
                .ok_or(RawFigaTreeError::TrackCountOverflow)
        })?;
        if track_count > limits.max_tracks {
            return Err(RawFigaTreeError::ResourceLimit {
                resource: "track",
                limit: limits.max_tracks,
            });
        }
        let descriptors_len = track_count
            .checked_mul(FIGA_TRACK_SIZE)
            .ok_or(RawFigaTreeError::TrackDescriptorBytesOverflow)?;
        let descriptors_end = (tracks_offset as usize)
            .checked_add(descriptors_len)
            .ok_or(RawFigaTreeError::TrackDescriptorBytesOverflow)?;
        dat.data
            .get(tracks_offset as usize..descriptors_end)
            .ok_or(RawFigaTreeError::TrackDescriptorsOutOfBounds)?;

        let mut tracks = Vec::with_capacity(track_count);
        let mut descriptor_index = 0usize;
        for (count_list_ordinal, count) in track_counts.iter().copied().enumerate() {
            for _ in 0..count {
                let descriptor_offset = tracks_offset as usize + descriptor_index * FIGA_TRACK_SIZE;
                let descriptor = &dat.data[descriptor_offset..descriptor_offset + FIGA_TRACK_SIZE];
                let length = read_u16_be(descriptor, 0).expect("bounded FObj length");
                let start_frame = read_u16_be(descriptor, 2).expect("bounded FObj start");
                let packed_field = u32::try_from(descriptor_offset)
                    .ok()
                    .and_then(|offset| offset.checked_add(8))
                    .ok_or(RawFigaTreeError::PackedDataRangeOverflow { count_list_ordinal })?;
                let packed_data_offset =
                    dat.resolve_required_pointer(packed_field)
                        .map_err(|error| RawFigaTreeError::InvalidTrackPointer {
                            count_list_ordinal,
                            field_offset: packed_field,
                            error,
                        })?;
                let packed_end = (packed_data_offset as usize)
                    .checked_add(length as usize)
                    .ok_or(RawFigaTreeError::PackedDataRangeOverflow { count_list_ordinal })?;
                let packed_data = dat
                    .data
                    .get(packed_data_offset as usize..packed_end)
                    .ok_or(RawFigaTreeError::PackedDataOutOfBounds { count_list_ordinal })?;
                tracks.push(RawFObjTrack {
                    count_list_ordinal,
                    descriptor_offset: descriptor_offset as u32,
                    packed_data_offset,
                    length,
                    start_frame,
                    object_type: descriptor[4],
                    frac_value: descriptor[5],
                    frac_slope: descriptor[6],
                    reserved: descriptor[7],
                    packed_data,
                });
                descriptor_index += 1;
            }
        }

        Ok(Self {
            source_offset,
            tree_type,
            flags,
            end_frame,
            nodes_offset,
            tracks_offset,
            track_counts,
            tracks,
        })
    }
}

fn resolve_pointer(
    dat: &DatFile,
    resource: &'static str,
    field_offset: u32,
) -> Result<u32, RawFigaTreeError> {
    dat.resolve_required_pointer(field_offset)
        .map_err(|error| RawFigaTreeError::InvalidPointer {
            resource,
            field_offset,
            error,
        })
}

fn read_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
