use super::{DatFile, DescriptorParseError, DescriptorReader};

/// Display Object — links material (MObj) and polygon (PObj) data.
///
/// For a traditional renderer, this is closer to a draw grouping than a mesh:
/// it connects one material-state chain to one or more geometry batches.
///
/// Layout (0x10 bytes):
///   0x00: class_name_ptr (u32)
///   0x04: next_ptr (u32) — linked list
///   0x08: mobj_ptr (u32) — material object
///   0x0C: pobj_ptr (u32) — polygon object
#[derive(Debug, Clone)]
pub struct DObj {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub mobj_ptr: Option<u32>,
    pub pobj_ptr: Option<u32>,
}

impl DObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "DObj", offset).require_extent(0x10)?;

        Ok(Self {
            offset,
            next_ptr: source.pointer("next", 0x04)?,
            mobj_ptr: source.pointer("mobj", 0x08)?,
            pobj_ptr: source.pointer("pobj", 0x0C)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DObj;
    use crate::descriptor::DescriptorParseError;
    use crate::{DatFile, DatPointerError};

    fn synthetic_dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    #[test]
    fn pointer_fields_report_exact_relocation_errors() {
        for (relative, field) in [(0x04_u32, "next"), (0x08, "mobj"), (0x0c, "pobj")] {
            let mut data = vec![0; 0x10];
            let field_offset = relative as usize;
            data[field_offset..field_offset + 4].copy_from_slice(&4_u32.to_be_bytes());

            assert_eq!(
                DObj::parse(&synthetic_dat_with_relocations(data, Vec::new()), 0).unwrap_err(),
                DescriptorParseError::InvalidPointer {
                    descriptor: "DObj",
                    field,
                    field_offset: relative,
                    source: DatPointerError::MissingRelocation,
                }
            );
        }
    }
}
