# tgg-next (proof of concept)

The textures.gg editor's Catalog design (`../mocks/b.html`) on
[Rune](https://github.com/anomalyco/rune), proving the one thing a port
depends on: hsd-render drawing a real fighter on Rune's own GPU device, inside
Rune's layout.

```bash
direnv exec ~/dev/texturesgg/core cargo run --release -- --iso path/to/melee.iso
# headless: render a PNG, optionally from a scene (orange, side)
direnv exec ~/dev/texturesgg/core cargo run --release -- --iso path/to/melee.iso --snap out.png --scene orange
```

`TGG_MELEE_ISO` works in place of `--iso`. It needs a clean NTSC 1.02 disc.

## How the viewport works

- The view places `effect(VIEWPORT)` like any box; Rune lays it out and
  hit-tests it (dragging it orbits).
- `viewport::Viewport` is the app's `Compositor`. Each frame it finds the
  slot's rect, has hsd-render encode the fighter into a texture of exactly that
  size (its own pass and depth buffer, recorded ahead of Rune's), then copies
  it into the slot from Rune's effect hook, so the slot's clip and painter
  order apply.
- Animation advances on Melee's 60 Hz clock from the frame's `dt`, capped at
  four ticks after a stall. A different costume reloads from the disc.
- Its own Cargo workspace: Rune pins wgpu 30, so this branch moves hsd-render
  to wgpu 30 (pixel baseline: 115/115 match). tgg-editor on this branch still
  pins 29 and doesn't build here.
