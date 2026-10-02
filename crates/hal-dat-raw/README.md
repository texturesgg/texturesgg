# `hal-dat-raw`

`hal-dat-raw` is a bounded parser for the HSD DAT archive container: the bytes,
header, relocation table, roots and externs, with no knowledge of what the
archive holds. It is the lowest layer of
[`docs/DAT_LAYERING_ARCHITECTURE.md`](https://github.com/texturesgg/texturesgg/blob/main/docs/DAT_LAYERING_ARCHITECTURE.md).

## Scope

The crate owns:

- big-endian bounded byte reads;
- the archive header and declared table extents;
- relocation sites and classified pointer resolution: a relocated word of zero
  points at the start of the data section, an unrelocated zero is null;
- public-root and extern table records;
- parser errors and archive-level resource limits (`DatResource` names the
  one a file went over).

`DatFile`, `DatHeader` and `RootNode` have public fields, so an archive can be
built in code (`DatFile::from_parts`) or patched in place. `DatFile::parse` is
what validates one; the rustdoc on `DatFile` lists the invariants a hand-built
value has to keep itself.

It holds no HSD descriptors, no game semantics, no rendering and no disc-image
handling. `dat-parser` builds on it and re-exports it as `dat_parser::raw`,
with `DatFile` and its errors at the crate root.

```bash
cargo test -p hal-dat-raw
```
