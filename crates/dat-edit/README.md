# `dat-edit`

`dat-edit` writes texture and color edits back into HSD DAT archives.
`dat-parser` reads archives and stays read-only; this crate owns writing. See
[`docs/DAT_LAYERING_ARCHITECTURE.md`](https://github.com/texturesgg/texturesgg/blob/main/docs/DAT_LAYERING_ARCHITECTURE.md).

## Scope

The crate owns:

- the texture document (`TextureDocument`): an open DAT's textures, one per
  block of pixel data with every image/palette descriptor pair that draws it,
  edits applied through a chosen use, every use re-decoded after an edit, and
  a fidelity report (changed blocks, lossy texels);
- in-place texture patches (`patch_texture`): re-encode only the blocks whose
  pixels changed, through `gx-texture`, and write them over the original
  bytes, keeping CI4/CI8 palettes;
- vertex and material colors: the colors a surface is drawn with, grouped
  into swatches, and rewritten at every site in the format each is stored in;
- undo and redo as byte snapshots of what an edit changed;
- the refusals that keep an edit safe (mipmapped images, data that overlaps
  a relocated pointer, ranges past the data section, pixel data two textures
  read differently).

Edits never move data, so the header, tables, and every other byte stay
identical. Operations that resize or relocate data (new images, a larger
palette) are out of scope.
