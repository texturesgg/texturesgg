use super::{DatFile, DescriptorParseError, DescriptorReader};

/// Source-defined `HSD_TObjDesc.blend_flags` values.
///
/// Coordinate, color-map, and alpha-map values are mutually exclusive fields;
/// light-map and bump values are independent bits. The loader copies the raw
/// descriptor word before setting `MTX_DIRTY` on the runtime object.
pub mod texture_flags {
    pub const COORD_UV: u32 = 0;
    pub const COORD_REFLECTION: u32 = 1;
    pub const COORD_HILIGHT: u32 = 2;
    pub const COORD_SHADOW: u32 = 3;
    pub const COORD_TOON: u32 = 4;
    pub const COORD_GRADATION: u32 = 5;
    pub const COORD_BACKLIGHT: u32 = 6;
    pub const COORD_MASK: u32 = 0x0000_000F;

    pub const LIGHTMAP_DIFFUSE: u32 = 1 << 4;
    pub const LIGHTMAP_SPECULAR: u32 = 1 << 5;
    pub const LIGHTMAP_AMBIENT: u32 = 1 << 6;
    pub const LIGHTMAP_EXT: u32 = 1 << 7;
    pub const LIGHTMAP_SHADOW: u32 = 1 << 8;
    pub const LIGHTMAP_MASK: u32 = 0x0000_01F0;

    pub const COLORMAP_NONE: u32 = 0 << 16;
    pub const COLORMAP_ALPHA_MASK: u32 = 1 << 16;
    pub const COLORMAP_RGB_MASK: u32 = 2 << 16;
    pub const COLORMAP_BLEND: u32 = 3 << 16;
    pub const COLORMAP_MODULATE: u32 = 4 << 16;
    pub const COLORMAP_REPLACE: u32 = 5 << 16;
    pub const COLORMAP_PASS: u32 = 6 << 16;
    pub const COLORMAP_ADD: u32 = 7 << 16;
    pub const COLORMAP_SUB: u32 = 8 << 16;
    pub const COLORMAP_MASK: u32 = 0x000F_0000;

    pub const ALPHAMAP_NONE: u32 = 0 << 20;
    pub const ALPHAMAP_ALPHA_MASK: u32 = 1 << 20;
    pub const ALPHAMAP_BLEND: u32 = 2 << 20;
    pub const ALPHAMAP_MODULATE: u32 = 3 << 20;
    pub const ALPHAMAP_REPLACE: u32 = 4 << 20;
    pub const ALPHAMAP_PASS: u32 = 5 << 20;
    pub const ALPHAMAP_ADD: u32 = 6 << 20;
    pub const ALPHAMAP_SUB: u32 = 7 << 20;
    pub const ALPHAMAP_MASK: u32 = 0x00F0_0000;

    pub const BUMP: u32 = 1 << 24;
    pub const MTX_DIRTY: u32 = 1 << 31;

    pub const fn coord(flags: u32) -> u32 {
        flags & COORD_MASK
    }
}

/// Texture Object — one ordered texture-stage usage.
///
/// A TObj is more than a texture asset: it also carries coordinate generation,
/// UV transformation, sampler wrapping, and combiner flags. Multiple TObjs can
/// reference shared image data while behaving differently in a material.
///
/// Layout (0x5C bytes):
///   0x00: class_name_ptr (u32)
///   0x04: next_ptr (u32) — linked list
///   0x08: tex_map_id (u32)
///   0x0C: tex_gen_src (u32)
///   0x10-0x30: transform (rotation, scale, translation — 9x f32)
///   0x34: wrap_s (u32)
///   0x38: wrap_t (u32)
///   0x3C: repeat_s (u8)
///   0x3D: repeat_t (u8)
///   0x40: blend_flags (u32) — raw coordinate/light-map/combiner fields
///   0x44: blending (f32)
///   0x48: magnification filter (u32)
///   0x4C: image_ptr (u32) — pointer to HSD_Image struct
///   0x50: tlut_ptr (u32) — palette data
///   0x54: lod_ptr (u32) — texture LOD descriptor
///   0x58: tev_ptr (u32) — per-texture TEV descriptor
#[derive(Debug, Clone)]
pub struct TObj {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub tex_map_id: u32,
    pub tex_gen_src: u32,
    /// UV transform: rotation (rx, ry, rz), scale (sx, sy, sz), translation (tx, ty, tz)
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub translation: [f32; 3],
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub repeat_s: u8,
    pub repeat_t: u8,
    pub flags: u32,
    pub blending: f32,
    pub mag_filter: u32,
    pub image_ptr: Option<u32>,
    pub tlut_ptr: Option<u32>,
    pub lod_ptr: Option<u32>,
    pub tev_ptr: Option<u32>,
    pub image: Option<ImageDesc>,
    pub tlut: Option<TlutDesc>,
    pub tev_desc: Option<TObjTevDesc>,
}

impl TObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "TObj", offset).require_extent(0x5c)?;

        let next_ptr = source.pointer("next", 0x04)?;
        let tex_map_id = source.u32(0x08)?;
        let tex_gen_src = source.u32(0x0C)?;

        let rotation = [source.f32(0x10)?, source.f32(0x14)?, source.f32(0x18)?];
        let scale = [source.f32(0x1C)?, source.f32(0x20)?, source.f32(0x24)?];
        let translation = [source.f32(0x28)?, source.f32(0x2C)?, source.f32(0x30)?];

        let wrap_s = source.u32(0x34)?;
        let wrap_t = source.u32(0x38)?;
        let repeat_s = source.u8(0x3C)?;
        let repeat_t = source.u8(0x3D)?;
        let flags = source.u32(0x40)?;
        let blending = source.f32(0x44)?;
        let mag_filter = source.u32(0x48)?;

        let image_ptr = source.pointer("image", 0x4C)?;
        let tlut_ptr = source.pointer("tlut", 0x50)?;
        let lod_ptr = source.pointer("lod", 0x54)?;
        let tev_ptr = source.pointer("tev", 0x58)?;

        let image = image_ptr
            .map(|offset| ImageDesc::parse(dat, offset))
            .transpose()?;
        let tlut = tlut_ptr
            .map(|offset| TlutDesc::parse(dat, offset))
            .transpose()?;
        let tev_desc = tev_ptr
            .map(|offset| TObjTevDesc::parse(dat, offset))
            .transpose()?;

        Ok(Self {
            offset,
            next_ptr,
            tex_map_id,
            tex_gen_src,
            rotation,
            scale,
            translation,
            wrap_s,
            wrap_t,
            repeat_s,
            repeat_t,
            flags,
            blending,
            mag_filter,
            image_ptr,
            tlut_ptr,
            lod_ptr,
            tev_ptr,
            image,
            tlut,
            tev_desc,
        })
    }
}

/// Declared `HSD_TObjTevDesc.active` bits.
pub mod tev_active {
    pub const KONST_R: u32 = 1 << 0;
    pub const KONST_G: u32 = 1 << 1;
    pub const KONST_B: u32 = 1 << 2;
    pub const KONST_A: u32 = 1 << 3;
    pub const TEV0_R: u32 = 1 << 4;
    pub const TEV0_G: u32 = 1 << 5;
    pub const TEV0_B: u32 = 1 << 6;
    pub const TEV0_A: u32 = 1 << 7;
    pub const TEV1_R: u32 = 1 << 8;
    pub const TEV1_G: u32 = 1 << 9;
    pub const TEV1_B: u32 = 1 << 10;
    pub const TEV1_A: u32 = 1 << 11;
    pub const COLOR_TEV: u32 = 1 << 30;
    pub const ALPHA_TEV: u32 = 1 << 31;
    pub const DECLARED_MASK: u32 = 0xC000_0FFF;
}

/// HSD-specific TObj TEV alpha-input selectors.
pub mod tev_alpha_input {
    pub const TEXA: u8 = 4;
    pub const ZERO: u8 = 7;
    pub const KONST_R: u8 = 0x40;
    pub const KONST_G: u8 = 0x41;
    pub const KONST_B: u8 = 0x42;
    pub const KONST_A: u8 = 0x43;
    pub const TEX0_A: u8 = 0x44;
    pub const TEX1_A: u8 = 0x45;
}

/// GX operation ordinals used by custom TObj TEV validation.
///
/// The serialized fields remain raw bytes; these constants only make matching
/// declarations and admitted/unsupported boundaries explicit.
pub mod tev_op {
    pub const ADD: u8 = 0;
    pub const SUB: u8 = 1;
    pub const R8_GT: u8 = 8;
    pub const R8_EQ: u8 = 9;
    pub const GR16_GT: u8 = 10;
    pub const GR16_EQ: u8 = 11;
    pub const BGR24_GT: u8 = 12;
    pub const BGR24_EQ: u8 = 13;
    pub const RGB8_GT: u8 = 14;
    pub const A8_GT: u8 = 14;
    pub const RGB8_EQ: u8 = 15;
    pub const A8_EQ: u8 = 15;
}

pub mod tev_bias {
    pub const ZERO: u8 = 0;
    pub const ADD_HALF: u8 = 1;
    pub const SUB_HALF: u8 = 2;
}

pub mod tev_scale {
    pub const SCALE_1: u8 = 0;
    pub const SCALE_2: u8 = 1;
    pub const SCALE_4: u8 = 2;
    pub const DIVIDE_2: u8 = 3;
}

/// HSD-specific TObj TEV color-input selectors.
pub mod tev_color_input {
    pub const TEXC: u8 = 8;
    pub const TEXA: u8 = 9;
    pub const ONE: u8 = 12;
    pub const HALF: u8 = 13;
    pub const ZERO: u8 = 15;
    pub const KONST_RGB: u8 = 0x80;
    pub const KONST_RRR: u8 = 0x81;
    pub const KONST_GGG: u8 = 0x82;
    pub const KONST_BBB: u8 = 0x83;
    pub const KONST_AAA: u8 = 0x84;
    pub const TEX0_RGB: u8 = 0x85;
    pub const TEX0_AAA: u8 = 0x86;
    pub const TEX1_RGB: u8 = 0x87;
    pub const TEX1_AAA: u8 = 0x88;
}

/// Serialized per-texture fixed-function TEV descriptor (0x20 bytes).
///
/// Enum and selector bytes remain raw so unsupported or source-unlisted values
/// survive parsing. `active` bits 30 and 31 gate matching custom color and alpha
/// expression construction; lower component bits are declared but not consumed
/// by the checked matching TObj execution path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TObjTevDesc {
    pub color_op: u8,
    pub alpha_op: u8,
    pub color_bias: u8,
    pub alpha_bias: u8,
    pub color_scale: u8,
    pub alpha_scale: u8,
    pub color_clamp: u8,
    pub alpha_clamp: u8,
    pub color_inputs: [u8; 4],
    pub alpha_inputs: [u8; 4],
    pub konst: [u8; 4],
    pub tev0: [u8; 4],
    pub tev1: [u8; 4],
    pub active: u32,
}

impl TObjTevDesc {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "TObjTevDesc", offset).require_extent(0x20)?;

        Ok(Self {
            color_op: source.u8(0x00)?,
            alpha_op: source.u8(0x01)?,
            color_bias: source.u8(0x02)?,
            alpha_bias: source.u8(0x03)?,
            color_scale: source.u8(0x04)?,
            alpha_scale: source.u8(0x05)?,
            color_clamp: source.u8(0x06)?,
            alpha_clamp: source.u8(0x07)?,
            color_inputs: source.array(0x08)?,
            alpha_inputs: source.array(0x0C)?,
            konst: source.array(0x10)?,
            tev0: source.array(0x14)?,
            tev1: source.array(0x18)?,
            active: source.u32(0x1C)?,
        })
    }

    pub const fn color_tev_active(&self) -> bool {
        self.active & tev_active::COLOR_TEV != 0
    }

    pub const fn alpha_tev_active(&self) -> bool {
        self.active & tev_active::ALPHA_TEV != 0
    }
}

/// Image descriptor — pixel data pointer, dimensions, format, and LOD range
/// (`HSD_ImageDesc`, `tobj.h`).
///
/// Layout (0x18 bytes):
///   0x00: data_ptr (u32) — pointer to pixel data
///   0x04: width (u16)
///   0x06: height (u16)
///   0x08: format (u32) — GXTexFmt
///   0x0C: mipmap (u32) — GXBool; the data then holds a mip chain
///   0x10: min_lod (f32)
///   0x14: max_lod (f32)
#[derive(Debug, Clone)]
pub struct ImageDesc {
    pub data_ptr: Option<u32>,
    pub width: u16,
    pub height: u16,
    pub format: u32,
    pub mipmap: u32,
    pub min_lod: f32,
    pub max_lod: f32,
}

impl ImageDesc {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        // HSD_TObjSetup reads every field (GXInitTexObjLOD takes both LODs).
        let source = DescriptorReader::new(dat, "ImageDesc", offset).require_extent(0x18)?;
        Ok(Self {
            data_ptr: source.pointer("data", 0x00)?,
            width: source.u16(0x04)?,
            height: source.u16(0x06)?,
            format: source.u32(0x08)?,
            mipmap: source.u32(0x0c)?,
            min_lod: source.f32(0x10)?,
            max_lod: source.f32(0x14)?,
        })
    }

    pub fn format_name(&self) -> &'static str {
        match self.format {
            0 => "I4",
            1 => "I8",
            2 => "IA4",
            3 => "IA8",
            4 => "RGB565",
            5 => "RGB5A3",
            6 => "RGBA8",
            8 => "CI4",
            9 => "CI8",
            10 => "CI14X2",
            14 => "CMP",
            _ => "Unknown",
        }
    }
}

/// Palette (TLUT) descriptor.
///
/// Layout:
///   0x00: data_ptr (u32) — palette data
///   0x04: format (u32) — GXTlutFmt
///   0x08: gx_tlut (u32)
///   0x0C: color_count (u16)
#[derive(Debug, Clone)]
pub struct TlutDesc {
    pub data_ptr: Option<u32>,
    pub format: u32,
    pub color_count: u16,
}

impl TlutDesc {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "TlutDesc", offset).require_extent(0x0e)?;
        let data_ptr = source.pointer("data", 0x00)?;
        let format = source.u32(0x04)?;
        let color_count = source.u16(0x0C)?;

        Ok(Self {
            data_ptr,
            format,
            color_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{DescriptorParseError, TObj};
    use crate::{DatFile, DatPointerError};

    fn synthetic_dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    #[test]
    fn unrelocated_nonzero_pointer_reports_exact_field() {
        let mut data = vec![0; 0x5c];
        data[0x4c..0x50].copy_from_slice(&0x10_u32.to_be_bytes());

        assert_eq!(
            TObj::parse(&synthetic_dat_with_relocations(data, Vec::new()), 0).unwrap_err(),
            DescriptorParseError::InvalidPointer {
                descriptor: "TObj",
                field: "image",
                field_offset: 0x4c,
                source: DatPointerError::MissingRelocation,
            }
        );
    }

    #[test]
    fn present_truncated_child_descriptors_fail_tobj_parse() {
        for (field_offset, descriptor) in [
            (0x4c_u32, "ImageDesc"),
            (0x50, "TlutDesc"),
            (0x58, "TObjTevDesc"),
        ] {
            let mut data = vec![0; 0x5c];
            let field = field_offset as usize;
            data[field..field + 4].copy_from_slice(&0x5c_u32.to_be_bytes());

            assert_eq!(
                TObj::parse(&synthetic_dat_with_relocations(data, vec![field_offset]), 0)
                    .unwrap_err(),
                DescriptorParseError::Truncated {
                    descriptor,
                    offset: 0x5c,
                }
            );
        }
    }

    #[test]
    fn image_descriptor_resolves_data_pointer_at_nonzero_offset() {
        let mut data = vec![0; 0x80];
        data[0x4c..0x50].copy_from_slice(&0x60_u32.to_be_bytes());
        data[0x60..0x64].copy_from_slice(&0x20_u32.to_be_bytes());
        data[0x64..0x66].copy_from_slice(&32_u16.to_be_bytes());
        data[0x66..0x68].copy_from_slice(&16_u16.to_be_bytes());
        data[0x68..0x6c].copy_from_slice(&6_u32.to_be_bytes());

        let tobj = TObj::parse(&synthetic_dat_with_relocations(data, vec![0x4c, 0x60]), 0)
            .expect("TObj with a complete image descriptor");
        let image = tobj.image.expect("image descriptor");
        assert_eq!(tobj.image_ptr, Some(0x60));
        assert_eq!(image.data_ptr, Some(0x20));
        assert_eq!((image.width, image.height, image.format), (32, 16, 6));
    }

    #[test]
    fn tobj_tev_descriptor_preserves_all_bytes_and_source_masks() {
        let mut data = vec![0; 0x80];
        data[0x58..0x5C].copy_from_slice(&0x60_u32.to_be_bytes());
        data[0x60..0x80].copy_from_slice(&(0..0x20).collect::<Vec<u8>>());
        data[0x7C..0x80].copy_from_slice(&0xC123_4567_u32.to_be_bytes());

        let tobj = TObj::parse(&synthetic_dat_with_relocations(data, vec![0x58]), 0).expect("TObj");
        let tev = tobj.tev_desc.expect("TEV descriptor");
        assert_eq!(
            (
                tev.color_op,
                tev.alpha_op,
                tev.color_bias,
                tev.alpha_bias,
                tev.color_scale,
                tev.alpha_scale,
                tev.color_clamp,
                tev.alpha_clamp,
            ),
            (0, 1, 2, 3, 4, 5, 6, 7)
        );
        assert_eq!(tev.color_inputs, [8, 9, 10, 11]);
        assert_eq!(tev.alpha_inputs, [12, 13, 14, 15]);
        assert_eq!(tev.konst, [16, 17, 18, 19]);
        assert_eq!(tev.tev0, [20, 21, 22, 23]);
        assert_eq!(tev.tev1, [24, 25, 26, 27]);
        assert_eq!(tev.active, 0xC123_4567);
        assert!(tev.color_tev_active());
        assert!(tev.alpha_tev_active());
        let mut inactive = tev.clone();
        inactive.active = 0;
        assert!(!inactive.color_tev_active());
        assert!(!inactive.alpha_tev_active());
    }
}
