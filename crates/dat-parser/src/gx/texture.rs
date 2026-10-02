//! GX texture decoding for images stored in a DAT, over the `gx-texture`
//! codec: these validate the data-section ranges and delegate.

use crate::DatFile;
use crate::descriptor::tobj::TlutDesc;
pub use gx_texture::{
    PaletteFormat, TextureDecodeError, TextureFormat, UnsupportedPaletteFormat,
    UnsupportedTextureFormat,
};
use thiserror::Error;

/// Why an image or palette stored in a DAT did not decode.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextureReadError {
    #[error(transparent)]
    Format(#[from] UnsupportedTextureFormat),
    #[error(transparent)]
    PaletteFormat(#[from] UnsupportedPaletteFormat),
    #[error("the image descriptor has no texel data")]
    NoImageData,
    #[error("the texel data at {offset:#x} runs past the data section")]
    ImageOutOfBounds { offset: u32 },
    #[error("the palette descriptor has no color data")]
    NoPaletteData,
    #[error("the palette data at {offset:#x} runs past the data section")]
    PaletteOutOfBounds { offset: u32 },
    #[error(transparent)]
    Decode(#[from] TextureDecodeError),
}

/// Decode the image at data-section offset `data_ptr` to row-major RGBA8
/// (`width * height * 4` bytes). CI4 and CI8 need their `tlut`.
pub fn decode_texture(
    dat: &DatFile,
    data_ptr: u32,
    width: u16,
    height: u16,
    format: TextureFormat,
    tlut: Option<&TlutDesc>,
) -> Result<Vec<u8>, TextureReadError> {
    let size = gx_texture::image_data_size(width, height, format)?;
    let raw = dat
        .data_slice(data_ptr, size)
        .ok_or(TextureReadError::ImageOutOfBounds { offset: data_ptr })?;
    let palette = match format.palette_entries() {
        Some(_) => {
            let tlut = tlut.ok_or(TextureDecodeError::MissingPalette(format))?;
            Some(decode_palette(dat, tlut)?)
        }
        None => None,
    };
    Ok(gx_texture::decode_image(
        raw,
        width,
        height,
        format,
        palette.as_deref(),
    )?)
}

/// Decode a TLUT's colors to RGBA8.
pub fn decode_palette(dat: &DatFile, tlut: &TlutDesc) -> Result<Vec<[u8; 4]>, TextureReadError> {
    let format = PaletteFormat::try_from(tlut.format)?;
    let offset = tlut.data_ptr.ok_or(TextureReadError::NoPaletteData)?;
    let raw = dat
        .data_slice(offset, usize::from(tlut.color_count) * 2)
        .ok_or(TextureReadError::PaletteOutOfBounds { offset })?;
    Ok(gx_texture::decode_palette(raw, format, tlut.color_count)?)
}

#[cfg(test)]
mod tests {
    use super::{TextureDecodeError, TextureFormat, TextureReadError, decode_texture};
    use crate::DatFile;

    fn dat(data: Vec<u8>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), Vec::new())
    }

    #[test]
    fn decodability_matches_decoder_input_boundaries() {
        let valid = dat(vec![0; 32]);
        assert!(decode_texture(&valid, 0, 8, 8, TextureFormat::I4, None).is_ok());

        let truncated = dat(vec![0; 31]);
        assert_eq!(
            decode_texture(&truncated, 0, 8, 8, TextureFormat::I4, None),
            Err(TextureReadError::ImageOutOfBounds { offset: 0 })
        );

        let missing_palette = dat(vec![0; 32]);
        assert_eq!(
            decode_texture(&missing_palette, 0, 8, 8, TextureFormat::Ci4, None),
            Err(TextureReadError::Decode(
                TextureDecodeError::MissingPalette(TextureFormat::Ci4)
            ))
        );
    }
}
