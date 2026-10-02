use dat_parser::DatFile;
use dat_parser::DatParseError;

fn write_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn rejects_header_counts_that_exceed_file_bounds() {
    let mut raw = vec![0_u8; 0x20];
    let raw_len = raw.len() as u32;
    write_u32(&mut raw, 0, raw_len);
    write_u32(&mut raw, 8, u32::MAX);

    assert!(matches!(
        DatFile::parse(&raw),
        Err(DatParseError::ResourceLimit {
            resource: "relocation",
            ..
        })
    ));
}

#[test]
fn rejects_duplicated_root_names_above_the_symbol_budget() {
    let symbol_len = 600_000usize;
    let mut raw = vec![0; 0x20 + 16 + symbol_len + 1];
    let raw_len = raw.len() as u32;
    write_u32(&mut raw, 0, raw_len);
    write_u32(&mut raw, 0x0c, 2);
    raw[0x30..0x30 + symbol_len].fill(b'a');
    raw[0x30 + symbol_len] = 0;

    assert!(matches!(
        DatFile::parse(&raw),
        Err(DatParseError::ResourceLimit {
            resource: "root symbol byte",
            ..
        })
    ));
}
