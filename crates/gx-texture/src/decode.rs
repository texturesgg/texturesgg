//! GX texel and palette decoders.
//!
//! All formats decode to RGBA8 (4 bytes per pixel, in R, G, B, A order).
//! Channel expansion and CMPR blending follow Dolphin's `TextureDecoder_Generic.cpp`
//! (`Convert*To8`, `DecodeDXTBlock`), which the encoders invert.

/// Decode GX texel data to row-major RGBA8 (`width * height * 4` bytes).
///
/// CI4 and CI8 need their decoded `palette` (see [`decode_palette`]); other
/// formats ignore it. Returns `None` for an unsupported format, a missing
/// palette, or `raw` shorter than [`image_data_size`].
pub fn decode_image(
    raw: &[u8],
    width: u16,
    height: u16,
    format: u32,
    palette: Option<&[[u8; 4]]>,
) -> Option<Vec<u8>> {
    let (w, h) = (usize::from(width), usize::from(height));
    w.checked_mul(h)?.checked_mul(4)?;
    let raw = raw.get(..image_data_size(format, width, height)?)?;
    decode_unchecked(raw, w, h, format, palette)
}

/// [`decode_image`] once `raw` is known to hold the whole image.
pub(crate) fn decode_unchecked(
    raw: &[u8],
    width: usize,
    height: usize,
    format: u32,
    palette: Option<&[[u8; 4]]>,
) -> Option<Vec<u8>> {
    Some(match format {
        8 => decode_ci4(raw, palette?, width, height),
        9 => decode_ci8(raw, palette?, width, height),
        0 => decode_i4(raw, width, height),
        1 => decode_i8(raw, width, height),
        2 => decode_ia4(raw, width, height),
        3 => decode_ia8(raw, width, height),
        4 => decode_rgb565(raw, width, height),
        5 => decode_rgb5a3(raw, width, height),
        6 => decode_rgba8(raw, width, height),
        14 => decode_cmp(raw, width, height),
        _ => return None,
    })
}

/// The short name of a `GXTexFmt` this codec handles ("CMPR", "CI8"), or
/// `None` for one it doesn't.
pub fn format_name(format: u32) -> Option<&'static str> {
    Some(match format {
        0 => "I4",
        1 => "I8",
        2 => "IA4",
        3 => "IA8",
        4 => "RGB565",
        5 => "RGB5A3",
        6 => "RGBA8",
        8 => "CI4",
        9 => "CI8",
        14 => "CMPR",
        _ => return None,
    })
}

/// Bytes of texel data a `width` x `height` image of `format` occupies,
/// including the padding of partial edge tiles, or `None` for a format this
/// codec does not handle.
pub fn image_data_size(format: u32, width: u16, height: u16) -> Option<usize> {
    // Tile dimensions per format (width, height)
    let (bw, bh) = match format {
        0 => (8, 8),  // I4
        1 => (8, 4),  // I8
        2 => (8, 4),  // IA4
        3 => (4, 4),  // IA8
        4 => (4, 4),  // RGB565
        5 => (4, 4),  // RGB5A3
        6 => (4, 4),  // RGBA8
        8 => (8, 8),  // CI4
        9 => (8, 4),  // CI8
        14 => (8, 8), // CMP
        _ => return None,
    };
    let w = usize::from(width).div_ceil(bw) * bw;
    let h = usize::from(height).div_ceil(bh) * bh;
    let size = w.checked_mul(h)?;
    match format {
        0 | 8 | 14 => Some(size / 2), // 4bpp
        1 | 2 | 9 => Some(size),      // 8bpp
        3..=5 => size.checked_mul(2), // 16bpp
        _ => size.checked_mul(4),     // RGBA8, 32bpp
    }
}

/// Decode `color_count` big-endian TLUT entries of `format` to RGBA8, or
/// `None` when `raw` is too short.
pub fn decode_palette(raw: &[u8], format: u32, color_count: u16) -> Option<Vec<[u8; 4]>> {
    let raw = raw.get(..usize::from(color_count) * 2)?;
    Some(
        raw.as_chunks::<2>()
            .0
            .iter()
            .map(|&entry| {
                let pixel = u16::from_be_bytes(entry);
                match format {
                    // IA8: alpha in the high byte.
                    0 => {
                        let intensity = (pixel & 0xFF) as u8;
                        [intensity, intensity, intensity, (pixel >> 8) as u8]
                    }
                    1 => rgb565(pixel),
                    // RGB5A3
                    _ => {
                        let (r, g, b, a) = decode_rgb5a3_pixel(pixel);
                        [r, g, b, a]
                    }
                }
            })
            .collect(),
    )
}

// --- Format decoders ---

fn decode_i4(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            for y1 in y..y + 8 {
                for x1 in (x..x + 8).step_by(2) {
                    if inp >= data.len() {
                        return output;
                    }
                    let pixel = data[inp];
                    inp += 1;

                    if y1 < height && x1 < width {
                        let i = expand4(pixel >> 4);
                        let idx = (y1 * width + x1) * 4;
                        output[idx] = i;
                        output[idx + 1] = i;
                        output[idx + 2] = i;
                        output[idx + 3] = 255;
                    }
                    if y1 < height && x1 + 1 < width {
                        let i = expand4(pixel & 0x0F);
                        let idx = (y1 * width + x1 + 1) * 4;
                        output[idx] = i;
                        output[idx + 1] = i;
                        output[idx + 2] = i;
                        output[idx + 3] = 255;
                    }
                }
            }
        }
    }
    output
}

fn decode_i8(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(8) {
            for y1 in y..y + 4 {
                for x1 in x..x + 8 {
                    if inp >= data.len() {
                        return output;
                    }
                    let pixel = data[inp];
                    inp += 1;

                    if y1 < height && x1 < width {
                        let idx = (y1 * width + x1) * 4;
                        output[idx] = pixel;
                        output[idx + 1] = pixel;
                        output[idx + 2] = pixel;
                        output[idx + 3] = 255;
                    }
                }
            }
        }
    }
    output
}

fn decode_ia4(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(8) {
            for y1 in y..y + 4 {
                for x1 in x..x + 8 {
                    if inp >= data.len() {
                        return output;
                    }
                    let pixel = data[inp];
                    inp += 1;

                    if y1 < height && x1 < width {
                        let i = expand4(pixel & 0x0F);
                        let a = expand4(pixel >> 4);
                        let idx = (y1 * width + x1) * 4;
                        output[idx] = i;
                        output[idx + 1] = i;
                        output[idx + 2] = i;
                        output[idx + 3] = a;
                    }
                }
            }
        }
    }
    output
}

fn decode_ia8(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            for y1 in y..y + 4 {
                for x1 in x..x + 4 {
                    if inp * 2 + 1 >= data.len() {
                        return output;
                    }
                    let pixel = u16::from_be_bytes([data[inp * 2], data[inp * 2 + 1]]);
                    inp += 1;

                    if y1 < height && x1 < width {
                        let a = (pixel >> 8) as u8;
                        let i = (pixel & 0xFF) as u8;
                        let idx = (y1 * width + x1) * 4;
                        output[idx] = i;
                        output[idx + 1] = i;
                        output[idx + 2] = i;
                        output[idx + 3] = a;
                    }
                }
            }
        }
    }
    output
}

fn decode_rgb565(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            for y1 in y..y + 4 {
                for x1 in x..x + 4 {
                    if inp * 2 + 1 >= data.len() {
                        return output;
                    }
                    let pixel = u16::from_be_bytes([data[inp * 2], data[inp * 2 + 1]]);
                    inp += 1;

                    if y1 < height && x1 < width {
                        let idx = (y1 * width + x1) * 4;
                        output[idx..idx + 4].copy_from_slice(&rgb565(pixel));
                    }
                }
            }
        }
    }
    output
}

pub(crate) fn decode_rgb5a3_pixel(pixel: u16) -> (u8, u8, u8, u8) {
    if pixel & (1 << 15) != 0 {
        // RGB555: 1RRRRRGGGGGBBBBB
        let r = expand5(((pixel >> 10) & 0x1F) as u8);
        let g = expand5(((pixel >> 5) & 0x1F) as u8);
        let b = expand5((pixel & 0x1F) as u8);
        (r, g, b, 255)
    } else {
        // ARGB3444: 0AAARRRRGGGGBBBB
        let a = expand3(((pixel >> 12) & 0x07) as u8);
        let r = expand4(((pixel >> 8) & 0x0F) as u8);
        let g = expand4(((pixel >> 4) & 0x0F) as u8);
        let b = expand4((pixel & 0x0F) as u8);
        (r, g, b, a)
    }
}

/// GX bit replication, as Dolphin's `Convert3To8`.
pub(crate) fn expand3(v: u8) -> u8 {
    (v << 5) | (v << 2) | (v >> 1)
}

pub(crate) fn expand4(v: u8) -> u8 {
    (v << 4) | v
}

pub(crate) fn expand5(v: u8) -> u8 {
    (v << 3) | (v >> 2)
}

pub(crate) fn expand6(v: u8) -> u8 {
    (v << 2) | (v >> 4)
}

/// The four colors a CMPR sub-block's selectors pick from, given its two RGB565
/// endpoints as stored. `c0 > c1` selects the opaque mode, otherwise the third
/// color is the average and the fourth is that average at zero alpha.
pub(crate) fn cmpr_palette(c0: u16, c1: u16) -> [[u8; 4]; 4] {
    let (color0, color1) = (rgb565(c0), rgb565(c1));
    let blend = |weight0: u32, weight1: u32, shift: u32| -> [u8; 4] {
        let mut out = [255; 4];
        for ((out, &value0), &value1) in out[..3].iter_mut().zip(&color0).zip(&color1) {
            *out = ((weight0 * u32::from(value0) + weight1 * u32::from(value1)) >> shift) as u8;
        }
        out
    };
    if c0 > c1 {
        // GX blends 5/8 + 3/8 rather than DXT1's thirds (Dolphin `DXTBlend`).
        [color0, color1, blend(5, 3, 3), blend(3, 5, 3)]
    } else {
        // Unlike DXT1's transparent black, keep the average's color.
        let average = blend(1, 1, 1);
        [
            color0,
            color1,
            average,
            [average[0], average[1], average[2], 0],
        ]
    }
}

/// RGB565: RRRRRGGGGGGBBBBB (R in high bits).
fn rgb565(pixel: u16) -> [u8; 4] {
    [
        expand5(((pixel >> 11) & 0x1F) as u8),
        expand6(((pixel >> 5) & 0x3F) as u8),
        expand5((pixel & 0x1F) as u8),
        255,
    ]
}

fn decode_rgb5a3(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            for y1 in y..y + 4 {
                for x1 in x..x + 4 {
                    if inp * 2 + 1 >= data.len() {
                        return output;
                    }
                    let pixel = u16::from_be_bytes([data[inp * 2], data[inp * 2 + 1]]);
                    inp += 1;

                    if y1 < height && x1 < width {
                        let (r, g, b, a) = decode_rgb5a3_pixel(pixel);
                        let idx = (y1 * width + x1) * 4;
                        output[idx] = r;
                        output[idx + 1] = g;
                        output[idx + 2] = b;
                        output[idx + 3] = a;
                    }
                }
            }
        }
    }
    output
}

fn decode_rgba8(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            // RGBA8 tiles: first 32 bytes are AR, next 32 are GB
            for k in 0..2 {
                for y1 in y..y + 4 {
                    for x1 in x..x + 4 {
                        if inp * 2 + 1 >= data.len() {
                            return output;
                        }
                        let pixel = u16::from_be_bytes([data[inp * 2], data[inp * 2 + 1]]);
                        inp += 1;

                        if y1 >= height || x1 >= width {
                            continue;
                        }

                        let idx = (y1 * width + x1) * 4;
                        if k == 0 {
                            let a = (pixel >> 8) as u8;
                            let r = (pixel & 0xFF) as u8;
                            output[idx] = r;
                            output[idx + 3] = a;
                        } else {
                            let g = (pixel >> 8) as u8;
                            let b = (pixel & 0xFF) as u8;
                            output[idx + 1] = g;
                            output[idx + 2] = b;
                        }
                    }
                }
            }
        }
    }
    output
}

fn decode_ci4(data: &[u8], palette: &[[u8; 4]], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            for y1 in y..y + 8 {
                for x1 in (x..x + 8).step_by(2) {
                    if inp >= data.len() {
                        return output;
                    }
                    let pixel = data[inp];
                    inp += 1;

                    if y1 < height && x1 < width {
                        let ci = (pixel >> 4) as usize;
                        if ci < palette.len() {
                            let idx = (y1 * width + x1) * 4;
                            output[idx..idx + 4].copy_from_slice(&palette[ci]);
                        }
                    }
                    if y1 < height && x1 + 1 < width {
                        let ci = (pixel & 0x0F) as usize;
                        if ci < palette.len() {
                            let idx = (y1 * width + x1 + 1) * 4;
                            output[idx..idx + 4].copy_from_slice(&palette[ci]);
                        }
                    }
                }
            }
        }
    }
    output
}

fn decode_ci8(data: &[u8], palette: &[[u8; 4]], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let mut inp = 0;

    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(8) {
            for y1 in y..y + 4 {
                for x1 in x..x + 8 {
                    if inp >= data.len() {
                        return output;
                    }
                    let pixel = data[inp] as usize;
                    inp += 1;

                    if y1 < height && x1 < width && pixel < palette.len() {
                        let idx = (y1 * width + x1) * 4;
                        output[idx..idx + 4].copy_from_slice(&palette[pixel]);
                    }
                }
            }
        }
    }
    output
}

fn decode_cmp(data: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; width * height * 4];
    let ww = if !width.is_multiple_of(8) {
        width + 8 - (width % 8)
    } else {
        width
    };

    for y in 0..height {
        for x in 0..width {
            let x0 = x & 0x03;
            let x1 = (x >> 2) & 0x01;
            let x2 = x >> 3;
            let y0 = y & 0x03;
            let y1 = (y >> 2) & 0x01;
            let y2 = y >> 3;

            let off = (8 * x1) + (16 * y1) + (32 * x2) + (4 * ww * y2);

            if off + 7 >= data.len() {
                continue;
            }

            let c0_raw = ((data[off] as u16) << 8) | data[off + 1] as u16;
            let c1_raw = ((data[off + 2] as u16) << 8) | data[off + 3] as u16;

            let colors = cmpr_palette(c0_raw, c1_raw);

            let pixel_bits =
                u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]]);
            let ix = x0 + 4 * y0;
            let color_idx = ((pixel_bits >> (30 - 2 * ix)) & 0x03) as usize;

            let idx = (y * width + x) * 4;
            output[idx..idx + 4].copy_from_slice(&colors[color_idx]);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{decode_image, decode_palette, image_data_size};

    #[test]
    fn decoding_checks_its_input() {
        assert!(decode_image(&[0; 32], 8, 8, 0, None).is_some());
        assert!(decode_image(&[0; 31], 8, 8, 0, None).is_none());
        assert!(decode_image(&[0; 64], 8, 8, 7, None).is_none());
        assert!(decode_image(&[0; 32], 8, 8, 8, None).is_none());
        assert_eq!(image_data_size(10, 8, 8), None);
        assert_eq!(image_data_size(14, 12, 4), Some(64));
        assert_eq!(decode_palette(&[0; 3], 1, 2), None);
        assert_eq!(
            decode_palette(&[0x80, 0x40, 0xF8, 0x00], 0, 1),
            Some(vec![[0x40, 0x40, 0x40, 0x80]])
        );
    }

    #[test]
    fn rgb5a3_expands_channels_by_bit_replication() {
        // One 4x4 tile: an ARGB3444 texel with alpha 2 and nibbles 1, 2, 3, then
        // an RGB555 texel with channels 1, 16, 31. Dolphin gives 73 (not 72) and 132.
        let mut data = vec![0; 32];
        data[0..2].copy_from_slice(&0x2123u16.to_be_bytes());
        data[2..4].copy_from_slice(&(0x8000u16 | (1 << 10) | (16 << 5) | 31).to_be_bytes());
        let rgba = decode_image(&data, 4, 4, 5, None).unwrap();
        assert_eq!(&rgba[0..4], &[0x11, 0x22, 0x33, 73]);
        assert_eq!(&rgba[4..8], &[8, 132, 255, 255]);
    }

    #[test]
    fn cmpr_blends_as_the_gamecube_does() {
        // 8x8 CMPR: four DXT blocks, only the first is inspected.
        let mut data = vec![0; 32];
        // Opaque mode (c0 > c1): red 31 and red 0; texels use indices 0, 1, 2, 3.
        data[0..2].copy_from_slice(&0xF800u16.to_be_bytes());
        data[2..4].copy_from_slice(&0x0000u16.to_be_bytes());
        data[4] = 0b00_01_10_11;
        let rgba = decode_image(&data.clone(), 8, 8, 14, None).unwrap();
        let reds: Vec<u8> = rgba[0..16].chunks(4).map(|texel| texel[0]).collect();
        assert_eq!(reds, [255, 0, 159, 95]);

        // Transparent mode (c0 <= c1): index 3 is the average with zero alpha.
        data[0..2].copy_from_slice(&0x0000u16.to_be_bytes());
        data[2..4].copy_from_slice(&0xF800u16.to_be_bytes());
        let rgba = decode_image(&data, 8, 8, 14, None).unwrap();
        assert_eq!(&rgba[8..12], &[127, 0, 0, 255]);
        assert_eq!(&rgba[12..16], &[127, 0, 0, 0]);
    }
}
