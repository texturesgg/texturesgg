use super::{DatFile, DescriptorParseError, DescriptorReader};
use crate::math::Mat4;

/// JObj flags from HSD.
pub mod flags {
    pub const SKELETON: u32 = 1 << 0;
    pub const SKELETON_ROOT: u32 = 1 << 1;
    pub const ENVELOPE_MODEL: u32 = 1 << 2;
    pub const CLASSICAL_SCALE: u32 = 1 << 3;
    pub const HIDDEN: u32 = 1 << 4;
    pub const PTCL: u32 = 1 << 5;
    pub const MTX_DIRTY: u32 = 1 << 6;
    pub const LIGHTING: u32 = 1 << 7;
    pub const TEXGEN: u32 = 1 << 8;

    pub const BILLBOARD_FIELD: u32 = 0xE00; // bits 9-11
    pub const BILLBOARD: u32 = 0x200;
    pub const VBILLBOARD: u32 = 0x400;
    pub const HBILLBOARD: u32 = 0x600;
    pub const RBILLBOARD: u32 = 0x800;

    pub const INSTANCE: u32 = 1 << 12;
    pub const PBILLBOARD: u32 = 0x2000;
    pub const SPLINE: u32 = 1 << 14;
    pub const FLIP_IK: u32 = 1 << 15;
    pub const SPECULAR: u32 = 1 << 16;
    pub const USE_QUATERNION: u32 = 1 << 17;
    // The source leaves these names unknown, but HSD_JObjDispAll uses them as
    // the per-node OPA, XLU, and TEXEDGE display gates, respectively.
    pub const UNK_B18: u32 = 1 << 18;
    pub const UNK_B19: u32 = 1 << 19;
    pub const UNK_B20: u32 = 1 << 20;

    pub const NULL_OBJ: u32 = 0 << 21;
    pub const JOINT1: u32 = 1 << 21;
    pub const JOINT2: u32 = 2 << 21;
    pub const JOINT: u32 = 3 << 21;
    pub const EFFECTOR: u32 = 3 << 21;
    pub const USER_DEF_MTX: u32 = 1 << 23;
    pub const MTX_INDEP_PARENT: u32 = 1 << 24;
    pub const MTX_INDEP_SRT: u32 = 1 << 25;
    pub const UNK_B26: u32 = 1 << 26;
    pub const UNK_B27: u32 = 1 << 27;
    pub const ROOT_OPA: u32 = 1 << 28;
    pub const ROOT_XLU: u32 = 1 << 29;
    pub const ROOT_TEXEDGE: u32 = 1 << 30;
    pub const ROOT_MASK: u32 = ROOT_OPA | ROOT_XLU | ROOT_TEXEDGE;
}

/// Raw JObj data read from the .dat file.
///
/// In a conventional scene graph, a JObj is closest to a transform node and
/// often doubles as a skeleton joint/bone. Its DObj pointer attaches drawables.
///
/// Layout (0x40 bytes):
///   0x00: class_name_ptr (u32)
///   0x04: flags (u32)
///   0x08: child_ptr (u32)
///   0x0C: next_ptr (u32)
///   0x10: dobj_ptr (u32) — or spline/particle depending on flags
///   0x14: rx, ry, rz (3x f32) — rotation in radians
///   0x20: sx, sy, sz (3x f32) — scale
///   0x2C: tx, ty, tz (3x f32) — translation
///   0x38: inverse_bind_ptr (u32) — pointer to 4x3 matrix
///   0x3C: robj_ptr (u32)
#[derive(Debug, Clone)]
pub struct JObj {
    pub offset: u32,
    pub flags: u32,
    pub child_ptr: Option<u32>,
    pub next_ptr: Option<u32>,
    /// Raw flag-selected `0x10` union pointer.
    pub union_ptr: Option<u32>,
    pub dobj_ptr: Option<u32>,
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub translation: [f32; 3],
    pub inverse_bind_ptr: Option<u32>,
    pub robj_ptr: Option<u32>,
}

impl JObj {
    /// Parse a JObj at the given offset in the data section.
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "JObj", offset).require_extent(0x40)?;

        let jobj_flags = source.u32(0x04)?;
        let child_ptr = source.pointer("child", 0x08)?;
        let next_ptr = source.pointer("next", 0x0C)?;

        let union_ptr = source.pointer("union", 0x10)?;
        // Matching loading gives SPLINE precedence over PTCL, then treats the
        // union as a DObj list only when neither flag is present.
        let dobj_ptr = if jobj_flags & (flags::SPLINE | flags::PTCL) == 0 {
            union_ptr
        } else {
            None
        };

        // Read rotation
        let rotation = [source.f32(0x14)?, source.f32(0x18)?, source.f32(0x1C)?];

        // Read scale
        let scale = [source.f32(0x20)?, source.f32(0x24)?, source.f32(0x28)?];

        // Read translation
        let translation = [source.f32(0x2C)?, source.f32(0x30)?, source.f32(0x34)?];

        let inverse_bind_ptr = source.pointer("inverse_bind", 0x38)?;
        let robj_ptr = source.pointer("robj", 0x3C)?;

        Ok(Self {
            offset,
            flags: jobj_flags,
            child_ptr,
            next_ptr,
            union_ptr,
            dobj_ptr,
            rotation,
            scale,
            translation,
            inverse_bind_ptr,
            robj_ptr,
        })
    }

    pub fn inverse_bind_transform(
        &self,
        dat: &DatFile,
    ) -> Result<Option<Mat4>, DescriptorParseError> {
        let Some(offset) = self.inverse_bind_ptr else {
            return Ok(None);
        };
        let source =
            DescriptorReader::new(dat, "InverseBindTransform", offset).require_extent(0x30)?;
        let mut rows = [[0.0f32; 4]; 3];
        for (row, values) in rows.iter_mut().enumerate() {
            for (col, value) in values.iter_mut().enumerate() {
                *value = source.f32((row * 4 + col) as u32 * 4)?;
            }
        }
        Ok(Some(Mat4::from_row_major_3x4(rows)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatPointerError;

    fn dat_with_data(data: Vec<u8>) -> DatFile {
        dat_with_relocations(data, Vec::new())
    }

    fn dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    #[test]
    fn jobj_pointer_fields_report_exact_relocation_errors() {
        for (relative, field) in [
            (0x08_u32, "child"),
            (0x0c, "next"),
            (0x10, "union"),
            (0x38, "inverse_bind"),
            (0x3c, "robj"),
        ] {
            let mut data = vec![0; 0x40];
            let field_offset = relative as usize;
            data[field_offset..field_offset + 4].copy_from_slice(&4_u32.to_be_bytes());

            assert_eq!(
                JObj::parse(&dat_with_data(data), 0).unwrap_err(),
                DescriptorParseError::InvalidPointer {
                    descriptor: "JObj",
                    field,
                    field_offset: relative,
                    source: DatPointerError::MissingRelocation,
                }
            );
        }
    }

    #[test]
    fn the_union_is_a_dobj_list_only_without_spline_or_particle_flags() {
        let with_flags = |flags: u32| {
            let mut data = vec![0; 0x80];
            data[0x04..0x08].copy_from_slice(&flags.to_be_bytes());
            data[0x10..0x14].copy_from_slice(&0x40u32.to_be_bytes());
            JObj::parse(&dat_with_relocations(data, vec![0x10]), 0).expect("JObj")
        };
        assert_eq!(with_flags(0).dobj_ptr, Some(0x40));
        for flags in [flags::SPLINE, flags::PTCL, flags::SPLINE | flags::PTCL] {
            let jobj = with_flags(flags);
            assert_eq!(jobj.union_ptr, Some(0x40));
            assert_eq!(jobj.dobj_ptr, None);
        }

        assert_eq!(
            JObj::parse(&dat_with_data(vec![0; 0x3f]), 0).unwrap_err(),
            DescriptorParseError::Truncated {
                descriptor: "JObj",
                offset: 0,
            }
        );
    }

    #[test]
    fn inverse_bind_transform_preserves_absence_and_rejects_truncated_present_data() {
        let absent = JObj::parse(&dat_with_data(vec![0; 0x40]), 0).unwrap();
        assert!(
            absent
                .inverse_bind_transform(&dat_with_data(vec![0; 0x40]))
                .unwrap()
                .is_none()
        );

        let rows = [
            [1.0f32, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
        ];
        let mut complete_data = vec![0; 0x70];
        complete_data[0x38..0x3c].copy_from_slice(&0x40u32.to_be_bytes());
        for (index, value) in rows.iter().copied().flatten().enumerate() {
            let offset = 0x40 + index * 4;
            complete_data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
        }
        let complete_dat = dat_with_relocations(complete_data, vec![0x38]);
        let complete = JObj::parse(&complete_dat, 0).unwrap();
        assert_eq!(
            complete
                .inverse_bind_transform(&complete_dat)
                .unwrap()
                .unwrap()
                .0,
            Mat4::from_row_major_3x4(rows).0
        );

        let mut truncated_data = vec![0; 0x60];
        truncated_data[0x38..0x3c].copy_from_slice(&0x50u32.to_be_bytes());
        let truncated_dat = dat_with_relocations(truncated_data, vec![0x38]);
        let truncated = JObj::parse(&truncated_dat, 0).unwrap();
        assert_eq!(
            truncated
                .inverse_bind_transform(&truncated_dat)
                .unwrap_err(),
            DescriptorParseError::Truncated {
                descriptor: "InverseBindTransform",
                offset: 0x50,
            }
        );
    }
}
