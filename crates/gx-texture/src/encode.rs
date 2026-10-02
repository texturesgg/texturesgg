//! GX texture format encoders for every format Melee's costumes use.
//!
//! Each encoder inverts the matching decoder in [`crate::decode`]. For the
//! direct formats a channel quantizes to the code whose expansion is nearest,
//! so an image the decoder produced re-encodes to codes that decode to the same
//! pixels. CI4 and CI8 keep their existing palette and pick the nearest entry.
//! CMPR is lossy; its encoder searches endpoints against the exact GX palette.

use crate::decode::{
    cmpr_palette, decode_rgb5a3_pixel, decode_unchecked, expand3, expand4, expand5, expand6,
    image_data_size,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextureEncodeError {
    #[error("GX texture format {0} has no encoder")]
    UnsupportedFormat(u32),
    #[error("a {width}x{height} image needs {expected} bytes of RGBA8, got {actual}")]
    PixelLength {
        width: u16,
        height: u16,
        expected: usize,
        actual: usize,
    },
    #[error("the original texture data is {actual} bytes, not {expected}")]
    OriginalLength { expected: usize, actual: usize },
    #[error("paletted format {0} needs a non-empty palette")]
    MissingPalette(u32),
}

/// Texture data re-encoded over an original, with how much of it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureOverwrite {
    pub bytes: Vec<u8>,
    /// Storage blocks in the image, including partial edge blocks.
    pub blocks: usize,
    /// Blocks whose bytes changed: a block is re-encoded when its visible
    /// pixels differ from what's asked, and counts only if that changes it.
    pub changed_blocks: usize,
}

/// A half-open rectangle of texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexelRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl TexelRect {
    fn intersects(&self, (x, y): (usize, usize), (width, height): (usize, usize)) -> bool {
        let (left, top) = (usize::from(self.x), usize::from(self.y));
        let (right, bottom) = (
            left + usize::from(self.width),
            top + usize::from(self.height),
        );
        x < right && left < x + width && y < bottom && top < y + height
    }
}

/// Re-encode `rgba` over `original`, a texture's current GX bytes, keeping the
/// original bytes of every storage block whose visible pixels decode unchanged.
///
/// Unedited regions therefore stay byte-identical even where the encoder would
/// pick a different but equivalent code, and block padding is never touched
/// in an unchanged block.
///
/// `dirty` limits the edit to the blocks it overlaps: every other block keeps
/// its original bytes without being compared. An editor that paints at full
/// precision needs this, because a lossy format (CMPR) never decodes back to
/// the painted pixels, so comparison alone would re-encode every block a
/// stroke ever touched. `None` compares the whole image.
pub fn encode_texture_over(
    original: &[u8],
    rgba: &[u8],
    width: u16,
    height: u16,
    format: u32,
    palette: Option<&[[u8; 4]]>,
    dirty: Option<TexelRect>,
) -> Result<TextureOverwrite, TextureEncodeError> {
    let encoder = BlockEncoder::new(rgba, width, height, format, palette)?;
    if original.len() != encoder.size {
        return Err(TextureEncodeError::OriginalLength {
            expected: encoder.size,
            actual: original.len(),
        });
    }
    let (w, h) = (encoder.width, encoder.height);
    let (block_width, block_height) = storage_block_size(format);
    let before =
        decode_unchecked(original, w, h, format, palette).expect("encodable formats decode");

    let origins = storage_blocks(format, w, h);
    let blocks = origins.len();
    let block_bytes = encoder.size.checked_div(blocks).unwrap_or(0);
    let mut bytes = vec![0; encoder.size];
    let changed_blocks = each_block(&origins, &mut bytes, |block, origin @ (x0, y0), out| {
        let outside =
            dirty.is_some_and(|dirty| !dirty.intersects((x0, y0), (block_width, block_height)));
        let unchanged = outside
            || x0 >= w
            || (y0..(y0 + block_height).min(h)).all(|y| {
                let row = (y * w + x0) * 4..(y * w + (x0 + block_width).min(w)) * 4;
                before[row.clone()] == rgba[row]
            });
        let original = &original[block * block_bytes..(block + 1) * block_bytes];
        if unchanged {
            out.copy_from_slice(original);
            return false;
        }
        encoder.encode_block_into(origin, out);
        // A lossy block asked for again encodes to the same bytes: that's no
        // change, so re-importing an unedited file changes nothing.
        out != original
    });
    debug_assert_eq!(bytes.len(), encoder.size);
    Ok(TextureOverwrite {
        bytes,
        blocks,
        changed_blocks,
    })
}

/// The texel dimensions of one storage tile of an encodable format.
fn tile_size(format: u32) -> Option<(usize, usize)> {
    match format {
        0 | 8 | 14 => Some((8, 8)),
        1 | 2 | 9 => Some((8, 4)),
        3..=6 => Some((4, 4)),
        _ => None,
    }
}

/// The texel dimensions of the smallest independently encoded block: the tile,
/// except CMPR, whose 8x8 tile holds four 4x4 sub-blocks.
fn storage_block_size(format: u32) -> (usize, usize) {
    match format {
        14 => (4, 4),
        _ => tile_size(format).expect("an encodable format"),
    }
}

/// Origins of the independently encoded blocks, in storage order. CMPR stores
/// each tile's sub-blocks top left, top right, bottom left, bottom right; a
/// sub-block wholly in padding still occupies its eight bytes.
fn storage_blocks(format: u32, width: usize, height: usize) -> Vec<(usize, usize)> {
    let (tile_width, tile_height) = tile_size(format).expect("an encodable format");
    let (block_width, block_height) = storage_block_size(format);
    let tiles_x = width.div_ceil(tile_width);
    (0..height.div_ceil(tile_height) * tiles_x)
        .flat_map(|tile| {
            let (tile_x, tile_y) = (tile % tiles_x * tile_width, tile / tiles_x * tile_height);
            (0..tile_height / block_height).flat_map(move |y| {
                (0..tile_width / block_width)
                    .map(move |x| (tile_x + x * block_width, tile_y + y * block_height))
            })
        })
        .collect()
}

/// Encode row-major RGBA8 pixels (R, G, B, A order) as GX texture data.
///
/// Returns exactly the bytes [`crate::decode_image`] reads for this
/// size and format. Texels in block padding beyond `width` and `height` are
/// never sampled and encode as zero. Intensity formats take BT.601 luma, which
/// is exact for gray input; I4 and I8 drop alpha, and RGB565 ignores it.
///
/// CI4 and CI8 need the image's decoded `palette` (see
/// [`crate::decode_palette`]) and keep it: each texel takes the index
/// of the nearest color by RGBA distance, the lowest index on a tie. CI4 can
/// reach only the first 16 entries. Other formats ignore `palette`.
pub fn encode_texture(
    rgba: &[u8],
    width: u16,
    height: u16,
    format: u32,
    palette: Option<&[[u8; 4]]>,
) -> Result<Vec<u8>, TextureEncodeError> {
    let encoder = BlockEncoder::new(rgba, width, height, format, palette)?;
    let origins = storage_blocks(format, encoder.width, encoder.height);
    let mut out = vec![0; encoder.size];
    each_block(&origins, &mut out, |_, origin, block| {
        encoder.encode_block_into(origin, block);
        true
    });
    Ok(out)
}

/// Run `block` on each storage block with its bytes in `out`, returning how
/// many calls reported a change. Blocks are independent, so native builds use
/// every core; a browser build runs them in order.
fn each_block(
    origins: &[(usize, usize)],
    out: &mut [u8],
    block: impl Fn(usize, (usize, usize), &mut [u8]) -> bool + Sync,
) -> usize {
    let Some(block_bytes) = out
        .len()
        .checked_div(origins.len())
        .filter(|&bytes| bytes > 0)
    else {
        return 0;
    };
    #[cfg(not(target_family = "wasm"))]
    {
        use rayon::prelude::*;
        out.par_chunks_mut(block_bytes)
            .zip(origins.par_iter())
            .enumerate()
            .map(|(index, (bytes, &origin))| block(index, origin, bytes))
            .filter(|&changed| changed)
            .count()
    }
    #[cfg(target_family = "wasm")]
    {
        out.chunks_mut(block_bytes)
            .zip(origins)
            .enumerate()
            .map(|(index, (bytes, &origin))| block(index, origin, bytes))
            .filter(|&changed| changed)
            .count()
    }
}

/// Validated input to encode, one storage block at a time.
struct BlockEncoder<'a> {
    rgba: &'a [u8],
    width: usize,
    height: usize,
    format: u32,
    /// The palette entries CI4 or CI8 can index; empty for other formats.
    palette: &'a [[u8; 4]],
    /// Encoded byte length of the whole image.
    size: usize,
}

impl<'a> BlockEncoder<'a> {
    fn new(
        rgba: &'a [u8],
        width: u16,
        height: u16,
        format: u32,
        palette: Option<&'a [[u8; 4]]>,
    ) -> Result<Self, TextureEncodeError> {
        let (w, h) = (usize::from(width), usize::from(height));
        tile_size(format).ok_or(TextureEncodeError::UnsupportedFormat(format))?;
        let expected = w
            .checked_mul(h)
            .and_then(|texels| texels.checked_mul(4))
            .filter(|&expected| expected == rgba.len())
            .ok_or(TextureEncodeError::PixelLength {
                width,
                height,
                expected: w.saturating_mul(h).saturating_mul(4),
                actual: rgba.len(),
            })?;
        // The block sizes are the decoder's, so the size is defined whenever
        // the pixel buffer fits in memory.
        let size =
            image_data_size(format, width, height).ok_or(TextureEncodeError::PixelLength {
                width,
                height,
                expected,
                actual: rgba.len(),
            })?;
        let palette = match format {
            8 | 9 => {
                let palette = palette
                    .filter(|palette| !palette.is_empty())
                    .ok_or(TextureEncodeError::MissingPalette(format))?;
                &palette[..palette.len().min(if format == 8 { 16 } else { 256 })]
            }
            _ => &[],
        };
        Ok(Self {
            rgba,
            width: w,
            height: h,
            format,
            palette,
            size,
        })
    }

    /// The texel at (x, y), or `None` in block padding beyond the image.
    fn texel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        (x < self.width && y < self.height).then(|| {
            let i = (y * self.width + x) * 4;
            [
                self.rgba[i],
                self.rgba[i + 1],
                self.rgba[i + 2],
                self.rgba[i + 3],
            ]
        })
    }

    /// Encode the storage block at `origin` into `out`, which holds exactly
    /// one block's bytes.
    fn encode_block_into(&self, origin: (usize, usize), out: &mut [u8]) {
        let mut block = Vec::with_capacity(out.len());
        self.encode_block(origin, &mut block);
        out.copy_from_slice(&block);
    }

    /// Append the storage block at `(x0, y0)`: a tile, or a CMPR sub-block.
    /// Padding texels encode as zero (index 0 for CI4 and CI8).
    fn encode_block(&self, (x0, y0): (usize, usize), out: &mut Vec<u8>) {
        let (block_width, block_height) = storage_block_size(self.format);
        let mut texels = (0..block_height)
            .flat_map(|y| (0..block_width).map(move |x| (x0 + x, y0 + y)))
            .map(|(x, y)| self.texel(x, y));
        match self.format {
            // I4 and CI4: two texels per byte, the first in the high nibble.
            0 | 8 => {
                let nibble = |texel: Option<[u8; 4]>| match self.format {
                    0 => quantize4(luma(texel.unwrap_or_default())),
                    _ => texel.map_or(0, |texel| nearest_index(self.palette, texel)),
                };
                while let (Some(first), Some(second)) = (texels.next(), texels.next()) {
                    out.push(nibble(first) << 4 | nibble(second));
                }
            }
            1 => out.extend(texels.map(|p| luma(p.unwrap_or_default()))),
            // IA4: alpha in the high nibble.
            2 => out.extend(texels.map(|p| {
                let p = p.unwrap_or_default();
                quantize4(p[3]) << 4 | quantize4(luma(p))
            })),
            3 => texels.for_each(|p| {
                let p = p.unwrap_or_default();
                out.extend([p[3], luma(p)]);
            }),
            4 => texels.for_each(|p| out.extend(rgb565(p.unwrap_or_default()).to_be_bytes())),
            5 => texels.for_each(|p| out.extend(rgb5a3(p.unwrap_or_default()).to_be_bytes())),
            // RGBA8: each 4x4 tile stores its 16 AR pairs, then its 16 GB pairs.
            6 => {
                let tile: Vec<[u8; 4]> = texels.map(Option::unwrap_or_default).collect();
                out.extend(tile.iter().flat_map(|p| [p[3], p[0]]));
                out.extend(tile.iter().flat_map(|p| [p[1], p[2]]));
            }
            9 => out.extend(texels.map(|p| p.map_or(0, |p| nearest_index(self.palette, p)))),
            14 => {
                let block: [Option<[u8; 4]>; 16] =
                    std::array::from_fn(|_| texels.next().expect("a 4x4 sub-block"));
                out.extend(encode_cmpr_block(&block));
            }
            _ => unreachable!("BlockEncoder::new accepted the format"),
        }
    }
}

/// BT.601 luma, rounded.
pub(crate) fn luma([r, g, b, _]: [u8; 4]) -> u8 {
    ((299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) + 500) / 1000) as u8
}

pub(crate) fn rgb565([r, g, b, _]: [u8; 4]) -> u16 {
    u16::from(quantize5(r)) << 11 | u16::from(quantize6(g)) << 5 | u16::from(quantize5(b))
}

/// RGB5A3 stores each texel as RGB555 (opaque) or ARGB3444; take whichever
/// decodes closer, preferring RGB555 on a tie.
pub(crate) fn rgb5a3(pixel: [u8; 4]) -> u16 {
    let [r, g, b, a] = pixel;
    let opaque = 0x8000
        | u16::from(quantize5(r)) << 10
        | u16::from(quantize5(g)) << 5
        | u16::from(quantize5(b));
    let translucent = u16::from(quantize3(a)) << 12
        | u16::from(quantize4(r)) << 8
        | u16::from(quantize4(g)) << 4
        | u16::from(quantize4(b));
    let error = |code: u16| {
        let (r, g, b, a) = decode_rgb5a3_pixel(code);
        [r, g, b, a]
            .iter()
            .zip(&pixel)
            .map(|(&decoded, &source)| u32::from(decoded.abs_diff(source)).pow(2))
            .sum::<u32>()
    };
    if error(translucent) < error(opaque) {
        translucent
    } else {
        opaque
    }
}

/// The palette index nearest `texel` by squared RGBA distance, lowest first.
fn nearest_index(palette: &[[u8; 4]], texel: [u8; 4]) -> u8 {
    let distance = |color: &[u8; 4]| -> u32 {
        color
            .iter()
            .zip(&texel)
            .map(|(&a, &b)| u32::from(a.abs_diff(b)).pow(2))
            .sum()
    };
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, color)| distance(color))
        .map_or(0, |(index, _)| index as u8)
}

/// Texels with alpha below this are transparent, which only CMPR's three-color
/// mode can store.
const CMPR_TRANSPARENT_BELOW: u8 = 128;

/// Encode one CMPR 4x4 sub-block; `None` marks padding outside the image.
///
/// Candidate endpoint pairs come from the block's own colors, its bounding box,
/// and the extremes along its principal axis, each tried in both modes where
/// the block allows. The best is refined by a least-squares fit to its selector
/// weights and then by single-step moves in RGB565 space. Every candidate is
/// scored with the decoder's own palette, so GX's 5/8 + 3/8 blend and the
/// endpoint order that selects the mode are exact.
fn encode_cmpr_block(texels: &[Option<[u8; 4]>; 16]) -> [u8; 8] {
    let transparent: Vec<[u8; 3]> = texels
        .iter()
        .flatten()
        .filter(|texel| texel[3] < CMPR_TRANSPARENT_BELOW)
        .map(|&[r, g, b, _]| [r, g, b])
        .collect();
    let opaque: Vec<[u8; 3]> = texels
        .iter()
        .flatten()
        .filter(|texel| texel[3] >= CMPR_TRANSPARENT_BELOW)
        .map(|&[r, g, b, _]| [r, g, b])
        .collect();

    if opaque.is_empty() {
        // Nothing visible: give hidden texels the mean of their colors, which
        // filtering can expose, as both endpoints (and so as the average).
        let color = if transparent.is_empty() {
            0
        } else {
            to_rgb565(mean(&transparent))
        };
        let (_, indices) = cmpr_error(color, color, texels);
        return cmpr_block_bytes(color, color, indices);
    }

    let mut endpoints: Vec<u16> = opaque.iter().map(|&color| to_rgb565(color)).collect();
    let mut low = [255u8; 3];
    let mut high = [0u8; 3];
    for color in &opaque {
        for channel in 0..3 {
            low[channel] = low[channel].min(color[channel]);
            high[channel] = high[channel].max(color[channel]);
        }
    }
    endpoints.extend([to_rgb565(low), to_rgb565(high)]);
    endpoints.extend(principal_extremes(&opaque).map(to_rgb565));
    endpoints.sort_unstable();
    endpoints.dedup();

    let three_color_only = !transparent.is_empty();
    let mut best = (u64::MAX, 0, 0);
    for (i, &a) in endpoints.iter().enumerate() {
        for &b in &endpoints[i..] {
            // c0 > c1 selects four colors; c0 <= c1 selects three.
            let orders = [(b, a), (a, b)];
            for (c0, c1) in orders {
                if three_color_only && c0 > c1 {
                    continue;
                }
                let (error, _) = cmpr_error(c0, c1, texels);
                if error < best.0 {
                    best = (error, c0, c1);
                }
            }
        }
    }

    let (mut error, mut c0, mut c1) = best;
    for _ in 0..4 {
        let (_, indices) = cmpr_error(c0, c1, texels);
        let Some((fit0, fit1)) = least_squares_endpoints(c0, c1, indices, texels) else {
            break;
        };
        let (fit_error, _) = cmpr_error(fit0, fit1, texels);
        if fit_error >= error {
            break;
        }
        (error, c0, c1) = (fit_error, fit0, fit1);
    }
    for _ in 0..64 {
        let step = rgb565_neighbours(c0)
            .map(|n| (n, c1))
            .chain(rgb565_neighbours(c1).map(|n| (c0, n)))
            .map(|(n0, n1)| (cmpr_error(n0, n1, texels).0, n0, n1))
            .min_by_key(|&(e, _, _)| e);
        match step {
            Some((step_error, n0, n1)) if step_error < error => {
                (error, c0, c1) = (step_error, n0, n1)
            }
            _ => break,
        }
    }

    let (_, indices) = cmpr_error(c0, c1, texels);
    cmpr_block_bytes(c0, c1, indices)
}

fn cmpr_block_bytes(c0: u16, c1: u16, indices: u32) -> [u8; 8] {
    let mut out = [0; 8];
    out[0..2].copy_from_slice(&c0.to_be_bytes());
    out[2..4].copy_from_slice(&c1.to_be_bytes());
    out[4..8].copy_from_slice(&indices.to_be_bytes());
    out
}

/// Squared RGB error of the best selector per texel, and the selectors packed
/// as the decoder reads them (texel 0 in the top two bits). A transparent texel
/// needs the zero-alpha color and an opaque one an opaque color; a block that
/// can't give it scores `u64::MAX`.
fn cmpr_error(c0: u16, c1: u16, texels: &[Option<[u8; 4]>; 16]) -> (u64, u32) {
    let palette = cmpr_palette(c0, c1);
    let mut error = 0u64;
    let mut indices = 0u32;
    for (i, texel) in texels.iter().enumerate() {
        let Some(texel) = texel else { continue };
        let want_opaque = texel[3] >= CMPR_TRANSPARENT_BELOW;
        let choice = palette
            .iter()
            .enumerate()
            .filter(|(_, color)| (color[3] == 255) == want_opaque)
            .map(|(index, color)| {
                let distance: u64 = if want_opaque {
                    (0..3)
                        .map(|c| u64::from(color[c].abs_diff(texel[c])).pow(2))
                        .sum()
                } else {
                    0
                };
                (distance, index)
            })
            .min();
        let Some((distance, index)) = choice else {
            return (u64::MAX, 0);
        };
        error += distance;
        indices |= (index as u32) << (30 - 2 * i);
    }
    (error, indices)
}

/// Endpoints that minimize squared error for fixed selectors, rounded to
/// RGB565 in the same order (and so the same mode).
fn least_squares_endpoints(
    c0: u16,
    c1: u16,
    indices: u32,
    texels: &[Option<[u8; 4]>; 16],
) -> Option<(u16, u16)> {
    let weights: [(f64, f64); 4] = if c0 > c1 {
        [(1.0, 0.0), (0.0, 1.0), (0.625, 0.375), (0.375, 0.625)]
    } else {
        [(1.0, 0.0), (0.0, 1.0), (0.5, 0.5), (0.0, 0.0)]
    };
    let (mut aa, mut ab, mut bb) = (0.0, 0.0, 0.0);
    let mut ap = [0.0f64; 3];
    let mut bp = [0.0f64; 3];
    for (i, texel) in texels.iter().enumerate() {
        let Some(texel) = texel.filter(|texel| texel[3] >= CMPR_TRANSPARENT_BELOW) else {
            continue;
        };
        let (a, b) = weights[(indices >> (30 - 2 * i) & 3) as usize];
        aa += a * a;
        ab += a * b;
        bb += b * b;
        for c in 0..3 {
            ap[c] += a * f64::from(texel[c]);
            bp[c] += b * f64::from(texel[c]);
        }
    }
    let determinant = aa * bb - ab * ab;
    if determinant.abs() < 1e-9 {
        return None;
    }
    let solve = |c: usize| {
        let e0 = (bb * ap[c] - ab * bp[c]) / determinant;
        let e1 = (aa * bp[c] - ab * ap[c]) / determinant;
        let clamp = |value: f64| value.round().clamp(0.0, 255.0) as u8;
        (clamp(e0), clamp(e1))
    };
    let [(r0, r1), (g0, g1), (b0, b1)] = [solve(0), solve(1), solve(2)];
    let (fit0, fit1) = (to_rgb565([r0, g0, b0]), to_rgb565([r1, g1, b1]));
    // Keep the mode the selectors were chosen for.
    let swapped = (fit0 > fit1) != (c0 > c1);
    Some(if swapped { (fit1, fit0) } else { (fit0, fit1) })
}

/// RGB565 codes one channel step away, clamped to each channel's range.
fn rgb565_neighbours(code: u16) -> impl Iterator<Item = u16> {
    [(11, 0x1F), (5, 0x3F), (0, 0x1F)]
        .into_iter()
        .flat_map(move |(shift, max): (u16, u16)| {
            let value = (code >> shift) & max;
            [value.checked_sub(1), (value < max).then_some(value + 1)]
                .into_iter()
                .flatten()
                .map(move |next| code & !(max << shift) | next << shift)
        })
}

fn to_rgb565([r, g, b]: [u8; 3]) -> u16 {
    rgb565([r, g, b, 255])
}

fn mean(colors: &[[u8; 3]]) -> [u8; 3] {
    std::array::from_fn(|c| {
        let sum: usize = colors.iter().map(|color| usize::from(color[c])).sum();
        ((sum + colors.len() / 2) / colors.len()) as u8
    })
}

/// The colors at either end of the block's principal axis (power iteration on
/// the color covariance), falling back to the mean for a flat block.
fn principal_extremes(colors: &[[u8; 3]]) -> [[u8; 3]; 2] {
    let center: [f64; 3] = std::array::from_fn(|c| {
        colors.iter().map(|color| f64::from(color[c])).sum::<f64>() / colors.len() as f64
    });
    let mut covariance = [[0.0f64; 3]; 3];
    for color in colors {
        let d: [f64; 3] = std::array::from_fn(|c| f64::from(color[c]) - center[c]);
        for (row, &di) in covariance.iter_mut().zip(&d) {
            for (cell, &dj) in row.iter_mut().zip(&d) {
                *cell += di * dj;
            }
        }
    }
    let mut axis = [1.0f64, 1.0, 1.0];
    for _ in 0..8 {
        let next: [f64; 3] =
            std::array::from_fn(|i| (0..3).map(|j| covariance[i][j] * axis[j]).sum());
        let length = next.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length < 1e-9 {
            let flat = mean(colors);
            return [flat, flat];
        }
        axis = next.map(|v| v / length);
    }
    let project = |color: &[u8; 3]| -> f64 {
        (0..3)
            .map(|c| (f64::from(color[c]) - center[c]) * axis[c])
            .sum()
    };
    let by_projection = |a: &&[u8; 3], b: &&[u8; 3]| project(a).total_cmp(&project(b));
    let low = colors
        .iter()
        .min_by(by_projection)
        .expect("a non-empty block");
    let high = colors
        .iter()
        .max_by(by_projection)
        .expect("a non-empty block");
    [*low, *high]
}

/// A decoder's bit-replicating channel expansion.
type Expand = fn(u8) -> u8;

/// The `bits`-wide code whose bit-replicated expansion is nearest to `value`.
/// Replication stays within one code of linear scaling, so only the rounded
/// linear guess and its neighbours can win.
fn quantize(value: u8, bits: u32, expand: Expand) -> u8 {
    let max = (1u32 << bits) - 1;
    let guess = (u32::from(value) * max + 127) / 255;
    (guess.saturating_sub(1)..=(guess + 1).min(max))
        .map(|code| code as u8)
        .min_by_key(|&code| expand(code).abs_diff(value))
        .expect("the candidate range is never empty")
}

fn quantize3(value: u8) -> u8 {
    quantize(value, 3, expand3)
}

fn quantize4(value: u8) -> u8 {
    quantize(value, 4, expand4)
}

fn quantize5(value: u8) -> u8 {
    quantize(value, 5, expand5)
}

fn quantize6(value: u8) -> u8 {
    quantize(value, 6, expand6)
}

#[cfg(test)]
mod tests {
    use super::{
        Expand, TexelRect, TextureEncodeError, encode_texture, encode_texture_over, quantize,
    };
    use crate::decode::{decode_image, expand3, expand4, expand5, expand6};

    const DIRECT_FORMATS: [u32; 7] = [0, 1, 2, 3, 4, 5, 6];

    fn decode(data: &[u8], width: u16, height: u16, format: u32) -> Vec<u8> {
        decode_image(data, width, height, format, None).expect("decodable test texture")
    }

    /// Deterministic xorshift bytes, so failures reproduce.
    fn noise(len: usize, mut state: u32) -> Vec<u8> {
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect()
    }

    #[test]
    fn quantization_picks_the_nearest_expansion() {
        let expanders: [(u32, Expand); 4] =
            [(3, expand3), (4, expand4), (5, expand5), (6, expand6)];
        for (bits, expand) in expanders {
            for value in 0..=255u8 {
                let best = (0..1u8 << bits)
                    .map(|code| expand(code).abs_diff(value))
                    .min()
                    .unwrap();
                let code = quantize(value, bits, expand);
                assert_eq!(expand(code).abs_diff(value), best, "{bits}-bit {value}");
            }
            for code in 0..1u8 << bits {
                assert_eq!(quantize(expand(code), bits, expand), code);
            }
        }
    }

    /// Every code of every direct format survives decode then encode unchanged,
    /// except opaque ARGB3444 texels that RGB555 represents exactly.
    #[test]
    fn every_texel_code_round_trips() {
        // (format, width, height): one texel per code, or two I4 codes per byte.
        let cases = [
            (0, 32, 16),
            (1, 16, 16),
            (2, 16, 16),
            (3, 256, 256),
            (4, 256, 256),
        ];
        for (format, width, height) in cases {
            let raw: Vec<u8> = match format {
                0..=2 => (0..=255).collect(),
                _ => (0..=u16::MAX).flat_map(u16::to_be_bytes).collect(),
            };
            let rgba = decode(&raw, width, height, format);
            let encoded = encode_texture(&rgba, width, height, format, None).unwrap();
            assert!(encoded == raw, "format {format} codes did not round trip");
        }

        let raw: Vec<u8> = (0..=u16::MAX).flat_map(u16::to_be_bytes).collect();
        let rgba = decode(&raw, 256, 256, 5);
        let encoded = encode_texture(&rgba, 256, 256, 5, None).unwrap();
        assert_eq!(decode(&encoded, 256, 256, 5), rgba);
        let (mut exact, mut switched) = (0, 0);
        for (original, encoded) in raw.chunks(2).zip(encoded.chunks(2)) {
            let code = u16::from_be_bytes([original[0], original[1]]);
            if original == encoded {
                exact += 1;
            } else {
                // Only opaque ARGB3444 whose channels are all 0 or 15 can move.
                assert_eq!(code & 0xF000, 0x7000, "RGB5A3 {code:#06x} changed");
                assert!(
                    [8, 4, 0]
                        .iter()
                        .all(|shift| matches!((code >> shift) & 0xF, 0 | 15)),
                    "RGB5A3 {code:#06x} changed"
                );
                switched += 1;
            }
        }
        assert_eq!((exact, switched), (65536 - 8, 8));

        let raw = noise(64 * 64 * 4, 0x2545_F491);
        let rgba = decode(&raw, 64, 64, 6);
        assert!(encode_texture(&rgba, 64, 64, 6, None).unwrap() == raw);
    }

    /// Sizes that are not block multiples: visible texels keep their codes and
    /// padding encodes as zero.
    #[test]
    fn partial_blocks_round_trip_visible_texels() {
        for (seed, format) in (1u32..).zip(DIRECT_FORMATS) {
            let (width, height) = (13, 7);
            let padded_len = encode_texture(&[0; 13 * 7 * 4], width, height, format, None)
                .unwrap()
                .len();
            let raw = noise(padded_len, seed.wrapping_mul(0x9E37_79B9));
            let rgba = decode(&raw, width, height, format);
            let encoded = encode_texture(&rgba, width, height, format, None).unwrap();
            assert_eq!(encoded.len(), padded_len);
            assert_eq!(
                decode(&encoded, width, height, format),
                rgba,
                "format {format}"
            );
        }
    }

    #[test]
    fn color_input_quantizes_to_the_nearest_code() {
        let orange = [255, 128, 0, 200];
        let rgba = orange.repeat(16);
        let expect = |format: u32, texel: &[u8]| {
            let encoded = encode_texture(&rgba, 4, 4, format, None).unwrap();
            assert_eq!(&encoded[..texel.len()], texel, "format {format}");
        };
        // Luma: (299 * 255 + 587 * 128 + 500) / 1000 = 151.
        expect(1, &[151]);
        expect(3, &[200, 151]);
        // I4 and IA4 round 151 to 9 (153) and alpha 200 to 12 (204).
        expect(0, &[0x99]);
        expect(2, &[0xC9]);
        // RGB565: 31, 32 (130), 0.
        expect(4, &(31u16 << 11 | 32 << 5).to_be_bytes());
        // RGB5A3 keeps alpha: 3-bit 5 (182) against 555's 255.
        expect(5, &(5u16 << 12 | 15 << 8 | 8 << 4).to_be_bytes());
    }

    /// Asking again for pixels a lossy format could only approximate
    /// re-encodes to the same bytes, which is no change.
    #[test]
    fn a_repeated_lossy_overwrite_changes_nothing() {
        let (width, height) = (16, 16);
        let gradient: Vec<u8> = (0..width * height)
            .flat_map(|texel| {
                let (x, y) = ((texel % width) as u8, (texel / width) as u8);
                [x * 16, y * 16, x.wrapping_mul(y), 255]
            })
            .collect();
        let original = encode_texture(&[0; 16 * 16 * 4], width, height, 14, None).unwrap();
        let first =
            encode_texture_over(&original, &gradient, width, height, 14, None, None).unwrap();
        assert!(first.changed_blocks > 0);
        assert!(
            decode(&first.bytes, width, height, 14) != gradient,
            "CMPR is lossy here"
        );
        let again =
            encode_texture_over(&first.bytes, &gradient, width, height, 14, None, None).unwrap();
        assert_eq!(again.changed_blocks, 0);
        assert!(again.bytes == first.bytes);
    }

    /// Unchanged blocks keep their original bytes, padding included; only
    /// blocks with a changed visible pixel are re-encoded.
    #[test]
    fn overwrite_keeps_unchanged_blocks_byte_identical() {
        for (seed, format) in (1u32..).zip(DIRECT_FORMATS.into_iter().chain([14])) {
            let (width, height) = (13, 7);
            let len = encode_texture(&[0; 13 * 7 * 4], width, height, format, None)
                .unwrap()
                .len();
            let original = noise(len, seed.wrapping_mul(0x85EB_CA6B));
            let mut rgba = decode(&original, width, height, format);

            let same =
                encode_texture_over(&original, &rgba, width, height, format, None, None).unwrap();
            assert!(same.bytes == original, "format {format}");
            assert_eq!(same.changed_blocks, 0);

            // The bottom-right texel lies in the last, partial block.
            // Copy in another decoded texel so the edit is representable.
            let last = rgba.len() - 4;
            let other = rgba
                .chunks(4)
                .find(|texel| *texel != &rgba[last..])
                .expect("noise decodes to more than one texel value")
                .to_vec();
            rgba[last..].copy_from_slice(&other);
            let edited =
                encode_texture_over(&original, &rgba, width, height, format, None, None).unwrap();
            assert_eq!(edited.changed_blocks, 1, "format {format}");
            let block_bytes = len / edited.blocks;
            let untouched = len - block_bytes;
            assert!(edited.bytes[..untouched] == original[..untouched]);
            // CMPR is lossy: the edited block need not reproduce exactly.
            if format != 14 {
                assert_eq!(decode(&edited.bytes, width, height, format), rgba);
            }
        }
        assert_eq!(
            encode_texture_over(&[0; 3], &[0; 4 * 4 * 4], 4, 4, 6, None, None),
            Err(TextureEncodeError::OriginalLength {
                expected: 64,
                actual: 3,
            })
        );
    }

    /// A decoded CMPR sub-block whose selectors use both endpoints contains
    /// those endpoints as colors, so the encoder finds a zero-error pair.
    #[test]
    fn cmpr_reproduces_blocks_that_show_both_endpoints() {
        let (width, height) = (32, 32);
        let mut raw = noise(32 * 32 / 2, 0xC0FF_EE11);
        for sub_block in raw.chunks_mut(8) {
            sub_block[4] = 0b00_01_10_11;
        }
        let rgba = decode(&raw, width, height, 14);
        let encoded = encode_texture(&rgba, width, height, 14, None).unwrap();
        assert_eq!(encoded.len(), raw.len());
        assert_eq!(decode(&encoded, width, height, 14), rgba);
    }

    #[test]
    fn cmpr_blends_with_the_gamecube_weights() {
        // Red 255 and 0 with GX's 5/8 + 3/8 blends (159, 95) round-trip exactly;
        // DXT1's thirds would give 170 and 85.
        let rgba: Vec<u8> = [255, 0, 159, 95]
            .repeat(4)
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect();
        let encoded = encode_texture(&rgba, 4, 4, 14, None).unwrap();
        assert_eq!(decode(&encoded, 4, 4, 14), rgba);
    }

    #[test]
    fn cmpr_keeps_transparency_and_zeroes_padding() {
        // Left half transparent, right half opaque teal; a 4x4 image fills one
        // sub-block of its 8x8 tile, and the other three are padding.
        let rgba: Vec<u8> = (0..16)
            .flat_map(|i| {
                if i % 4 < 2 {
                    [0, 0, 0, 0]
                } else {
                    [0, 128, 128, 255]
                }
            })
            .collect();
        let encoded = encode_texture(&rgba, 4, 4, 14, None).unwrap();
        assert_eq!(encoded.len(), 32);
        assert_eq!(encoded[8..], [0; 24]);
        let decoded = decode(&encoded, 4, 4, 14);
        for (texel, source) in decoded.chunks(4).zip(rgba.chunks(4)) {
            assert_eq!(texel[3], source[3]);
            if source[3] == 255 {
                // Teal isn't representable in RGB565; the average of two
                // neighbouring endpoints, (0, 127, 127), is closer than either.
                assert_eq!(texel[..3], [0, 127, 127]);
            }
        }
    }

    /// A gradient along one axis keeps each block's colors on a line, which
    /// CMPR's four-color palette can follow closely.
    #[test]
    fn cmpr_follows_a_one_axis_gradient_closely() {
        let (width, height) = (16u16, 16u16);
        let rgba: Vec<u8> = (0..256)
            .flat_map(|i| {
                let x = (i % 16) as u8;
                [x * 16, 64 + x * 8, 255 - x * 12, 255]
            })
            .collect();
        let decoded = decode(
            &encode_texture(&rgba, width, height, 14, None).unwrap(),
            width,
            height,
            14,
        );
        let errors: Vec<u8> = decoded
            .iter()
            .zip(&rgba)
            .map(|(&decoded, &source)| decoded.abs_diff(source))
            .collect();
        let max = *errors.iter().max().unwrap();
        let mean = errors.iter().map(|&e| f64::from(e)).sum::<f64>() / errors.len() as f64;
        assert!(max <= 6 && mean <= 1.5, "max {max}, mean {mean}");
    }

    /// Distinct colors, so every index decodes to a color only it has.
    fn distinct_palette(len: usize) -> Vec<[u8; 4]> {
        (0..len)
            .map(|i| [i as u8, (i * 7) as u8, 255 - i as u8, (i * 3) as u8])
            .collect()
    }

    fn decode_with(
        data: &[u8],
        width: u16,
        height: u16,
        format: u32,
        palette: &[[u8; 4]],
    ) -> Vec<u8> {
        decode_image(data, width, height, format, Some(palette)).expect("decodable test texture")
    }

    #[test]
    fn every_palette_index_round_trips() {
        // CI8: all 256 indices in a 16x16 image; CI4: all 256 index pairs.
        for (format, colors, width, height) in [(9, 256, 16, 16), (8, 16, 32, 16)] {
            let palette = distinct_palette(colors);
            let raw: Vec<u8> = (0..=255).collect();
            let rgba = decode_with(&raw, width, height, format, &palette);
            let encoded = encode_texture(&rgba, width, height, format, Some(&palette)).unwrap();
            assert!(encoded == raw, "format {format}");
        }
    }

    #[test]
    fn palette_encoding_maps_to_the_nearest_reachable_entry() {
        // Entry 16 would match exactly, but CI4 reaches only 0..16.
        let mut palette = distinct_palette(17);
        palette[16] = [200, 100, 50, 255];
        palette[3] = [190, 100, 50, 255];
        palette[5] = palette[3];
        let rgba = [200u8, 100, 50, 255].repeat(64);
        let ci4 = encode_texture(&rgba, 8, 8, 8, Some(&palette)).unwrap();
        // The tie between entries 3 and 5 goes to 3.
        assert!(ci4.iter().all(|&byte| byte == 0x33));
        let ci8 = encode_texture(&rgba, 8, 8, 9, Some(&palette)).unwrap();
        assert!(ci8.iter().all(|&index| index == 16));
    }

    /// Blocks outside the dirty rectangle keep their bytes even when their
    /// pixels differ, as they do after a full-precision stroke on CMPR.
    #[test]
    fn overwrite_touches_only_blocks_the_dirty_rect_overlaps() {
        // RGB565 16x8: eight 4x4 blocks in two rows of four.
        let original = noise(16 * 8 * 2, 0x1234_5678);
        let rgba = [200u8, 10, 10, 255].repeat(16 * 8);
        // Texels (5..7, 1..3) overlap only block 1 (x 4..8, y 0..4).
        let dirty = TexelRect {
            x: 5,
            y: 1,
            width: 2,
            height: 2,
        };
        let edited = encode_texture_over(&original, &rgba, 16, 8, 4, None, Some(dirty)).unwrap();
        assert_eq!((edited.blocks, edited.changed_blocks), (8, 1));
        assert!(edited.bytes[..32] == original[..32]);
        assert!(edited.bytes[32..64] != original[32..64]);
        assert!(edited.bytes[64..] == original[64..]);

        let everything = encode_texture_over(&original, &rgba, 16, 8, 4, None, None).unwrap();
        assert_eq!(everything.changed_blocks, 8);
    }

    #[test]
    fn rejects_unsupported_formats_and_wrong_lengths() {
        for format in [7, 10] {
            assert_eq!(
                encode_texture(&[0; 64 * 4], 8, 8, format, None),
                Err(TextureEncodeError::UnsupportedFormat(format))
            );
        }
        for format in [8, 9] {
            for palette in [None, Some(&[][..])] {
                assert_eq!(
                    encode_texture(&[0; 64 * 4], 8, 8, format, palette),
                    Err(TextureEncodeError::MissingPalette(format))
                );
            }
        }
        assert_eq!(
            encode_texture(&[0; 10], 2, 2, 6, None),
            Err(TextureEncodeError::PixelLength {
                width: 2,
                height: 2,
                expected: 16,
                actual: 10,
            })
        );
    }
}
