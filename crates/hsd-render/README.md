# hsd-render

A wgpu renderer for HSD models: it draws the `HsdScene` and
`HsdEvaluatedDrawWork` that `dat-parser` produces, lowering GX materials,
TEV stages, color channels and pixel-engine state to WGSL and wgpu pipelines.

`HsdRenderer` borrows a caller's `wgpu::Device` and `Queue` and encodes into
a caller's target view, so an offscreen capture, a browser canvas and a gpui
surface can host it alike. The library knows nothing about a particular game;
a caller poses a model (with `melee-dat`, for Melee) and hands each frame
here. wgpu is pinned to gpui-ce's version so the editor can share its device.

It draws the textures.gg site's 3D preview (compiled to WebAssembly) and the
editor viewport. The pixel baseline (`pixel-baseline.json`, here) is a
regression check, not an authority: the Melee decompilation and the game
itself in Dolphin are.

## Commands

Run from the repository root inside the dev shell. The commands that need the
game read a clean Melee NTSC 1.02 disc image: `--iso PATH`, else the one
`TGG_MELEE_ISO` names.

```bash
# Bind-pose captures of one DAT, all four fixed views, 642x528.
cargo run --release -p hsd-render -- capture path/to/PlFcNr.dat --out /tmp/hsd

# Pixel regression: render every case in pixel-baseline.json (10 fighters from
# four sides, all 71 stages 90 frames in) and compare pixel hashes. The files
# are found on the disc by hash. --update records intended changes;
# --reference DIR adds diffs.
cargo run --release -p hsd-render -- regression --software

# Play one catalog Wait1 cycle; the frame after the loop reset must be
# byte-identical to the first frame.
cargo run --release -p hsd-render -- idle path/to/PlFcNr.dat \
  --out /tmp/hsd-idle --every 30
```

A recognized fighter costume draws only the model parts the game draws: its
alternate faces, hands, items, and detail levels are hidden exactly as
`ftparts.c` hides them, from tables read out of the fighter data
(`melee_dat::fighter::parts`). Costume roots in an expansion slot the
game doesn't have play as costume 0, as the game clamps them. `melee-dat`'s
`melee-iso` tests check those masks against the catalog's for every stock
costume.

`capture --policy melee-fighter` evaluates like the site does for fighter packs
(the default is generic HSD); `idle` always uses the MeleeFighter policy.
`--software` selects the CPU fallback adapter (lavapipe) instead of the GPU.
Captures are deterministic per adapter: repeated runs are byte-identical.
Two adapters do not hash alike, because rasterizers differ at silhouette edges
and coplanar decals; the baseline records the adapter it was made on.
