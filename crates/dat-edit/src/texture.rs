//! Writing new texture pixels into a DAT's image data.

use dat_parser::DatFile;
use dat_parser::descriptor::DescriptorParseError;
use dat_parser::descriptor::tobj::{ImageDesc, TlutDesc};
use dat_parser::gx::texture::decode_palette;
use gx_texture::{TexelRect, TextureEncodeError, encode_texture_over, image_data_size};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TexturePatchError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("image descriptor {descriptor:#x} has no pixel data")]
    NoImageData { descriptor: u32 },
    #[error("palette descriptor {descriptor:#x} has no readable colors")]
    NoPaletteData { descriptor: u32 },
    #[error("image descriptor {descriptor:#x} is mipmapped; mip chains cannot be patched yet")]
    Mipmapped { descriptor: u32 },
    #[error(
        "palette descriptor {descriptor:#x} holds {capacity} colors, fewer than the {colors} to write"
    )]
    PaletteTooLarge {
        descriptor: u32,
        colors: usize,
        capacity: usize,
    },
    #[error("the data at {data_offset:#x} runs past the data section")]
    OutOfBounds { data_offset: u32 },
    #[error("the data at {data_offset:#x} overlaps the pointer field at {site:#x}")]
    OverlapsPointer { data_offset: u32, site: u32 },
    #[error(transparent)]
    Encode(#[from] TextureEncodeError),
}

/// New texel bytes for a texture, where they go, and how much changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexturePatch {
    /// Data-section offset of the texture's pixel data.
    pub data_offset: u32,
    /// The texture's encoded pixel data after the edit.
    pub bytes: Vec<u8>,
    pub blocks: usize,
    pub changed_blocks: usize,
}

/// Encode row-major RGBA8 `rgba` as the new pixels of the image described at
/// data-section offset `image`, returning the bytes to write at the image's
/// data (see [`TexturePatch`]); nothing is written. CI4 and CI8 images also
/// need the offset of the TLUT descriptor they are drawn with (the TObj's, or
/// a material animation's); their palette is kept and each texel maps to its
/// nearest color.
///
/// Only storage blocks whose pixels change are re-encoded, and with `dirty`
/// only the blocks it overlaps are considered (see [`encode_texture_over`]).
/// Every TObj or material animation sharing the image data sees the edit
/// once the bytes are written. The patch is refused when its range runs
/// past the data section or covers a relocated pointer.
pub fn patch_texture(
    dat: &DatFile,
    image: u32,
    palette: Option<u32>,
    rgba: &[u8],
    dirty: Option<TexelRect>,
) -> Result<TexturePatch, TexturePatchError> {
    let descriptor = image;
    let image = ImageDesc::parse(dat, descriptor)?;
    let data_offset = image
        .data_ptr
        .ok_or(TexturePatchError::NoImageData { descriptor })?;
    if image.mipmap != 0 {
        return Err(TexturePatchError::Mipmapped { descriptor });
    }
    let colors = palette
        .map(|descriptor| {
            let tlut = TlutDesc::parse(dat, descriptor)?;
            decode_palette(dat, &tlut).ok_or(TexturePatchError::NoPaletteData { descriptor })
        })
        .transpose()?;
    let byte_len = image_data_size(image.format, image.width, image.height)
        .ok_or(TextureEncodeError::UnsupportedFormat(image.format))?;
    let original = writable(dat, data_offset, byte_len)?;

    let overwrite = encode_texture_over(
        original,
        rgba,
        image.width,
        image.height,
        image.format,
        colors.as_deref(),
        dirty,
    )?;
    Ok(TexturePatch {
        data_offset,
        bytes: overwrite.bytes,
        blocks: overwrite.blocks,
        changed_blocks: overwrite.changed_blocks,
    })
}

/// Where big-endian TLUT `entries` in the palette's own format (see
/// [`gx_texture::build_palette`]) go to replace the colors of the palette
/// described at data-section offset `descriptor`: its data-section offset,
/// once checked safe to overwrite. Nothing is written; every image drawn
/// through the palette sees the change once the entries are.
pub(crate) fn patch_palette(
    dat: &DatFile,
    descriptor: u32,
    entries: &[u8],
) -> Result<u32, TexturePatchError> {
    let tlut = TlutDesc::parse(dat, descriptor)?;
    let data_offset = tlut
        .data_ptr
        .ok_or(TexturePatchError::NoPaletteData { descriptor })?;
    if entries.len() > usize::from(tlut.color_count) * 2 {
        return Err(TexturePatchError::PaletteTooLarge {
            descriptor,
            colors: entries.len() / 2,
            capacity: usize::from(tlut.color_count),
        });
    }
    writable(dat, data_offset, entries.len())?;
    Ok(data_offset)
}

/// The `len` data-section bytes at `data_offset`, when they're safe to
/// overwrite: inside the data section, and holding no relocated pointer
/// (that would mean a wrong descriptor, and overwriting it would corrupt the
/// pointer).
pub(crate) fn writable(
    dat: &DatFile,
    data_offset: u32,
    len: usize,
) -> Result<&[u8], TexturePatchError> {
    let original = dat
        .data_slice(data_offset, len)
        .ok_or(TexturePatchError::OutOfBounds { data_offset })?;
    let start = u64::from(data_offset);
    let end = start + len as u64;
    let sites = &dat.relocation_sites;
    let first = sites.partition_point(|&site| u64::from(site) + 4 <= start);
    if let Some(&site) = sites.get(first).filter(|&&site| u64::from(site) < end) {
        return Err(TexturePatchError::OverlapsPointer { data_offset, site });
    }
    Ok(original)
}

#[cfg(test)]
mod tests {
    use super::{TexturePatch, TexturePatchError, patch_texture};
    use dat_parser::DatFile;
    use dat_parser::descriptor::tobj::TlutDesc;
    use dat_parser::gx::texture::decode_texture;
    use dat_parser::raw::header::DATA_SECTION_OFFSET;
    use gx_texture::TextureEncodeError;

    const DESCRIPTOR: u32 = 0;
    const PIXELS: u32 = 0x20;

    /// A DAT whose data section holds one 8x8 image descriptor at 0 pointing at
    /// pixel data at 0x20, with its data pointer relocated.
    fn archive(format: u32, pixels: &[u8], mipmap: u32, extra_sites: &[u32]) -> Vec<u8> {
        let mut data = vec![0; PIXELS as usize];
        data[0..4].copy_from_slice(&PIXELS.to_be_bytes());
        data[4..6].copy_from_slice(&8u16.to_be_bytes());
        data[6..8].copy_from_slice(&8u16.to_be_bytes());
        data[8..12].copy_from_slice(&format.to_be_bytes());
        data[12..16].copy_from_slice(&mipmap.to_be_bytes());
        data.extend_from_slice(pixels);
        let sites: Vec<u32> = [DESCRIPTOR].iter().chain(extra_sites).copied().collect();

        let mut file = vec![0; DATA_SECTION_OFFSET];
        let file_size = DATA_SECTION_OFFSET + data.len() + sites.len() * 4;
        file[0..4].copy_from_slice(&(file_size as u32).to_be_bytes());
        file[4..8].copy_from_slice(&(data.len() as u32).to_be_bytes());
        file[8..12].copy_from_slice(&(sites.len() as u32).to_be_bytes());
        file.extend_from_slice(&data);
        file.extend(sites.iter().flat_map(|site| site.to_be_bytes()));
        file
    }

    /// `file` with `patch`'s bytes written, as a document writes them.
    fn patched(file: &[u8], patch: &TexturePatch) -> Vec<u8> {
        let mut file = file.to_vec();
        let start = DATA_SECTION_OFFSET + patch.data_offset as usize;
        file[start..start + patch.bytes.len()].copy_from_slice(&patch.bytes);
        file
    }

    fn decode(file: &[u8], format: u32) -> Vec<u8> {
        let dat = DatFile::parse(file).expect("patched file reparses");
        decode_texture(&dat, PIXELS, 8, 8, format, None).expect("texture decodes")
    }

    /// RGB565 8x8 is four 4x4 blocks of 32 bytes, stored row-major.
    fn rgb565_pixels() -> Vec<u8> {
        (0..64u16)
            .flat_map(|texel| (texel * 997).to_be_bytes())
            .collect()
    }

    #[test]
    fn rewrites_only_the_changed_block_and_reparses() {
        let original = archive(4, &rgb565_pixels(), 0, &[]);
        let dat = DatFile::parse(&original).unwrap();
        let mut rgba = decode(&original, 4);
        // Texel (5, 1) sits in block 1 (top right).
        let texel = (8 + 5) * 4;
        rgba[texel..texel + 4].copy_from_slice(&[255, 0, 0, 255]);

        let patch = patch_texture(&dat, DESCRIPTOR, None, &rgba, None).unwrap();
        assert_eq!(
            (
                patch.data_offset,
                patch.bytes.len(),
                patch.blocks,
                patch.changed_blocks
            ),
            (PIXELS, 128, 4, 1)
        );
        let file = patched(&original, &patch);
        assert_eq!(decode(&file, 4), rgba);

        let pixels = DATA_SECTION_OFFSET + PIXELS as usize;
        let block = pixels + 32..pixels + 64;
        let changed: Vec<usize> = (0..file.len())
            .filter(|&i| file[i] != original[i])
            .collect();
        assert!(!changed.is_empty());
        assert!(changed.iter().all(|i| block.contains(i)), "{changed:?}");
    }

    #[test]
    fn unchanged_pixels_leave_the_file_identical() {
        // Opaque black as ARGB3444 0x7000 re-encodes to RGB555 0x8000, but an
        // unedited block keeps its original bytes.
        let pixels: Vec<u8> = (0..64).flat_map(|_| 0x7000u16.to_be_bytes()).collect();
        let original = archive(5, &pixels, 0, &[]);
        let dat = DatFile::parse(&original).unwrap();
        let rgba = decode(&original, 5);

        let patch = patch_texture(&dat, DESCRIPTOR, None, &rgba, None).unwrap();
        assert_eq!((patch.blocks, patch.changed_blocks), (4, 0));
        assert!(patched(&original, &patch) == original);
    }

    #[test]
    fn refuses_patches_it_cannot_make_safely() {
        let pixels = rgb565_pixels();
        let rgba = vec![0; 8 * 8 * 4];
        let check = |file: Vec<u8>, expected: TexturePatchError| {
            let dat = DatFile::parse(&file).unwrap();
            assert_eq!(
                patch_texture(&dat, DESCRIPTOR, None, &rgba, None),
                Err(expected)
            );
        };

        check(
            archive(4, &pixels, 1, &[]),
            TexturePatchError::Mipmapped { descriptor: 0 },
        );
        // A relocated word must hold an in-bounds pointer, so zero it.
        let mut with_pointer = pixels.clone();
        with_pointer[0x40..0x44].fill(0);
        check(
            archive(4, &with_pointer, 0, &[PIXELS + 0x40]),
            TexturePatchError::OverlapsPointer {
                data_offset: PIXELS,
                site: PIXELS + 0x40,
            },
        );
        check(
            archive(4, &pixels[..96], 0, &[]),
            TexturePatchError::OutOfBounds {
                data_offset: PIXELS,
            },
        );
        check(
            archive(10, &[0; 64], 0, &[]),
            TexturePatchError::Encode(TextureEncodeError::UnsupportedFormat(10)),
        );
        check(
            archive(9, &[0; 64], 0, &[]),
            TexturePatchError::Encode(TextureEncodeError::MissingPalette(9)),
        );
    }

    #[test]
    fn ci8_keeps_its_palette_and_maps_edits_to_the_nearest_color() {
        // CI8 indices, then a TLUT descriptor, then four RGB565 colors.
        const TLUT: u32 = PIXELS + 64;
        const COLORS: u32 = TLUT + 0x10;
        let mut tail: Vec<u8> = (0..64u8).map(|texel| texel % 4).collect();
        let mut tlut = [0u8; 0x10];
        tlut[0..4].copy_from_slice(&COLORS.to_be_bytes());
        tlut[4..8].copy_from_slice(&1u32.to_be_bytes());
        tlut[0x0c..0x0e].copy_from_slice(&4u16.to_be_bytes());
        tail.extend(tlut);
        // Red, green, blue, white.
        tail.extend(
            [0xF800u16, 0x07E0, 0x001F, 0xFFFF]
                .iter()
                .flat_map(|c| c.to_be_bytes()),
        );
        let original = archive(9, &tail, 0, &[TLUT]);
        let dat = DatFile::parse(&original).unwrap();
        let tlut = TlutDesc::parse(&dat, TLUT).unwrap();
        let decode = |file: &[u8]| {
            let dat = DatFile::parse(file).expect("patched file reparses");
            decode_texture(&dat, PIXELS, 8, 8, 9, Some(&tlut)).expect("texture decodes")
        };

        let mut rgba = decode(&original);
        let unchanged = patch_texture(&dat, DESCRIPTOR, Some(TLUT), &rgba, None).unwrap();
        assert_eq!((unchanged.blocks, unchanged.changed_blocks), (2, 0));
        assert!(patched(&original, &unchanged) == original);

        // A near-blue texel takes the blue entry; nothing else moves, the
        // palette included.
        rgba[..4].copy_from_slice(&[10, 5, 240, 255]);
        let patch = patch_texture(&dat, DESCRIPTOR, Some(TLUT), &rgba, None).unwrap();
        let file = patched(&original, &patch);
        assert_eq!(patch.changed_blocks, 1);
        rgba[..4].copy_from_slice(&[0, 0, 255, 255]);
        assert_eq!(decode(&file), rgba);
        let first_index = DATA_SECTION_OFFSET + PIXELS as usize;
        let changed: Vec<usize> = (0..file.len())
            .filter(|&i| file[i] != original[i])
            .collect();
        assert_eq!(changed, [first_index]);
        assert_eq!(file[first_index], 2);
    }
}
