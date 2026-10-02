use super::header::DatHeader;
use super::reader::Reader;

/// Parse the ordinary relocation table.
///
/// Each entry is a data-relative pointer-field site. The u32 stored at that site
/// is the data-relative pointer target, including target zero.
///
/// Returns the serialized sites sorted for deterministic membership checks, or
/// `None` when the table runs past the end of `raw`.
pub(crate) fn parse_relocation_sites(raw: &[u8], header: &DatHeader) -> Option<Vec<u32>> {
    let mut reader = Reader::new(raw);
    reader.seek(header.reloc_table_offset());
    let count = header.reloc_count as usize;
    // The table has to fit in the input, so the input bounds the allocation.
    let mut sites = Vec::with_capacity(count.min(reader.remaining() / 4));
    for _ in 0..count {
        sites.push(reader.read_u32()?);
    }

    sites.sort_unstable();
    Some(sites)
}
