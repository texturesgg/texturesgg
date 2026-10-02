//! The texture and palette formats this codec handles.

use std::fmt;
use thiserror::Error;

/// A `GXTexFmt` this codec decodes and encodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TextureFormat {
    I4,
    I8,
    Ia4,
    Ia8,
    Rgb565,
    Rgb5a3,
    Rgba8,
    Ci4,
    Ci8,
    Cmpr,
}

/// A `GXTexFmt` value this codec has no decoder or encoder for.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("GX texture format {0} is not supported")]
pub struct UnsupportedTextureFormat(pub u32);

impl TryFrom<u32> for TextureFormat {
    type Error = UnsupportedTextureFormat;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::I4,
            1 => Self::I8,
            2 => Self::Ia4,
            3 => Self::Ia8,
            4 => Self::Rgb565,
            5 => Self::Rgb5a3,
            6 => Self::Rgba8,
            8 => Self::Ci4,
            9 => Self::Ci8,
            14 => Self::Cmpr,
            _ => return Err(UnsupportedTextureFormat(value)),
        })
    }
}

impl From<TextureFormat> for u32 {
    /// The `GXTexFmt` value.
    fn from(format: TextureFormat) -> Self {
        match format {
            TextureFormat::I4 => 0,
            TextureFormat::I8 => 1,
            TextureFormat::Ia4 => 2,
            TextureFormat::Ia8 => 3,
            TextureFormat::Rgb565 => 4,
            TextureFormat::Rgb5a3 => 5,
            TextureFormat::Rgba8 => 6,
            TextureFormat::Ci4 => 8,
            TextureFormat::Ci8 => 9,
            TextureFormat::Cmpr => 14,
        }
    }
}

impl TextureFormat {
    /// The format's usual short name ("CMPR", "CI8").
    pub fn name(self) -> &'static str {
        match self {
            Self::I4 => "I4",
            Self::I8 => "I8",
            Self::Ia4 => "IA4",
            Self::Ia8 => "IA8",
            Self::Rgb565 => "RGB565",
            Self::Rgb5a3 => "RGB5A3",
            Self::Rgba8 => "RGBA8",
            Self::Ci4 => "CI4",
            Self::Ci8 => "CI8",
            Self::Cmpr => "CMPR",
        }
    }

    /// The most palette entries a texel can index: 16 for CI4, 256 for CI8,
    /// and `None` for a format that stores colors directly.
    pub fn palette_entries(self) -> Option<usize> {
        match self {
            Self::Ci4 => Some(16),
            Self::Ci8 => Some(256),
            _ => None,
        }
    }

    /// The texel dimensions of one storage tile. Texel data is a row-major
    /// run of whole tiles, so an image is padded up to a multiple of this.
    pub(crate) fn tile_size(self) -> (usize, usize) {
        match self {
            Self::I4 | Self::Ci4 | Self::Cmpr => (8, 8),
            Self::I8 | Self::Ia4 | Self::Ci8 => (8, 4),
            Self::Ia8 | Self::Rgb565 | Self::Rgb5a3 | Self::Rgba8 => (4, 4),
        }
    }

    /// Stored bits per texel.
    pub(crate) fn bits_per_texel(self) -> u64 {
        match self {
            Self::I4 | Self::Ci4 | Self::Cmpr => 4,
            Self::I8 | Self::Ia4 | Self::Ci8 => 8,
            Self::Ia8 | Self::Rgb565 | Self::Rgb5a3 => 16,
            Self::Rgba8 => 32,
        }
    }
}

impl fmt::Display for TextureFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A `GXTlutFmt`: how a palette stores each 16-bit entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PaletteFormat {
    Ia8,
    Rgb565,
    Rgb5a3,
}

/// A value that is not a `GXTlutFmt`.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("palette format {0} is not IA8 (0), RGB565 (1), or RGB5A3 (2)")]
pub struct UnsupportedPaletteFormat(pub u32);

impl TryFrom<u32> for PaletteFormat {
    type Error = UnsupportedPaletteFormat;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Ia8,
            1 => Self::Rgb565,
            2 => Self::Rgb5a3,
            _ => return Err(UnsupportedPaletteFormat(value)),
        })
    }
}

impl From<PaletteFormat> for u32 {
    /// The `GXTlutFmt` value.
    fn from(format: PaletteFormat) -> Self {
        match format {
            PaletteFormat::Ia8 => 0,
            PaletteFormat::Rgb565 => 1,
            PaletteFormat::Rgb5a3 => 2,
        }
    }
}

impl PaletteFormat {
    /// The format's usual short name ("RGB5A3").
    pub fn name(self) -> &'static str {
        match self {
            Self::Ia8 => "IA8",
            Self::Rgb565 => "RGB565",
            Self::Rgb5a3 => "RGB5A3",
        }
    }
}

impl fmt::Display for PaletteFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{PaletteFormat, TextureFormat, UnsupportedPaletteFormat, UnsupportedTextureFormat};

    /// Every `GXTexFmt` value either maps to a format that maps back to it,
    /// or is refused by value.
    #[test]
    fn formats_convert_to_and_from_their_gx_values() {
        let supported: Vec<u32> = (0..32)
            .filter(|&value| match TextureFormat::try_from(value) {
                Ok(format) => {
                    assert_eq!(u32::from(format), value);
                    true
                }
                Err(error) => {
                    assert_eq!(error, UnsupportedTextureFormat(value));
                    false
                }
            })
            .collect();
        assert_eq!(supported, [0, 1, 2, 3, 4, 5, 6, 8, 9, 14]);
        for value in 0..3 {
            assert_eq!(u32::from(PaletteFormat::try_from(value).unwrap()), value);
        }
        assert_eq!(PaletteFormat::try_from(3), Err(UnsupportedPaletteFormat(3)));
    }
}
