//! Loss-aware serialized generic animation graphs.
//!
//! This module classifies every pointer through the DAT relocation table and
//! bounds every linked traversal and source-counted array. It preserves
//! descriptor identity and raw fields but does not attach or evaluate them.
use std::collections::HashSet;

use crate::descriptor::DescriptorParseError;
use crate::descriptor::aobj::{
    RawAObjDesc, RawAObjError, RawAObjLimits, RawAObjWork, parse_optional as parse_aobj,
};
use crate::{DatFile, DatPointerError};
use thiserror::Error;

pub const ANIM_JOINT_SIZE: usize = 0x14;

#[derive(Clone, Copy, Debug)]
pub struct RawGenericAnimationLimits {
    pub max_anim_joints: usize,
    pub max_aobjs: usize,
    pub max_fobjs: usize,
    pub max_path_components: usize,
    pub max_packed_bytes: usize,
}

impl Default for RawGenericAnimationLimits {
    fn default() -> Self {
        Self {
            max_anim_joints: 16_384,
            max_aobjs: 16_384,
            max_fobjs: 65_536,
            max_path_components: 1_000_000,
            max_packed_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RawAnimJointGraph<'a> {
    pub root_offset: u32,
    /// Source preorder. `position` is the source child/sibling position, not an ID.
    pub joints: Vec<RawAnimJoint<'a>>,
}

#[derive(Clone, Debug)]
pub struct RawAnimJoint<'a> {
    pub source_offset: u32,
    pub position: Vec<usize>,
    pub child_offset: Option<u32>,
    pub next_offset: Option<u32>,
    pub aobj: Option<RawAObjDesc<'a>>,
    pub raw_flags: u32,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawGenericAnimationError {
    #[error("{resource} descriptor at data offset {source_offset:#x} is out of bounds")]
    DescriptorOutOfBounds {
        resource: &'static str,
        source_offset: u32,
    },
    #[error("{resource} pointer field overflows at data offset {source_offset:#x}")]
    PointerFieldOverflow {
        resource: &'static str,
        source_offset: u32,
    },
    #[error("{resource} pointer at data offset {field_offset:#x} is invalid: {source}")]
    InvalidPointer {
        resource: &'static str,
        field_offset: u32,
        #[source]
        source: DatPointerError,
    },
    #[error("{resource} linked structure cycles at data offset {source_offset:#x}")]
    Cycle {
        resource: &'static str,
        source_offset: u32,
    },
    #[error("generic animation exceeds the {resource} budget of {limit}")]
    ResourceLimit {
        resource: &'static str,
        limit: usize,
    },
    #[error("generic animation {resource} count overflows")]
    CountOverflow { resource: &'static str },
    #[error("FObj packed-data range overflows at descriptor {source_offset:#x}")]
    PackedDataRangeOverflow { source_offset: u32 },
    #[error("FObj packed data is out of bounds at descriptor {source_offset:#x}")]
    PackedDataOutOfBounds { source_offset: u32 },
    #[error(transparent)]
    Descriptor(DescriptorParseError),
}

#[derive(Default)]
struct Work {
    anim_joints: usize,
    aobj: RawAObjWork,
    path_components: usize,
}

pub(super) fn aobj_limits(limits: RawGenericAnimationLimits) -> RawAObjLimits {
    RawAObjLimits {
        max_aobjs: limits.max_aobjs,
        max_fobjs: limits.max_fobjs,
        max_packed_bytes: limits.max_packed_bytes,
    }
}
pub(super) fn aobj_error(error: RawAObjError) -> RawGenericAnimationError {
    match error {
        RawAObjError::Descriptor(error) => material_descriptor_error(error),
        RawAObjError::Cycle {
            resource,
            source_offset,
        } => RawGenericAnimationError::Cycle {
            resource,
            source_offset,
        },
        RawAObjError::ResourceLimit { resource, limit } => {
            RawGenericAnimationError::ResourceLimit { resource, limit }
        }
        RawAObjError::CountOverflow { resource } => {
            RawGenericAnimationError::CountOverflow { resource }
        }
        RawAObjError::PackedDataRangeOverflow { source_offset } => {
            RawGenericAnimationError::PackedDataRangeOverflow { source_offset }
        }
        RawAObjError::PackedDataOutOfBounds { source_offset } => {
            RawGenericAnimationError::PackedDataOutOfBounds { source_offset }
        }
    }
}

impl<'a> RawAnimJointGraph<'a> {
    pub fn parse(dat: &'a DatFile, root_offset: u32) -> Result<Self, RawGenericAnimationError> {
        Self::parse_with_limits(dat, root_offset, RawGenericAnimationLimits::default())
    }

    pub fn parse_with_limits(
        dat: &'a DatFile,
        root_offset: u32,
        limits: RawGenericAnimationLimits,
    ) -> Result<Self, RawGenericAnimationError> {
        let mut work = Work::default();
        charge_path_components(&mut work, 1, limits.max_path_components)?;
        let mut seen = HashSet::new();
        let mut pending = vec![(vec![0], root_offset)];
        let mut joints = Vec::new();

        while let Some((position, source_offset)) = pending.pop() {
            charge(&mut work.anim_joints, limits.max_anim_joints, "AnimJoint")?;
            if !seen.insert(source_offset) {
                return Err(RawGenericAnimationError::Cycle {
                    resource: "AnimJoint",
                    source_offset,
                });
            }
            descriptor(dat, source_offset, ANIM_JOINT_SIZE, "AnimJoint")?;
            let child_offset = pointer(dat, source_offset, 0x00, "AnimJoint child")?;
            let next_offset = pointer(dat, source_offset, 0x04, "AnimJoint next")?;
            let aobj_offset = pointer(dat, source_offset, 0x08, "AnimJoint AObj")?;
            let raw_flags = read_u32(dat, source_offset, 0x10, "AnimJoint")?;
            let aobj = parse_aobj(dat, aobj_offset, aobj_limits(limits), &mut work.aobj)
                .map_err(aobj_error)?;

            if let Some(next) = next_offset {
                charge_path_components(&mut work, position.len(), limits.max_path_components)?;
                let mut next_position = position.clone();
                *next_position.last_mut().expect("nonempty source position") += 1;
                pending.push((next_position, next));
            }
            if let Some(child) = child_offset {
                let child_components = position.len().checked_add(1).ok_or(
                    RawGenericAnimationError::CountOverflow {
                        resource: "source path component",
                    },
                )?;
                charge_path_components(&mut work, child_components, limits.max_path_components)?;
                let mut child_position = position.clone();
                child_position.push(0);
                pending.push((child_position, child));
            }
            joints.push(RawAnimJoint {
                source_offset,
                position,
                child_offset,
                next_offset,
                aobj,
                raw_flags,
            });
        }

        Ok(Self {
            root_offset,
            joints,
        })
    }
}

fn material_descriptor_error(error: DescriptorParseError) -> RawGenericAnimationError {
    match error {
        DescriptorParseError::Truncated { descriptor, offset } => {
            RawGenericAnimationError::DescriptorOutOfBounds {
                resource: descriptor,
                source_offset: offset,
            }
        }
        DescriptorParseError::InvalidPointer {
            descriptor,
            field,
            field_offset,
            source,
        } => {
            let resource = match (descriptor, field) {
                ("AObj", "fobj") => "AObj FObj",
                ("AObj", "obj_id") => "AObj obj_id",
                ("FObj", "next") => "FObj next",
                ("FObj", "packed_data") => "FObj packed data",
                _ => descriptor,
            };
            RawGenericAnimationError::InvalidPointer {
                resource,
                field_offset,
                source,
            }
        }
        error => RawGenericAnimationError::Descriptor(error),
    }
}

pub(super) fn descriptor<'a>(
    dat: &'a DatFile,
    source_offset: u32,
    size: usize,
    resource: &'static str,
) -> Result<&'a [u8], RawGenericAnimationError> {
    dat.data_slice(source_offset, size)
        .ok_or(RawGenericAnimationError::DescriptorOutOfBounds {
            resource,
            source_offset,
        })
}

pub(super) fn read_u32(
    dat: &DatFile,
    source_offset: u32,
    relative_offset: u32,
    resource: &'static str,
) -> Result<u32, RawGenericAnimationError> {
    let field_offset = source_offset.checked_add(relative_offset).ok_or(
        RawGenericAnimationError::PointerFieldOverflow {
            resource,
            source_offset,
        },
    )?;
    dat.read_u32(field_offset)
        .ok_or(RawGenericAnimationError::DescriptorOutOfBounds {
            resource,
            source_offset,
        })
}

pub(super) fn pointer(
    dat: &DatFile,
    source_offset: u32,
    relative_offset: u32,
    resource: &'static str,
) -> Result<Option<u32>, RawGenericAnimationError> {
    let field_offset = source_offset.checked_add(relative_offset).ok_or(
        RawGenericAnimationError::PointerFieldOverflow {
            resource,
            source_offset,
        },
    )?;
    dat.resolve_pointer(field_offset)
        .map_err(|source| RawGenericAnimationError::InvalidPointer {
            resource,
            field_offset,
            source,
        })
}

fn charge_path_components(
    work: &mut Work,
    components: usize,
    limit: usize,
) -> Result<(), RawGenericAnimationError> {
    work.path_components = work.path_components.checked_add(components).ok_or(
        RawGenericAnimationError::CountOverflow {
            resource: "source path component",
        },
    )?;
    if work.path_components > limit {
        return Err(RawGenericAnimationError::ResourceLimit {
            resource: "source path component",
            limit,
        });
    }
    Ok(())
}

fn charge(
    count: &mut usize,
    limit: usize,
    resource: &'static str,
) -> Result<(), RawGenericAnimationError> {
    *count = count
        .checked_add(1)
        .ok_or(RawGenericAnimationError::CountOverflow { resource })?;
    if *count > limit {
        return Err(RawGenericAnimationError::ResourceLimit { resource, limit });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dat(data: Vec<u8>, mut relocation_sites: Vec<u32>) -> DatFile {
        relocation_sites.sort_unstable();
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    fn put_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn relocated_zero_is_an_aobj_descriptor_identity_not_null() {
        let mut data = vec![0; 0x60];
        put_u32(&mut data, 0x20 + 0x08, 0);
        let dat = dat(data, vec![0x28]);
        let graph = RawAnimJointGraph::parse(&dat, 0x20).expect("relocated-zero AObj");
        assert_eq!(
            graph.joints[0].aobj.as_ref().map(|aobj| aobj.source_offset),
            Some(0)
        );
    }

    #[test]
    fn fobj_packed_data_uses_relocation_classification_not_the_raw_word() {
        let mut data = vec![0; 0x60];
        put_u32(&mut data, 0x08, 0x20);
        put_u32(&mut data, 0x28, 0x30);
        put_u32(&mut data, 0x34, 1);
        data[0x50] = 0xa5;

        let unrelocated = dat(data.clone(), vec![0x08, 0x28]);
        let graph =
            RawAnimJointGraph::parse(&unrelocated, 0).expect("unrelocated packed-data word");
        let fobj = &graph.joints[0].aobj.as_ref().expect("AObj").fobjs[0];
        assert_eq!(fobj.packed_data_offset, None);
        assert!(fobj.packed_data.is_empty());

        put_u32(&mut data, 0x40, 0x50);
        let relocated = dat(data, vec![0x08, 0x28, 0x40]);
        let graph = RawAnimJointGraph::parse(&relocated, 0).expect("relocated packed-data pointer");
        let fobj = &graph.joints[0].aobj.as_ref().expect("AObj").fobjs[0];
        assert_eq!(fobj.packed_data_offset, Some(0x50));
        assert_eq!(fobj.packed_data, &[0xa5]);
    }
    #[test]
    fn aobj_fobj_and_packed_byte_budgets_accumulate_across_graph() {
        let mut data = vec![0; 0xc2];
        put_u32(&mut data, 0x04, 0x20);
        put_u32(&mut data, 0x08, 0x40);
        put_u32(&mut data, 0x28, 0x60);
        put_u32(&mut data, 0x48, 0x80);
        put_u32(&mut data, 0x68, 0xa0);
        put_u32(&mut data, 0x84, 1);
        put_u32(&mut data, 0x90, 0xc0);
        put_u32(&mut data, 0xa4, 1);
        put_u32(&mut data, 0xb0, 0xc1);
        data[0xc0..0xc2].copy_from_slice(&[0xa5, 0x5a]);
        let dat = dat(data, vec![0x04, 0x08, 0x28, 0x48, 0x68, 0x90, 0xb0]);

        for (limits, resource) in [
            (
                RawGenericAnimationLimits {
                    max_aobjs: 1,
                    ..RawGenericAnimationLimits::default()
                },
                "AObj",
            ),
            (
                RawGenericAnimationLimits {
                    max_fobjs: 1,
                    ..RawGenericAnimationLimits::default()
                },
                "FObj",
            ),
            (
                RawGenericAnimationLimits {
                    max_packed_bytes: 1,
                    ..RawGenericAnimationLimits::default()
                },
                "packed byte",
            ),
        ] {
            assert!(matches!(
                RawAnimJointGraph::parse_with_limits(&dat, 0, limits),
                Err(RawGenericAnimationError::ResourceLimit {
                    resource: actual,
                    limit: 1,
                }) if actual == resource
            ));
        }
    }
    #[test]
    fn rejects_nonzero_unrelocated_topology_pointer() {
        let mut data = vec![0; 0x40];
        put_u32(&mut data, 0, 0x20);
        assert!(matches!(
            RawAnimJointGraph::parse(&dat(data, Vec::new()), 0),
            Err(RawGenericAnimationError::InvalidPointer {
                resource: "AnimJoint child",
                field_offset: 0,
                source: DatPointerError::MissingRelocation,
            })
        ));
    }

    #[test]
    fn rejects_deep_paths_before_cloning_unbounded_position_components() {
        let mut data = vec![0; 0x40];
        put_u32(&mut data, 0, 0x20);
        let limits = RawGenericAnimationLimits {
            max_path_components: 2,
            ..RawGenericAnimationLimits::default()
        };
        assert!(matches!(
            RawAnimJointGraph::parse_with_limits(&dat(data, vec![0]), 0, limits),
            Err(RawGenericAnimationError::ResourceLimit {
                resource: "source path component",
                limit: 2,
            })
        ));
    }
    #[test]
    fn rejects_animjoint_cycles_and_explicit_node_budget_exhaustion() {
        let mut cycle = vec![0; 0x40];
        put_u32(&mut cycle, 0x04, 0);
        assert!(matches!(
            RawAnimJointGraph::parse(&dat(cycle, vec![0x04]), 0),
            Err(RawGenericAnimationError::Cycle {
                resource: "AnimJoint",
                source_offset: 0
            })
        ));

        let mut chain = vec![0; 0x40];
        put_u32(&mut chain, 0x04, 0x20);
        let limits = RawGenericAnimationLimits {
            max_anim_joints: 1,
            ..RawGenericAnimationLimits::default()
        };
        assert!(matches!(
            RawAnimJointGraph::parse_with_limits(&dat(chain, vec![0x04]), 0, limits),
            Err(RawGenericAnimationError::ResourceLimit {
                resource: "AnimJoint",
                limit: 1
            })
        ));
    }
}
