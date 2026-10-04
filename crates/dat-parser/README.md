# dat-parser

Parser and renderer-neutral semantic layer for HAL's HSD `.dat` archives, the
model format of Super Smash Bros. Melee and other HAL GameCube games.

The original DAT bytes remain the source of truth. This crate validates hostile
input, resolves the HSD pointer graph, decodes GX geometry and textures, and
builds a bounded scene with its evaluated draw work. It knows nothing about a
GPU backend (`hsd-render`) or application policy, and leaves a particular
game to `melee-dat`, with three exceptions that are Melee's and live here
because the evaluation they feed does: the fighter envelope weight policy,
FigaTree joint animation, and the stage `map_head` descriptor.

A scene is built from the archive's model roots, or from roots a caller found
by structures the root table doesn't list
(`HsdScene::from_model_roots_with_limits`, for a fighter's articles or an
effect table's models). Evaluation follows HSD's runtime as far as the draw
work needs: a pose can hold joints to another
model's joint as RObj constraints do (`HsdJointConstraint`, by position and
orientation), and an evaluator given the camera's view
(`HsdDrawWorkEvaluator::set_view`) turns billboarded joints to face it, as
`HSD_JObjMakePositionMtx` does.

## Layout

- `hal-dat-raw` (re-exported as `dat_parser::raw`, with `DatFile` and its errors
  at the crate root): archive bytes, header, relocation, roots, externs.
- `src/descriptor/`: parsers for serialized HSD descriptors.
- `src/gx/`: GX display lists, vertex attributes, texture formats.
- `src/math.rs`: matrix math shared by the semantic layers.
- `src/hsd/`: scene construction (`scene/`), draw evaluation (`draw/`), a loaded `HsdSource` (`source.rs`), envelopes, PE and
  draw passes, color channels, texture coordinates, custom TEV, and animation.
- `../melee-dat`: Melee itself: fighters, stages, the reference catalog.

Semantic modules must not depend on rendering backends, browser APIs, feature
flags, or product policy. See
[`docs/DAT_LAYERING_ARCHITECTURE.md`](https://github.com/texturesgg/texturesgg/blob/main/docs/DAT_LAYERING_ARCHITECTURE.md)
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
