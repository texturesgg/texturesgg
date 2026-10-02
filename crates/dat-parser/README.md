# dat-parser

Parser and renderer-neutral semantic layer for HAL's HSD `.dat` archives, the
model format of Super Smash Bros. Melee and other HAL GameCube games.

The original DAT bytes remain the source of truth. This crate validates hostile
input, resolves the HSD pointer graph, decodes GX geometry and textures, and
builds a bounded scene with its evaluated draw work. It knows nothing about a
particular game (that is `melee-dat`), a GPU backend (`hsd-render`), or
application policy.

## Layout

- `hal-dat-raw` (re-exported as `dat_parser::raw`, with `DatFile` and its errors
  at the crate root): archive bytes, header, relocation, roots, externs.
- `src/descriptor/`: parsers for serialized HSD descriptors.
- `src/gx/`: GX display lists, vertex attributes, texture formats, PNG output.
- `src/math.rs`: matrix math shared by the semantic layers.
- `src/hsd/`: scene construction (`scene/`), draw evaluation (`draw/`), a loaded `HsdSource` (`source.rs`), envelopes, PE and
  draw passes, color channels, texture coordinates, custom TEV, and animation.
- `../melee-dat`: Melee itself: fighters, stages, the reference catalog.

Semantic modules must not depend on rendering backends, browser APIs, feature
flags, or product policy. See
[`docs/DAT_LAYERING_ARCHITECTURE.md`](../../docs/DAT_LAYERING_ARCHITECTURE.md)
for the layers, contracts, and the source-backed rules they follow.

## Invariants

- Parse and validate before dereferencing source offsets.
- Bound input sizes, graph traversal, decoded collections, animation work, and
  browser payloads.
- Preserve source ordering, pointer provenance, unsupported states, and the
  distinct generic and fighter evaluation policies.
- Fail with a structured error when serialized state cannot be represented by a
  supported contract.
- Keep immutable topology separate from per-frame matrices and deformed vertices.

## Commands

```bash
# Parse a DAT and print a summary
cargo run -p dat-parser --bin parse-dat -- input.dat

# Extract decoded textures
cargo run -p dat-parser --bin extract-textures -- input.dat output-directory

# Unit and reference tests
cargo test -p dat-parser
```

Every test builds its DAT bytes by hand; none needs a game file.
