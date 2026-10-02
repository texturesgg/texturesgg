# `gx-texture`

`gx-texture` is the GameCube GX texture codec. It works on raw texel and
palette bytes, so it serves anything that stores GX textures; `dat-parser`
slices them out of DAT archives. See
[`docs/DAT_LAYERING_ARCHITECTURE.md`](../../docs/DAT_LAYERING_ARCHITECTURE.md).

## Scope

The crate owns:

- decoding every format Melee uses (I4, I8, IA4, IA8, RGB565, RGB5A3, RGBA8,
  CI4, CI8, CMPR) and TLUT palettes to RGBA8, following Dolphin's decoder;
- encoding them back: nearest codes for the direct formats, nearest entries of
  an existing palette for CI4 and CI8, and an endpoint search against the
  exact GX palette for CMPR;
- tile and block layout, including re-encoding only the blocks an edit changes
  (`encode_texture_over`).

It deliberately contains no DAT or HSD knowledge, no file I/O, and no palette
generation.
