# DAT parsing and rendering architecture

> Open fidelity gaps are in [`RENDERER_KNOWN_ISSUES.md`](RENDERER_KNOWN_ISSUES.md);
> the HSD format reference is in [`hal_dat/`](hal_dat/README.md).

The original DAT bytes are canonical. Everything below them (scene contracts,
draw work, GPU resources, captures) is a derived projection that must be
reproducible from those bytes.

## Layers

```text
hal-dat-raw                archive bytes, header, relocation, roots, externs
gx-texture                 GX texel and palette codec on raw bytes (no DAT knowledge)
    ↓
dat-parser::descriptor     serialized HSD descriptors (JObj, DObj, PObj, MObj, TObj, AObj)
    ↓
dat-parser::hsd            HSD semantics: scene, draw evaluation, envelopes, PE, TEV,
                           color channels, texture coordinates, animation
    ↓
melee-dat                  the game on top: fighters, stages, their animations, the
                           reference catalog, the model a host loads
    ↓
HsdScene + draw work       the renderer-neutral scene and its evaluated draw work
    ↓
hsd-render                 wgpu lowering: geometry, materials, TEV, lighting, draw submission
    ↓
tgg-editor, the textures.gg site
                           the desktop app; the site's canvas (WebGPU, WebGL2)
```

Beside the render path, the site validates uploads with `dat-parser` alone,
compiled to WebAssembly with no GPU, and `dat-edit` (over `dat-parser` and
`gx-texture`) writes edits back into archive bytes for the editor.

Arrows point from provider to consumer. A layer may depend only on layers above
it: the raw crate depends on nothing in this repo, semantic code never imports
browser or backend types, contracts never follow archive pointers, and backends
never interpret raw HSD flags that a contract already resolves.

## Module map

| Path                                             | Owns                                                                                                                                                                                                                                                                                                                             |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/hal-dat-raw`                           | Bounded byte reads, header/table extents, relocation-site pointer resolution, public roots, extern fixup chains, archive errors and limits.                                                                                                                                                                                      |
| `dat-parser/src/descriptor/`                     | Loss-aware parsers for serialized descriptors (`DescriptorReader`, `jobj`, `dobj`, `pobj`, `mobj`, `tobj`, animation, the stage map head, traversal budgets). `dat_parser::raw` re-exports `hal-dat-raw`.                                                                                                                        |
| `crates/gx-texture`                            | The GX texture codec: texel and palette decoding (Dolphin-aligned), encoding for every format the costumes use, CMPR, block layout.                                                                                                                                                                                              |
| `dat-parser/src/gx/`                             | Display lists, vertex attribute decode, DAT-backed texture decoding over `gx-texture`, PNG output.                                                                                                                                                                                                                               |
| `crates/dat-edit`                              | In-place DAT writing: `patch_texture` re-encodes changed texture blocks and writes them back without moving data. `dat-parser` stays read-only.                                                                                                                                                                                  |
| `dat-parser/src/hsd/scene/`                      | `HsdScene`: bounded model-root discovery and scene construction.                                                                                                                                                                                                                                                                 |
| `dat-parser/src/hsd/draw/`                       | Prepared topology, per-frame JObj matrices, INSTANCE handling, rigid/envelope deformation.                                                                                                                                                                                                                                       |
| `dat-parser/src/hsd/source.rs`                   | `HsdSource`: a DAT loaded within `hsd_scene_limits` (scene and prepared evaluator).                                                                                                                                                                                                                                              |
| `dat-parser/src/hsd/envelope.rs`                 | Envelope matrix palettes with generic and fighter single-weight policies.                                                                                                                                                                                                                                                        |
| `dat-parser/src/hsd/{pe,channel,texture,tev}.rs` | Resolved PE state and draw pass, color-channel usage, TObj texture coordinates, validation of the admitted custom-TEV subset.                                                                                                                                                                                                    |
| `dat-parser/src/hsd/animation/`                  | Scalar FObj/AObj playback, joint poses, and AnimJoint trees attached to a model root.                                                                                                                                                                                                                                            |
| `crates/melee-dat`                             | Melee on top of the generic layers: fighter animation binding, model-part selection, catalog playback of a fighter's animations, a stage's general points (whose camera range frames it) and load-time joint animations, the vanilla file table, and the one loaded model every host draws and drives, fighter, stage or static. |
| `hsd-render/src/geometry.rs`                     | Flattening draw work into interleaved vertices and packets, pass draw order, per-frame vertex updates, reflection validation, resource limits.                                                                                                                                                                                   |
| `hsd-render/src/material.rs`, `shader.rs`        | Texture stages, the TEV plan, textures, channel colors, PE lowering, and WGSL generation.                                                                                                                                                                                                                                        |
| `hsd-render/src/lighting.rs`, `camera.rs`        | Lighting presets and the fixed preview views.                                                                                                                                                                                                                                                                                    |
| `hsd-render/src/renderer.rs`                     | GPU resources, pipelines, bindings, and draw submission.                                                                                                                                                                                                                                                                         |
| `crates/tgg-ui`                                | The editor's theme (Gallery and Paper palettes, web tokens as rems) and its components.                                                                                                                                                                                                                                          |
| `crates/tgg-editor`                            | The desktop app: the player's game and skin library, installs into the ISO, and the texture and color editor around the `hsd-render` viewport.                                                                                                                                                                                   |
| `crates/gc-iso`                                | GameCube disc images: the header and file table, one file read at a time, and one file replaced in place.                                                                                                                                                                                                                        |
| `hsd-render/src/bin/hsd_render.rs`               | The capture, pixel-regression (`pixel-baseline.json`), idle, animations and pick CLI.                                                                                                                                                                                                                                            |

## What renders today

| Area                                                                                                                | Status                                                                                    |
| ------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| JObj hierarchy, Euler SRT, INSTANCE draws                                                                           | Implemented.                                                                              |
| Rigid and envelope skinning (generic and fighter policies)                                                          | Implemented.                                                                              |
| Draw passes (opaque → texEdge → translucent)                                                                        | Implemented.                                                                              |
| PE state (standard and the admitted custom mode)                                                                    | Implemented; dither, destination-alpha overrides and logic ops but COPY are rejected.     |
| Color channels and GX per-vertex lighting                                                                           | Implemented with presets; the default is a neutral preview, not a Melee scene.            |
| Multi-stage TObj TEV (color/alpha maps, light-map phases)                                                           | Implemented, up to GX's 8 stages (stock costumes use 2).                                  |
| Texture coordinates: TEX0-TEX7 matrices, reflection                                                                 | Implemented.                                                                              |
| Raw 8-bit GX color math                                                                                             | Implemented.                                                                              |
| Fighter animation (every animation that binds whole-body; the idle loop on the site)                                | Implemented.                                                                              |
| Stage load-time joint and texture animation                                                                         | Implemented.                                                                              |
| Emboss bump, hilight/shadow/toon coordinates                                                                        | Not implemented. Emboss stages are skipped; other unsupported coordinates fail the model. |
| Point/spot and alpha lights, JObj pass gates, billboards, IK, parent-scale compensation, quaternion joint rotations | Not implemented; see the known issues.                                                    |
| PATH (spline) tracks, RObj constraints, and ShapeAnim                                                               | Not implemented. A joint driven by one holds still.                                       |

## Contracts

### `HsdScene`

The renderer reads the scene the parser builds; there is no second copy and no
serialized form.

- **Roots and joints.** `parent` describes the owned JObj tree. `children` is
  the draw graph: owned children, or exactly one referenced target for an
  INSTANCE joint. Consumers must not rebuild ownership from `children` or load
  a target through an INSTANCE edge.
- **Display objects** resolve their own state: `pixel_engine()`, `pass()`, and
  `channels()`. Materials carry colors and ordered TObjs, each with canonical
  `coordinates()`, `light_map()`, `color_map()`, `alpha_map()` and
  `is_bump()`. Raw render flags, TObj flags and texgen selectors stay as
  provenance; consumers call the resolvers instead of re-deriving them.
  `HsdSource` refuses a scene with a display object that has no pass.
- **Polygons** carry decoded vertices, triangles, their binding (rigid or
  envelope), their `cull()` mode, and `tex_coord_attribute_mask()`, so a
  missing attribute never becomes a plausible zero UV.
- **Textures** carry decoded RGBA, deduplicated by descriptor identity;
  `content_key()` names the pixels, which several descriptors can share.

### Evaluated draw work

Per root: joint world matrices plus world-space positions, normals, binormals,
and tangents. Packets are ordered draw _occurrences_: a source polygon can
repeat (INSTANCE), each occurrence with its own contiguous vertex range. Do not
deduplicate packets. The renderer does not read binormals and tangents; they
are there for emboss bump mapping.

### Changing a contract

Migrate every caller in the same change (the renderer, the WASM adapter, the
editor); the compiler finds them.

## Source-backed semantics

References are to the [Melee decompilation](https://github.com/doldecomp/melee).

- **Draw passes.** `DObjLoad` (`dobj.c`) classifies the MObj render mode:
  neither XLU nor NO_ZUPDATE is opaque, XLU alone is texEdge, XLU with
  NO_ZUPDATE is translucent, and NO_ZUPDATE alone panics (rejected here).
  Render passes run opaque, texEdge, translucent (`HSD_GObj_804085F0` in
  `gobj.c`).
- **Color channels.** `HSD_SetupChannelMode` (`state.c`) lights COLOR0 only for
  exactly DIFFUSE, passes vertex color only for exactly VERTEX, and otherwise
  outputs white. Lit COLOR0 is `clamp(material ambient × ambient light + Σ light
× max(N·L, 0))`. Specular COLOR1 uses HSD's attenuation
  `x² / (k0 + (1 − k0) x²)`, `k0 = shininess / 2`, with a half-vector per joint
  (`lobj.c`).
- **TEV.** `MObjMakeTExp` (`mobj.c`) starts from material constants, vertex
  color, or white; applies diffuse/ambient light-map stages; multiplies lit
  COLOR0; builds the specular chain (specular stages, × COLOR1, added); then
  applies EXT stages. `TObjMakeTExp` (`tobj.c`) defines each color and alpha
  map, and applies a multi-phase stage's alpha map only once.
- **Texture coordinates.** `MakeTextureMtx` (`tobj.c`) builds `S × R × T` with a
  `1e-10` scale threshold and a mirrored-V offset. Reflection rows become
  `[0.5·m0, −0.5·m1, 0, 0.5·m0 + 0.5·m1 + m2 + m3]` over normalized
  camera-space normals. Backends keep full STQ and divide per fragment; they
  reject zero or non-finite Q and Q sign crossings within a triangle.
- **Color math.** GX samples, combines, blends, and writes raw 8-bit values;
  the display interprets them as sRGB. `hsd-render` does the same, with no sRGB
  decode or encode.
- **PE.** `HSD_SetupPEMode` (`state.c`) resolves blending, depth, and alpha
  compare from the render mode, or from the MObj's PEDesc when it has one.
  The scene carries all of it, including the logic op of a logic blend.
  hsd-render lowers every blend factor, depth compare, and alpha test, and
  a COPY logic op as a plain write; it refuses the other logic ops, dither,
  and the destination-alpha override, which wgpu cannot express.

## Working rules

- Validate hostile input before dereferencing, and bound every traversal,
  collection, and payload.
- An unsupported state on an applied path fails the model with a structured
  error rather than rendering a guess. Exceptions are deliberate and documented
  (emboss stages are skipped).
- Rendering changes run the pixel-regression check against a clean Melee NTSC
  1.02 disc image
  (`TGG_MELEE_ISO=melee.iso cargo run -p hsd-render --release -- regression --software`):
  predict the changed cases, inspect the captures, then record with `--update`
  in the same PR. A stable hash is not evidence of Melee fidelity; the Melee
  decompilation and the game itself are the authority.
- Tests build their DAT bytes by hand. The few that need the game read the
  same disc image, behind the `melee-iso` feature of `melee-dat` and
  `tgg-editor`; CI compiles them and cannot run them.
- CI runs `cargo fmt --check`, `cargo clippy -D warnings` (native, wasm, and
  the `melee-iso` targets), and `cargo test --workspace`.

## Disc images

GameCube ISO/FST handling (`crates/gc-iso`) is a separate boundary. It reads
exact file extents and never parses HAL descriptors. The raw DAT parser starts
from an immutable byte slice and never opens a disc image. Applications may
depend on both and pass exact bytes and provenance between them.
