//! Typed serialized material-animation descriptors.
//!
//! This module owns source descriptor validation and pointer classification.
//! Bounded graph traversal lives in `crate::descriptor::traversal::material_animation`.

use super::{DatFile, DescriptorParseError, DescriptorReader};

pub const MAT_ANIM_JOINT_SIZE: usize = 0x0c;
pub const MAT_ANIM_SIZE: usize = 0x10;
pub const TEX_ANIM_SIZE: usize = 0x18;
pub const AOBJ_SIZE: usize = 0x10;
pub const FOBJ_SIZE: usize = 0x14;

#[derive(Clone, Debug)]
pub struct MatAnimJoint {
    pub offset: u32,
    pub child_ptr: Option<u32>,
    pub next_ptr: Option<u32>,
    pub material_anim_ptr: Option<u32>,
}

impl MatAnimJoint {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "MatAnimJoint", offset)
            .require_extent(MAT_ANIM_JOINT_SIZE)?;

        Ok(Self {
            offset,
            child_ptr: source.pointer("child", 0x00)?,
            next_ptr: source.pointer("next", 0x04)?,
            material_anim_ptr: source.pointer("material_anim", 0x08)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct MatAnim {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub animation_ptr: Option<u32>,
    pub texture_animation_ptr: Option<u32>,
    pub render_animation_ptr: Option<u32>,
}

impl MatAnim {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "MatAnim", offset).require_extent(MAT_ANIM_SIZE)?;

        Ok(Self {
            offset,
            next_ptr: source.pointer("next", 0x00)?,
            animation_ptr: source.pointer("animation", 0x04)?,
            texture_animation_ptr: source.pointer("texture_animation", 0x08)?,
            render_animation_ptr: source.pointer("render_animation", 0x0c)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TexAnim {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub texture_map_id: u32,
    pub animation_ptr: Option<u32>,
    pub image_table_ptr: Option<u32>,
    pub tlut_table_ptr: Option<u32>,
    pub image_count: u16,
    pub tlut_count: u16,
}

impl TexAnim {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "TexAnim", offset).require_extent(TEX_ANIM_SIZE)?;

        Ok(Self {
            offset,
            next_ptr: source.pointer("next", 0x00)?,
            texture_map_id: source.u32(0x04)?,
            animation_ptr: source.pointer("animation", 0x08)?,
            image_table_ptr: source.pointer("image_table", 0x0c)?,
            tlut_table_ptr: source.pointer("tlut_table", 0x10)?,
            image_count: source.u16(0x14)?,
            tlut_count: source.u16(0x16)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct AObj {
    pub offset: u32,
    pub flags: u32,
    pub end_frame_bits: u32,
    pub fobj_ptr: Option<u32>,
    pub obj_id_ptr: Option<u32>,
}

impl AObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "AObj", offset).require_extent(AOBJ_SIZE)?;

        Ok(Self {
            offset,
            flags: source.u32(0x00)?,
            end_frame_bits: source.u32(0x04)?,
            fobj_ptr: source.pointer("fobj", 0x08)?,
            obj_id_ptr: source.pointer("obj_id", 0x0c)?,
        })
    }
}

#[derive(Clone, Debug)]
pub struct FObj {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub data_length: u32,
    pub start_frame_bits: u32,
    pub track_type: u8,
    pub value_flag: u8,
    pub tangent_flag: u8,
    pub reserved: u8,
    pub packed_data_ptr: Option<u32>,
}

impl FObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "FObj", offset).require_extent(FOBJ_SIZE)?;

        Ok(Self {
            offset,
            next_ptr: source.pointer("next", 0x00)?,
            data_length: source.u32(0x04)?,
            start_frame_bits: source.u32(0x08)?,
            track_type: source.u8(0x0c)?,
            value_flag: source.u8(0x0d)?,
            tangent_flag: source.u8(0x0e)?,
            reserved: source.u8(0x0f)?,
            packed_data_ptr: source.pointer("packed_data", 0x10)?,
        })
    }
}
