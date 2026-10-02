//! Bounded retention of serialized AObj/FObj graphs and packed FObj bytes.

use std::collections::HashSet;

use super::{
    DatFile, DescriptorParseError,
    material_animation::{AObj, FObj},
};

#[derive(Clone, Debug)]
pub struct RawAObjDesc<'a> {
    pub source_offset: u32,
    pub raw_flags: u32,
    pub end_frame_bits: u32,
    pub obj_id: Option<u32>,
    pub fobjs: Vec<RawGenericFObjDesc<'a>>,
}

#[derive(Clone, Copy, Debug)]
pub struct RawGenericFObjDesc<'a> {
    pub source_offset: u32,
    pub next_offset: Option<u32>,
    pub packed_data_offset: Option<u32>,
    pub length: u32,
    pub start_frame_bits: u32,
    pub object_type: u8,
    pub frac_value: u8,
    pub frac_slope: u8,
    pub reserved: u8,
    pub packed_data: &'a [u8],
}

#[derive(Clone, Copy)]
pub(super) struct RawAObjLimits {
    pub max_aobjs: usize,
    pub max_fobjs: usize,
    pub max_packed_bytes: usize,
}

#[derive(Default)]
pub(super) struct RawAObjWork {
    aobjs: usize,
    fobjs: usize,
    packed_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RawAObjError {
    Descriptor(DescriptorParseError),
    Cycle {
        resource: &'static str,
        source_offset: u32,
    },
    ResourceLimit {
        resource: &'static str,
        limit: usize,
    },
    CountOverflow {
        resource: &'static str,
    },
    PackedDataRangeOverflow {
        source_offset: u32,
    },
    PackedDataOutOfBounds {
        source_offset: u32,
    },
}

pub(super) fn parse_optional<'a>(
    dat: &'a DatFile,
    source_offset: Option<u32>,
    limits: RawAObjLimits,
    work: &mut RawAObjWork,
) -> Result<Option<RawAObjDesc<'a>>, RawAObjError> {
    let Some(source_offset) = source_offset else {
        return Ok(None);
    };
    charge(&mut work.aobjs, limits.max_aobjs, "AObj")?;
    let descriptor = AObj::parse(dat, source_offset).map_err(RawAObjError::Descriptor)?;
    let fobjs = parse_fobjs(dat, descriptor.fobj_ptr, limits, work)?;
    Ok(Some(RawAObjDesc {
        source_offset,
        raw_flags: descriptor.flags,
        end_frame_bits: descriptor.end_frame_bits,
        obj_id: descriptor.obj_id_ptr,
        fobjs,
    }))
}

fn parse_fobjs<'a>(
    dat: &'a DatFile,
    root_offset: Option<u32>,
    limits: RawAObjLimits,
    work: &mut RawAObjWork,
) -> Result<Vec<RawGenericFObjDesc<'a>>, RawAObjError> {
    let mut source_offset = root_offset;
    let mut seen = HashSet::new();
    let mut fobjs = Vec::new();
    while let Some(offset) = source_offset {
        charge(&mut work.fobjs, limits.max_fobjs, "FObj")?;
        if !seen.insert(offset) {
            return Err(RawAObjError::Cycle {
                resource: "FObj",
                source_offset: offset,
            });
        }
        let descriptor = FObj::parse(dat, offset).map_err(RawAObjError::Descriptor)?;
        let packed_length = usize::try_from(descriptor.data_length).map_err(|_| {
            RawAObjError::PackedDataRangeOverflow {
                source_offset: offset,
            }
        })?;
        let packed_data = match descriptor.packed_data_ptr {
            Some(packed_data_offset) => {
                let packed_end = (packed_data_offset as usize)
                    .checked_add(packed_length)
                    .ok_or(RawAObjError::PackedDataRangeOverflow {
                        source_offset: offset,
                    })?;
                dat.data
                    .get(packed_data_offset as usize..packed_end)
                    .ok_or(RawAObjError::PackedDataOutOfBounds {
                        source_offset: offset,
                    })?
            }
            None => &[],
        };
        work.packed_bytes =
            work.packed_bytes
                .checked_add(packed_length)
                .ok_or(RawAObjError::CountOverflow {
                    resource: "packed byte",
                })?;
        if work.packed_bytes > limits.max_packed_bytes {
            return Err(RawAObjError::ResourceLimit {
                resource: "packed byte",
                limit: limits.max_packed_bytes,
            });
        }
        fobjs.push(RawGenericFObjDesc {
            source_offset: offset,
            next_offset: descriptor.next_ptr,
            packed_data_offset: descriptor.packed_data_ptr,
            length: descriptor.data_length,
            start_frame_bits: descriptor.start_frame_bits,
            object_type: descriptor.track_type,
            frac_value: descriptor.value_flag,
            frac_slope: descriptor.tangent_flag,
            reserved: descriptor.reserved,
            packed_data,
        });
        source_offset = descriptor.next_ptr;
    }
    Ok(fobjs)
}

fn charge(count: &mut usize, limit: usize, resource: &'static str) -> Result<(), RawAObjError> {
    *count = count
        .checked_add(1)
        .ok_or(RawAObjError::CountOverflow { resource })?;
    if *count > limit {
        return Err(RawAObjError::ResourceLimit { resource, limit });
    }
    Ok(())
}
