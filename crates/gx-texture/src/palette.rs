//! Building a CI4 or CI8 palette for an image.
//!
//! Every color is first rounded to what the palette format stores (IA8,
//! RGB565, or RGB5A3), so palette entries are exactly what the GPU will
//! show. An image with no more distinct stored colors than the palette has
//! entries gets them all, most used first, and encodes with no loss beyond
//! the format's rounding. A busier image is reduced by median cut, splitting
//! the box of colors with the widest weighted spread, then refined by a few
//! rounds of k-means, which moves each entry to the mean of the colors
//! nearest it. Distances are squared RGBA, as the CI encoder's nearest-entry
//! search uses.

use crate::decode::decode_palette_entry;
use crate::encode::{luma, rgb5a3, rgb565};
use crate::format::PaletteFormat;
use std::collections::HashMap;

/// k-means rounds after median cut; later rounds rarely move an entry.
const REFINE_ROUNDS: usize = 4;

/// The most entries a GX palette holds (`GX_TF_C14X2`'s 14-bit index).
pub const MAX_PALETTE_ENTRIES: usize = 1 << 14;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum PaletteError {
    #[error("a palette needs at least one entry")]
    Empty,
    #[error("a palette of {count} entries is over the {MAX_PALETTE_ENTRIES} GX can index")]
    TooManyEntries { count: usize },
    #[error("an image with no pixels has no colors to build a palette from")]
    NoPixels,
}

/// One palette entry in `format`, as its stored 16-bit value.
fn encode_entry(color: [u8; 4], format: PaletteFormat) -> u16 {
    match format {
        // IA8: alpha in the high byte, intensity in the low.
        PaletteFormat::Ia8 => u16::from(color[3]) << 8 | u16::from(luma(color)),
        PaletteFormat::Rgb565 => rgb565(color),
        PaletteFormat::Rgb5a3 => rgb5a3(color),
    }
}

/// `color` as `format` stores and decodes it.
fn stored(color: [u8; 4], format: PaletteFormat) -> [u8; 4] {
    decode_palette_entry(encode_entry(color, format), format)
}

fn distance(a: [u8; 4], b: [u8; 4]) -> u32 {
    a.iter()
        .zip(&b)
        .map(|(&a, &b)| u32::from(a.abs_diff(b)).pow(2))
        .sum()
}

/// A palette of at most `count` entries for row-major RGBA8 `rgba`, stored
/// in `format`: the decoded colors, and the TLUT bytes (`count` big-endian
/// entries; unused ones repeat the last color).
pub fn build_palette(
    rgba: &[u8],
    format: PaletteFormat,
    count: usize,
) -> Result<(Vec<[u8; 4]>, Vec<u8>), PaletteError> {
    if count == 0 {
        return Err(PaletteError::Empty);
    }
    if count > MAX_PALETTE_ENTRIES {
        return Err(PaletteError::TooManyEntries { count });
    }
    let texels = rgba.as_chunks::<4>().0;
    if texels.is_empty() {
        return Err(PaletteError::NoPixels);
    }
    let mut histogram: HashMap<[u8; 4], u64> = HashMap::new();
    for &texel in texels {
        *histogram.entry(stored(texel, format)).or_default() += 1;
    }
    let mut colors: Vec<([u8; 4], u64)> = histogram.into_iter().collect();
    // Most used first; ties by color, so the palette is deterministic.
    colors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let palette = if colors.len() <= count {
        colors.iter().map(|&(color, _)| color).collect()
    } else {
        refine(&colors, median_cut(&colors, count, format), format)
    };
    let mut tlut = Vec::with_capacity(count * 2);
    for index in 0..count {
        let color = palette[index.min(palette.len().saturating_sub(1))];
        tlut.extend(encode_entry(color, format).to_be_bytes());
    }
    Ok((palette, tlut))
}

/// Split the weighted colors into `count` boxes and take each box's mean.
fn median_cut(colors: &[([u8; 4], u64)], count: usize, format: PaletteFormat) -> Vec<[u8; 4]> {
    let mut boxes: Vec<Vec<([u8; 4], u64)>> = vec![colors.to_vec()];
    while boxes.len() < count {
        // The box whose widest channel spreads furthest, weighted by how many
        // texels it holds, gains the most from a split.
        let Some((index, channel)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, colors)| colors.len() > 1)
            .map(|(index, colors)| {
                let (channel, range) = (0..4)
                    .map(|channel| {
                        let values = colors.iter().map(|(color, _)| color[channel]);
                        let range = values.clone().max().unwrap_or(0) - values.min().unwrap_or(0);
                        (channel, range)
                    })
                    .max_by_key(|&(_, range)| range)
                    .expect("four channels");
                let weight: u64 = colors.iter().map(|&(_, weight)| weight).sum();
                (index, channel, u64::from(range) * weight)
            })
            .filter(|&(_, _, score)| score > 0)
            .max_by_key(|&(_, _, score)| score)
            .map(|(index, channel, _)| (index, channel))
        else {
            break;
        };
        let mut splitting = boxes.swap_remove(index);
        splitting.sort_by_key(|(color, _)| color[channel]);
        // Split at the weighted median, keeping both halves non-empty.
        let total: u64 = splitting.iter().map(|&(_, weight)| weight).sum();
        let mut running = 0;
        let mut at = 1;
        for (position, &(_, weight)) in splitting.iter().enumerate() {
            running += weight;
            if running * 2 >= total {
                at = (position + 1).clamp(1, splitting.len() - 1);
                break;
            }
        }
        let upper = splitting.split_off(at);
        boxes.push(splitting);
        boxes.push(upper);
    }
    let mut palette: Vec<[u8; 4]> = boxes.iter().map(|colors| mean(colors, format)).collect();
    palette.sort_unstable();
    palette.dedup();
    palette
}

/// The weighted mean of `colors`, as `format` stores it.
fn mean(colors: &[([u8; 4], u64)], format: PaletteFormat) -> [u8; 4] {
    let total: u64 = colors.iter().map(|&(_, weight)| weight).sum::<u64>().max(1);
    let mut sums = [0u64; 4];
    for &(color, weight) in colors {
        for (sum, &channel) in sums.iter_mut().zip(&color) {
            *sum += u64::from(channel) * weight;
        }
    }
    stored(sums.map(|sum| ((sum + total / 2) / total) as u8), format)
}

/// Move each entry to the mean of the colors nearest it, a few times.
fn refine(
    colors: &[([u8; 4], u64)],
    mut palette: Vec<[u8; 4]>,
    format: PaletteFormat,
) -> Vec<[u8; 4]> {
    for _ in 0..REFINE_ROUNDS {
        let nearest = assign(colors, &palette);
        let mut members: Vec<Vec<([u8; 4], u64)>> = vec![Vec::new(); palette.len()];
        for (&entry, &color) in nearest.iter().zip(colors) {
            members[entry].push(color);
        }
        let moved: Vec<[u8; 4]> = members
            .iter()
            .zip(&palette)
            .map(|(members, &entry)| {
                if members.is_empty() {
                    entry
                } else {
                    mean(members, format)
                }
            })
            .collect();
        if moved == palette {
            break;
        }
        palette = moved;
    }
    palette.sort_unstable();
    palette.dedup();
    palette
}

/// The nearest palette entry to each color. Colors are independent, so
/// native builds use every core.
fn assign(colors: &[([u8; 4], u64)], palette: &[[u8; 4]]) -> Vec<usize> {
    let nearest = |&(color, _): &([u8; 4], u64)| {
        palette
            .iter()
            .enumerate()
            .min_by_key(|&(_, &entry)| distance(entry, color))
            .map_or(0, |(index, _)| index)
    };
    #[cfg(not(target_family = "wasm"))]
    {
        use rayon::prelude::*;
        colors.par_iter().map(nearest).collect()
    }
    #[cfg(target_family = "wasm")]
    {
        colors.iter().map(nearest).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{PaletteError, build_palette, stored};
    use crate::{PaletteFormat, TextureFormat, decode_image, decode_palette, encode_texture};

    #[test]
    fn an_image_with_no_pixels_is_an_error_not_an_empty_palette() {
        // A 0×N texture can still name a palette with entries to fill.
        for rgba in [&[][..], &[1, 2, 3][..]] {
            assert!(matches!(
                build_palette(rgba, PaletteFormat::Rgb565, 16),
                Err(PaletteError::NoPixels)
            ));
        }
    }

    /// Encode `rgba` as CI8 through a palette built for it and decode it back.
    fn round_trip(
        rgba: &[u8],
        width: u16,
        height: u16,
        format: PaletteFormat,
        count: usize,
    ) -> Vec<u8> {
        let (colors, tlut) = build_palette(rgba, format, count).unwrap();
        assert_eq!(tlut.len(), count * 2);
        assert_eq!(
            decode_palette(&tlut, format, count as u16).unwrap()[..colors.len()],
            colors[..]
        );
        let data = encode_texture(rgba, width, height, TextureFormat::Ci8, Some(&colors)).unwrap();
        decode_image(&data, width, height, TextureFormat::Ci8, Some(&colors)).unwrap()
    }

    #[test]
    fn few_colors_are_kept_exactly() {
        // Four RGB565-representable colors, one translucent RGB5A3 one.
        let colors = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [0, 0, 0, 255],
        ];
        let rgba: Vec<u8> = (0..64).flat_map(|texel| colors[texel % 4]).collect();
        assert_eq!(round_trip(&rgba, 8, 8, PaletteFormat::Rgb565, 256), rgba);
        let mut translucent = rgba.clone();
        translucent[3] = 0x20;
        let stored_texel = stored([255, 0, 0, 0x20], PaletteFormat::Rgb5a3);
        let decoded = round_trip(&translucent, 8, 8, PaletteFormat::Rgb5a3, 16);
        assert_eq!(decoded[..4], stored_texel);
        assert_eq!(decoded[4..], translucent[4..]);
    }

    #[test]
    fn a_busy_image_fits_its_palette_closely() {
        // A 64x64 gradient has ~4k colors; 256 entries should land within a
        // few steps of each channel on average.
        let (width, height) = (64u16, 64u16);
        let rgba: Vec<u8> = (0..usize::from(width) * usize::from(height))
            .flat_map(|texel| {
                let (x, y) = ((texel % 64) as u8, (texel / 64) as u8);
                [x * 4, y * 4, 255 - x * 2, 255]
            })
            .collect();
        let decoded = round_trip(&rgba, width, height, PaletteFormat::Rgb565, 256);
        let error: f64 = decoded
            .iter()
            .zip(&rgba)
            .map(|(&a, &b)| f64::from(a.abs_diff(b)))
            .sum::<f64>()
            / rgba.len() as f64;
        assert!(error < 3.0, "mean channel error {error}");
        // Sixteen entries (CI4's limit) still get the gist.
        let decoded = round_trip(&rgba, width, height, PaletteFormat::Rgb565, 16);
        let error: f64 = decoded
            .iter()
            .zip(&rgba)
            .map(|(&a, &b)| f64::from(a.abs_diff(b)))
            .sum::<f64>()
            / rgba.len() as f64;
        assert!(error < 12.0, "mean channel error {error}");
    }

    #[test]
    fn bad_sizes_are_errors() {
        let format = PaletteFormat::Rgb565;
        assert_eq!(build_palette(&[0; 4], format, 0), Err(PaletteError::Empty));
        assert_eq!(
            build_palette(&[0; 4], format, usize::MAX),
            Err(PaletteError::TooManyEntries { count: usize::MAX })
        );
    }
}
