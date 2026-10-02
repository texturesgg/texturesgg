use crate::{Error, Result, io};

/// An entry in the GCM file system table.
#[derive(Debug, Clone)]
pub struct FstEntry {
    /// File or directory name.
    pub name: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// File offset within the ISO (for files).
    pub offset: u32,
    /// File size in bytes (for files), or next entry index (for directories).
    pub size: u32,
    /// Index of this entry in the FST (needed to write back changes).
    pub fst_index: usize,
}

/// Parse the FST from raw ISO data.
pub fn parse_fst(
    data: &[u8],
    fst_offset: usize,
    total_entries: usize,
    string_table_offset: usize,
) -> Result<Vec<FstEntry>> {
    let entries_size = total_entries
        .checked_mul(12)
        .ok_or_else(|| Error::InvalidIso("FST entry count overflow".into()))?;
    let entries_end = fst_offset
        .checked_add(entries_size)
        .ok_or_else(|| Error::InvalidIso("FST entry range overflow".into()))?;
    if entries_end > data.len()
        || string_table_offset < entries_end
        || string_table_offset > data.len()
    {
        return Err(Error::InvalidIso(
            "FST entries or string table are out of bounds".into(),
        ));
    }

    let mut entries = Vec::with_capacity(total_entries);
    for i in 0..total_entries {
        let entry_offset = fst_offset + i * 12;
        let flags = data[entry_offset];
        let name_offset = io::read_u24_be(data, entry_offset + 1) as usize;
        let name_start = string_table_offset
            .checked_add(name_offset)
            .filter(|offset| *offset < data.len())
            .ok_or_else(|| Error::InvalidIso("FST name offset is out of bounds".into()))?;
        let file_offset = io::read_u32_be(data, entry_offset + 4);
        let file_size = io::read_u32_be(data, entry_offset + 8);
        let name = io::read_cstring(data, name_start);

        entries.push(FstEntry {
            is_dir: flags == 1,
            name,
            offset: file_offset,
            size: file_size,
            fst_index: i,
        });
    }

    Ok(entries)
}
