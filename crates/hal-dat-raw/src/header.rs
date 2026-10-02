use super::reader::Reader;

/// HSD .dat file header (first 0x20 bytes).
///
/// Layout:
///   0x00: file_size (u32)
///   0x04: data_size (u32) — size of data section (starts at 0x20)
///   0x08: reloc_count (u32)
///   0x0C: root_count (u32)
///   0x10: extern_count (u32)
///   0x14: version (4 bytes, typically zeroed)
///   0x18-0x1F: padding
#[derive(Debug, Clone)]
pub struct DatHeader {
    pub file_size: u32,
    pub data_size: u32,
    pub reloc_count: u32,
    pub root_count: u32,
    pub extern_count: u32,
    pub version: [u8; 4],
}

/// Byte offset where the data section begins.
pub const DATA_SECTION_OFFSET: usize = 0x20;

impl DatHeader {
    pub fn parse(reader: &mut Reader) -> Option<Self> {
        let file_size = reader.read_u32()?;
        let data_size = reader.read_u32()?;
        let reloc_count = reader.read_u32()?;
        let root_count = reader.read_u32()?;
        let extern_count = reader.read_u32()?;
        let version_bytes = reader.read_bytes(4)?;
        let version = [
            version_bytes[0],
            version_bytes[1],
            version_bytes[2],
            version_bytes[3],
        ];

        Some(Self {
            file_size,
            data_size,
            reloc_count,
            root_count,
            extern_count,
            version,
        })
    }

    /// Byte offset of the relocation table in the raw file.
    pub fn reloc_table_offset(&self) -> usize {
        DATA_SECTION_OFFSET + self.data_size as usize
    }

    /// Byte offset of the root table in the raw file.
    pub fn root_table_offset(&self) -> usize {
        self.reloc_table_offset() + self.reloc_count as usize * 4
    }

    /// Byte offset of the extern table in the raw file.
    pub fn extern_table_offset(&self) -> usize {
        self.root_table_offset() + self.root_count as usize * 8
    }

    /// Byte offset of the symbol string table.
    pub fn symbol_table_offset(&self) -> usize {
        self.extern_table_offset() + self.extern_count as usize * 8
    }
}
