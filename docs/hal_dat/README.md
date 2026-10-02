# HSD format reference

HAL's HSD library stores models and animation in `.dat` archives. These two
documents describe that format as Super Smash Bros. Melee (NTSC 1.02) uses it:

- [`ssbm_hal_dat_tables.md`](ssbm_hal_dat_tables.md): the archive container and
  the model structures in it (joints, display objects, materials, textures,
  polygons, stage and menu tables).
- [`ssbm_hal_dat_animation_tables.md`](ssbm_hal_dat_animation_tables.md): the
  animation structures, the packed keyframe stream, and how the game evaluates
  them.

They are a lookup surface for readers of `dat-parser` and `melee-dat`. What
those crates implement is in
[`../DAT_LAYERING_ARCHITECTURE.md`](../DAT_LAYERING_ARCHITECTURE.md), and what
they leave out is in
[`../RENDERER_KNOWN_ISSUES.md`](../RENDERER_KNOWN_ISSUES.md).

## Sources

- The [Melee decompilation](https://github.com/doldecomp/melee). A function or
  file name beside a fact (`HSD_JObjLoadJoint`, `dobj.c`) names where the game
  does it; links pin a commit.
- The [MKWiiki page on the format](<https://mkwiiki.org/wiki/HAL_DAT_(File_Format)>),
  which the structure tables started from. Where the two disagree, the
  decompilation wins and the table states what the game does.

## Rules for these documents

1. State what the format is and what the game does with it. How the code
   handles it belongs in the code and the architecture document.
2. Pin source links to a commit or a page revision.
3. Keep raw fields and unknown bits in the tables even when their meaning is
   unknown, and say that it is unknown.
4. No game data: no bytes from a DAT or the executable, no extracted
   animation, and nothing that identifies a private file.
