# `gx-texture`

`gx-texture` is the GameCube GX texture codec. It works on raw texel and
palette bytes, so it serves anything that stores GX textures; `dat-parser`
slices them out of DAT archives. See
[`docs/DAT_LAYERING_ARCHITECTURE.md`](https://github.com/texturesgg/texturesgg/blob/main/docs/DAT_LAYERING_ARCHITECTURE.md).

## Scope

The crate owns:

- decoding every format Melee uses (I4, I8, IA4, IA8, RGB565, RGB5A3, RGBA8,
  CI4, CI8, CMPR) and TLUT palettes to RGBA8, following Dolphin's decoder;
- encoding them back: nearest codes for the direct formats, nearest entries of
  an existing palette for CI4 and CI8, and an endpoint search against the
  exact GX palette for CMPR;
- building a palette for an image (median cut, refined by k-means);
- tile and block layout, including re-encoding only the blocks an edit changes
  (`encode_texture_over`).

`TextureFormat` and `PaletteFormat` name the formats; each converts from its
raw `GXTexFmt` or `GXTlutFmt` value with `TryFrom<u32>`, which refuses a value
the codec has no code for. Decoding reports a short buffer or a missing
palette as a `TextureDecodeError`. A CI4 or CI8 texel whose index is past the
end of its palette decodes as transparent black.

It deliberately contains no DAT or HSD knowledge and no file I/O.

## Credits

Channel expansion and CMPR blending follow Dolphin's
`TextureDecoder_Generic.cpp` (GPL-2.0-or-later). The decoders' walk over each
format's tiles is adapted from libWiiSharp (Copyright (C) 2009 Leathl,
GPL-3.0-or-later), as carried in HSDLib's `GXImageConverter`.
