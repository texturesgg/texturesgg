use super::reader::Reader;

/// A named root node entry in the .dat file.
///
/// Root table entries are 8 bytes each:
///   - u32 data_offset (offset within data section)
///   - u32 symbol_offset (offset within symbol string table)
///
/// `data_offset` is the stored value, unchecked: it may lie outside the data
/// section, in a parsed file as in one built by hand. Readers resolve it
/// through [`DatFile`](crate::DatFile)'s bounded accessors.
#[derive(Debug, Clone)]
pub struct RootNode {
    /// Data-relative value from the table entry.
    ///
    /// For a public root, this is where the root object starts. `DatFile::externs`
    /// reuses this type, but there it names the first pointer-field site in the
    /// external-reference fixup chain rather than an object.
    pub data_offset: u32,
    /// Name of this root (e.g. "scene_data", "map_head", character joint names).
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootTableError {
    InvalidLayout,
    SymbolBudget,
}

/// Parse one root/extern table while bounding copied symbol bytes.
pub(crate) fn parse_root_table(
    raw: &[u8],
    root_table_offset: usize,
    symbol_table_offset: usize,
    count: u32,
    max_symbol_bytes: usize,
) -> Result<Vec<RootNode>, RootTableError> {
    let mut reader = Reader::new(raw);
    // Each entry is eight bytes of the input, so the input bounds the allocation.
    let mut roots = Vec::with_capacity((count as usize).min(raw.len() / 8));
    let mut symbol_bytes = 0usize;

    for i in 0..count as usize {
        let entry_offset = i
            .checked_mul(8)
            .and_then(|relative| root_table_offset.checked_add(relative))
            .ok_or(RootTableError::InvalidLayout)?;
        reader.seek(entry_offset);
        let data_offset = reader.read_u32().ok_or(RootTableError::InvalidLayout)?;
        let symbol_offset = reader.read_u32().ok_or(RootTableError::InvalidLayout)?;
        let symbol_offset = symbol_table_offset
            .checked_add(symbol_offset as usize)
            .ok_or(RootTableError::InvalidLayout)?;
        let name = reader
            .read_cstring_at(symbol_offset)
            .ok_or(RootTableError::InvalidLayout)?;
        symbol_bytes = symbol_bytes
            .checked_add(name.len())
            .filter(|total| *total <= max_symbol_bytes)
            .ok_or(RootTableError::SymbolBudget)?;

        roots.push(RootNode {
            data_offset,
            name: name.to_string(),
        });
    }

    Ok(roots)
}

/// Parse the extern reference table (the same serialized pair of u32 values).
///
/// Unlike a public root's first word, an extern entry's first word names the head
/// pointer-field site of a fixup chain.
pub(crate) fn parse_extern_table(
    raw: &[u8],
    extern_table_offset: usize,
    symbol_table_offset: usize,
    count: u32,
    max_symbol_bytes: usize,
) -> Result<Vec<RootNode>, RootTableError> {
    parse_root_table(
        raw,
        extern_table_offset,
        symbol_table_offset,
        count,
        max_symbol_bytes,
    )
}
