//! Minimal PNG encoder — no external dependencies.
//!
//! Produces valid PNG files from RGBA8 pixel data using uncompressed deflate
//! (store blocks). File sizes are larger than compressed PNG but this avoids
//! pulling in zlib/miniz_oxide for a debugging tool.

/// Encode RGBA8 pixels to a PNG file.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), (width * height * 4) as usize);

    let mut png = Vec::new();

    // PNG signature
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk — raw pixel data wrapped in uncompressed deflate
    // Build the deflate stream using uncompressed (store) blocks
    let deflate_data = deflate_store(rgba, width as usize, height as usize);

    // Wrap in zlib container: CMF + FLG + data + ADLER32
    let mut zlib = Vec::new();
    zlib.push(0x78); // CMF: deflate, window size 32K
    zlib.push(0x01); // FLG: check bits (0x7801 % 31 == 0)

    zlib.extend_from_slice(&deflate_data);

    // Adler32 of the uncompressed data (filter bytes + pixel rows)
    let adler = adler32_filtered(rgba, width as usize, height as usize);
    zlib.extend_from_slice(&adler.to_be_bytes());

    write_chunk(&mut png, b"IDAT", &zlib);

    // IEND chunk
    write_chunk(&mut png, b"IEND", &[]);

    png
}

/// Generate deflate store blocks for filtered PNG rows.
/// Each row is: [0x00 filter byte] + [row pixel data].
fn deflate_store(rgba: &[u8], width: usize, height: usize) -> Vec<u8> {
    let row_bytes = width * 4;
    let filtered_row_len = 1 + row_bytes; // filter byte + pixels

    // Maximum store block payload is 65535 bytes
    // We'll emit one block per row (or split if row > 65535)
    let mut out = Vec::new();

    for y in 0..height {
        let is_last = y == height - 1;
        let row_start = y * row_bytes;
        let row_data = &rgba[row_start..row_start + row_bytes];

        // Each row block: filter_byte(0) + row pixel data
        let block_len = filtered_row_len as u16;

        if filtered_row_len <= 65535 {
            // Single store block for this row
            let bfinal = if is_last { 0x01u8 } else { 0x00u8 };
            out.push(bfinal); // BFINAL + BTYPE=00 (store)
            out.extend_from_slice(&block_len.to_le_bytes());
            out.extend_from_slice(&(!block_len).to_le_bytes()); // one's complement
            out.push(0); // filter byte: None
            out.extend_from_slice(row_data);
        } else {
            // Row too long, split into multiple blocks (very wide textures)
            // First block: filter byte + partial data
            let mut remaining = row_data;
            let mut first = true;

            while first || !remaining.is_empty() {
                let payload_limit = if first { 65535 - 1 } else { 65535 }; // -1 for filter byte
                let chunk_len = remaining.len().min(payload_limit);
                let is_really_last = is_last && chunk_len == remaining.len();

                let total_block = if first { 1 + chunk_len } else { chunk_len };
                let block_sz = total_block as u16;
                let bfinal = if is_really_last { 0x01u8 } else { 0x00u8 };
                out.push(bfinal);
                out.extend_from_slice(&block_sz.to_le_bytes());
                out.extend_from_slice(&(!block_sz).to_le_bytes());

                if first {
                    out.push(0); // filter byte
                    first = false;
                }
                out.extend_from_slice(&remaining[..chunk_len]);
                remaining = &remaining[chunk_len..];
            }
        }
    }

    out
}

/// Compute Adler32 checksum over the filtered data (filter_byte + row for each row).
fn adler32_filtered(rgba: &[u8], width: usize, height: usize) -> u32 {
    let row_bytes = width * 4;
    let mut a: u32 = 1;
    let mut b: u32 = 0;

    for y in 0..height {
        // Filter byte
        a %= 65521;
        b = (b + a) % 65521;

        // Row pixels
        let row_start = y * row_bytes;
        for i in 0..row_bytes {
            a = (a + rgba[row_start + i] as u32) % 65521;
            b = (b + a) % 65521;
        }
    }

    (b << 16) | a
}

fn write_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend_from_slice(data);

    // CRC32 over type + data
    let crc = crc32(chunk_type, data);
    png.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in chunk_type.iter().chain(data.iter()) {
        let index = ((crc ^ byte as u32) & 0xFF) as usize;
        crc = CRC_TABLE[index] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

static CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            if c & 1 != 0 {
                c = 0xEDB88320 ^ (c >> 1);
            } else {
                c >>= 1;
            }
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};
