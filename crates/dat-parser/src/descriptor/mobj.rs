use super::{DatFile, DescriptorParseError, DescriptorReader};

/// Source-defined `HSD_MObjDesc.rendermode` values.
///
/// The low-bit diffuse aliases overlap the channel flags exactly; callers must
/// preserve the raw word rather than treating every name as an independent bit.
pub mod render_flags {
    pub const DIFFUSE_SHIFT: u32 = 0;
    pub const DIFFUSE_BITS: u32 = 0x0000_0003;
    pub const DIFFUSE_MAT0: u32 = 0x0000_0000;
    pub const DIFFUSE_MAT: u32 = 0x0000_0001;
    pub const DIFFUSE_VTX: u32 = 0x0000_0002;
    pub const DIFFUSE_BOTH: u32 = 0x0000_0003;

    pub const CONSTANT: u32 = 1 << 0;
    pub const VERTEX: u32 = 1 << 1;
    pub const DIFFUSE: u32 = 1 << 2;
    pub const SPECULAR: u32 = 1 << 3;
    pub const CHANNEL_FIELD: u32 = CONSTANT | VERTEX | DIFFUSE | SPECULAR;

    pub const TEX0: u32 = 1 << 4;
    pub const TEX1: u32 = 1 << 5;
    pub const TEX2: u32 = 1 << 6;
    pub const TEX3: u32 = 1 << 7;
    pub const TEX4: u32 = 1 << 8;
    pub const TEX5: u32 = 1 << 9;
    pub const TEX6: u32 = 1 << 10;
    pub const TEX7: u32 = 1 << 11;
    pub const TEXTURES: u32 = 0x0000_0FF0;
    pub const TOON: u32 = 1 << 12;

    pub const ALPHA_SHIFT: u32 = 13;
    pub const ALPHA_BITS: u32 = 0x0000_6000;
    pub const ALPHA_COMPAT: u32 = 0x0000_0000;
    pub const ALPHA_MAT: u32 = 0x0000_2000;
    pub const ALPHA_VTX: u32 = 0x0000_4000;
    pub const ALPHA_BOTH: u32 = 0x0000_6000;

    pub const SHADOW: u32 = 1 << 26;
    pub const ZMODE_ALWAYS: u32 = 1 << 27;
    pub const NO_ZUPDATE: u32 = 1 << 29;
    pub const XLU: u32 = 1 << 30;
    pub const BLENDING: u32 = XLU | NO_ZUPDATE;
}

/// Material Object — material colors, texture references, render flags.
///
/// Unlike a PBR material, an MObj describes fixed-function HSD/GX state whose
/// ordered TObjs may participate in multiple texture-combiner stages.
///
/// Layout (0x18 bytes):
///   0x00: class_name_ptr (u32)
///   0x04: render_flags (u32)
///   0x08: tobj_ptr (u32) — texture object linked list
///   0x0C: material_ptr (u32) — material colors struct
///   0x10: renderdesc_ptr (u32) — `MObjLoad` does not read it
///   0x14: pe_desc_ptr (u32) — pixel engine descriptor
#[derive(Debug, Clone)]
pub struct MObj {
    pub offset: u32,
    pub render_flags: u32,
    pub tobj_ptr: Option<u32>,
    pub material_ptr: Option<u32>,
    pub renderdesc_ptr: Option<u32>,
    pub pe_desc_ptr: Option<u32>,
    pub material: Option<Material>,
    pub pe_desc: Option<PEDesc>,
}

impl MObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "MObj", offset).require_extent(0x18)?;

        let render_flags = source.u32(0x04)?;
        let tobj_ptr = source.pointer("tobj", 0x08)?;
        let material_ptr = source.pointer("material", 0x0C)?;
        let renderdesc_ptr = source.pointer("renderdesc", 0x10)?;
        let pe_desc_ptr = source.pointer("pe_desc", 0x14)?;

        let material = material_ptr
            .map(|offset| Material::parse(dat, offset))
            .transpose()?;
        let pe_desc = pe_desc_ptr
            .map(|offset| PEDesc::parse(dat, offset))
            .transpose()?;

        Ok(Self {
            offset,
            render_flags,
            tobj_ptr,
            material_ptr,
            renderdesc_ptr,
            pe_desc_ptr,
            material,
            pe_desc,
        })
    }
}

/// Pixel-engine descriptor copied into runtime state by the MObj loader.
///
/// Layout (0x0C bytes): one raw flag byte, three alpha-reference bytes, blend
/// mode/factors/logic op, depth compare, and alpha-compare operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PEDesc {
    pub flags: u8,
    pub ref0: u8,
    pub ref1: u8,
    pub dst_alpha: u8,
    pub blend_mode: u8,
    pub src_factor: u8,
    pub dst_factor: u8,
    pub logic_op: u8,
    pub z_compare: u8,
    pub alpha_compare0: u8,
    pub alpha_op: u8,
    pub alpha_compare1: u8,
}

impl PEDesc {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "PEDesc", offset).require_extent(0x0C)?;

        Ok(Self {
            flags: source.u8(0x00)?,
            ref0: source.u8(0x01)?,
            ref1: source.u8(0x02)?,
            dst_alpha: source.u8(0x03)?,
            blend_mode: source.u8(0x04)?,
            src_factor: source.u8(0x05)?,
            dst_factor: source.u8(0x06)?,
            logic_op: source.u8(0x07)?,
            z_compare: source.u8(0x08)?,
            alpha_compare0: source.u8(0x09)?,
            alpha_op: source.u8(0x0A)?,
            alpha_compare1: source.u8(0x0B)?,
        })
    }
}

/// Material colors.
///
/// Layout (0x14 bytes):
///   0x00: ambient RGBA (4 bytes)
///   0x04: diffuse RGBA (4 bytes)
///   0x08: specular RGBA (4 bytes)
///   0x0C: alpha (f32)
///   0x10: shininess (f32)
#[derive(Debug, Clone)]
pub struct Material {
    pub ambient: [u8; 4],
    pub diffuse: [u8; 4],
    pub specular: [u8; 4],
    pub alpha: f32,
    pub shininess: f32,
}

impl Material {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "Material", offset).require_extent(0x14)?;

        Ok(Self {
            ambient: source.array(0x00)?,
            diffuse: source.array(0x04)?,
            specular: source.array(0x08)?,
            alpha: source.f32(0x0C)?,
            shininess: source.f32(0x10)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::MObj;
    use crate::descriptor::DescriptorParseError;
    use crate::{DatFile, DatPointerError};

    fn synthetic_dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    #[test]
    fn pointer_fields_report_exact_relocation_errors() {
        for (relative, field) in [
            (0x08_u32, "tobj"),
            (0x0c, "material"),
            (0x10, "renderdesc"),
            (0x14, "pe_desc"),
        ] {
            let mut data = vec![0; 0x18];
            let field_offset = relative as usize;
            data[field_offset..field_offset + 4].copy_from_slice(&4_u32.to_be_bytes());

            assert_eq!(
                MObj::parse(&synthetic_dat_with_relocations(data, Vec::new()), 0).unwrap_err(),
                DescriptorParseError::InvalidPointer {
                    descriptor: "MObj",
                    field,
                    field_offset: relative,
                    source: DatPointerError::MissingRelocation,
                }
            );
        }
    }

    #[test]
    fn present_truncated_child_descriptors_fail_mobj_parse() {
        for (relative, descriptor) in [(0x0c_u32, "Material"), (0x14, "PEDesc")] {
            let mut data = vec![0; 0x18];
            let field_offset = relative as usize;
            data[field_offset..field_offset + 4].copy_from_slice(&0x18_u32.to_be_bytes());

            assert_eq!(
                MObj::parse(&synthetic_dat_with_relocations(data, vec![relative]), 0).unwrap_err(),
                DescriptorParseError::Truncated {
                    descriptor,
                    offset: 0x18,
                }
            );
        }
    }

    #[test]
    fn pe_descriptor_preserves_flags_and_gx_operands() {
        let mut data = vec![0; 0x40];
        data[0x14..0x18].copy_from_slice(&0x20_u32.to_be_bytes());
        data[0x20..0x2C].copy_from_slice(&[0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);

        let mobj = MObj::parse(&synthetic_dat_with_relocations(data, vec![0x14]), 0)
            .expect("synthetic MObj");
        let pe = mobj.pe_desc.expect("PE descriptor");
        assert_eq!(pe.flags, 0xFF);
        assert_eq!((pe.ref0, pe.ref1, pe.dst_alpha), (1, 2, 3));
        assert_eq!((pe.blend_mode, pe.src_factor, pe.dst_factor), (4, 5, 6));
        assert_eq!(pe.logic_op, 7);
        assert_eq!(pe.z_compare, 8);
        assert_eq!(
            (pe.alpha_compare0, pe.alpha_op, pe.alpha_compare1),
            (9, 10, 11)
        );
    }

    #[test]
    fn parse_preserves_uninterpreted_renderdesc_pointer() {
        let mut data = vec![0; 0x40];
        data[4..8].copy_from_slice(&0x6000_0014_u32.to_be_bytes());
        data[0x0C..0x10].copy_from_slice(&0x20_u32.to_be_bytes());
        data[0x10..0x14].copy_from_slice(&0x30_u32.to_be_bytes());

        let mobj = MObj::parse(&synthetic_dat_with_relocations(data, vec![0x0C, 0x10]), 0)
            .expect("synthetic MObj");
        assert_eq!(mobj.render_flags, 0x6000_0014);
        assert_eq!(mobj.material_ptr, Some(0x20));
        assert!(mobj.material.is_some());
        assert_eq!(mobj.renderdesc_ptr, Some(0x30));
        assert_eq!(mobj.pe_desc_ptr, None);
    }
}
