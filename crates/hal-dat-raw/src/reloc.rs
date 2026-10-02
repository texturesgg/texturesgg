use super::header::DatHeader;
use super::reader::Reader;

/// Parse the ordinary relocation table.
///
/// Each entry is a data-relative pointer-field site. The u32 stored at that site
/// is the data-relative pointer target, including target zero.
///
/// Returns the serialized sites sorted for deterministic membership checks.
pub fn parse_relocation_sites(raw: &[u8], header: &DatHeader) -> Vec<u32> {
    let base = header.reloc_table_offset();
    let mut reader = Reader::new(raw);
    let mut sites = Vec::with_capacity(header.reloc_count as usize);

    for i in 0..header.reloc_count as usize {
        reader.seek(base + i * 4);
        if let Some(offset) = reader.read_u32() {
            sites.push(offset);
        }
    }

    sites.sort_unstable();
    sites
}
