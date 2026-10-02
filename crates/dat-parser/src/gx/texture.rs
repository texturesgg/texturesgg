//! GX texture decoding for images stored in a DAT, over the `gx-texture`
//! codec: these validate the data-section ranges and delegate.

use crate::DatFile;
use crate::descriptor::tobj::TlutDesc;

/// Decode the image at data-section offset `data_ptr` to row-major RGBA8
/// (`width * height * 4` bytes). CI4 and CI8 need their `tlut`.
pub fn decode_texture(
    dat: &DatFile,
    data_ptr: u32,
    width: u16,
    height: u16,
    format: u32,
    tlut: Option<&TlutDesc>,
) -> Option<Vec<u8>> {
    let raw = dat.data_slice(
        data_ptr,
        gx_texture::image_data_size(format, width, height)?,
    )?;
    let palette = match format {
        8 | 9 => Some(decode_palette(dat, tlut?)?),
        _ => None,
    };
    gx_texture::decode_image(raw, width, height, format, palette.as_deref())
}

/// Decode a TLUT's colors to RGBA8.
pub fn decode_palette(dat: &DatFile, tlut: &TlutDesc) -> Option<Vec<[u8; 4]>> {
    let raw = dat.data_slice(tlut.data_ptr?, usize::from(tlut.color_count) * 2)?;
    gx_texture::decode_palette(raw, tlut.format, tlut.color_count)
}

#[cfg(test)]
mod tests {
    use super::decode_texture;
    use crate::DatFile;

    fn dat(data: Vec<u8>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), Vec::new())
    }

    #[test]
    fn decodability_matches_decoder_input_boundaries() {
        let valid = dat(vec![0; 32]);
        assert!(decode_texture(&valid, 0, 8, 8, 0, None).is_some());

        let truncated = dat(vec![0; 31]);
        assert!(decode_texture(&truncated, 0, 8, 8, 0, None).is_none());

        let unsupported = dat(vec![0; 64]);
        assert!(decode_texture(&unsupported, 0, 8, 8, 7, None).is_none());

        let missing_palette = dat(vec![0; 32]);
        assert!(decode_texture(&missing_palette, 0, 8, 8, 8, None).is_none());
    }
}
