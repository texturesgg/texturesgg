//! The GameCube GX texture codec, on raw bytes.
//!
//! Formats are `GXTexFmt` values (I4 0 through RGBA8 6, CI4 8, CI8 9, CMPR 14)
//! and palettes `GXTlutFmt` values (IA8 0, RGB565 1, RGB5A3 2). Images decode
//! to row-major RGBA8. Decoding follows Dolphin's `TextureDecoder_Generic.cpp`,
//! over a tile walk adapted from libWiiSharp; encoding inverts it. This crate knows nothing about DAT archives; callers
//! slice texel and palette bytes out of whatever container holds them.

mod decode;
mod encode;
mod palette;

pub use decode::{decode_image, decode_palette, format_name, image_data_size};
pub use encode::{
    TexelRect, TextureEncodeError, TextureOverwrite, encode_texture, encode_texture_over,
};
pub use palette::{MAX_PALETTE_ENTRIES, PaletteError, build_palette};
