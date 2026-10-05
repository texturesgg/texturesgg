# textures.gg

The Rust behind [textures.gg](https://textures.gg): a parser and renderer for
the model format of Super Smash Bros. Melee, and a desktop app that installs
skins into a player's game and edits their textures.

HAL's HSD library stores Melee's fighters and stages in `.dat` archives. These
crates read them, pose and animate them as the game does, draw them with wgpu,
and write texture and color edits back in place.

## Crates

```text
crates/
  hal-dat-raw/   HSD DAT archives: bytes, relocation, roots
  gx-texture/    GameCube texture codec: decode, encode, palettes, CMPR
  gc-iso/        GameCube disc images: read a file, replace a file
  dat-parser/    HSD descriptors, scenes, draw evaluation, animation
  melee-dat/     Melee on top of dat-parser: fighters, stages, their animation
  dat-edit/      In-place texture and color edits to a DAT
  hsd-render/    wgpu renderer for HSD models
  tgg-mod/       Code-mod packages for tgg-melee: manifests, catalogs, installs

  tgg-cli/       The `tgg` command line: build, publish and install code mods
  tgg-ui/        The desktop app's theme and components, on gpui-ce
  tgg-editor/    The desktop app
```

The first eight are libraries; all but tgg-mod are published on crates.io.
Each has a README that says what it owns and what it leaves to the others.

- [`docs/DAT_LAYERING_ARCHITECTURE.md`](docs/DAT_LAYERING_ARCHITECTURE.md): how
  a DAT becomes pixels, layer by layer.
- [`docs/RENDERER_KNOWN_ISSUES.md`](docs/RENDERER_KNOWN_ISSUES.md): where a
  preview differs from the game.
- [`docs/hal_dat/`](docs/hal_dat/README.md): a reference for the HSD file
  format and its animation data.

## Development

`flake.nix` pins the toolchain; with Nix and direnv, entering the directory
loads it. Without them, a stable Rust toolchain and the libraries gpui-ce needs
(see `.github/workflows/ci.yml` for the Debian package names) are enough.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p tgg-editor
```

No game files are in this repository, and none are needed to build or to run
the tests above. A few tests and the renderer's pixel regression read a clean
Melee NTSC 1.02 disc image that you supply:

```bash
TGG_MELEE_ISO=melee.iso cargo test -p melee-dat -p tgg-editor \
  --features melee-dat/melee-iso,tgg-editor/melee-iso
TGG_MELEE_ISO=melee.iso cargo run --release -p hsd-render -- regression --software
```

## Contributing

- The parsers read untrusted files: validate before dereferencing, bound every
  traversal, and fail a model with an error instead of drawing a guess.
- No game files: no DAT, ISO or executable bytes, and no extracted animation.
- Tests build their bytes by hand, and each is named for the rule it protects.
- A rendering change runs the pixel regression above: predict the cases that
  change, look at the captures, and record them with `--update` in the same
  change.
- The renderer and the crates beneath it also build for
  `wasm32-unknown-unknown`; the site runs them in the browser.

## License

GPL-3.0-or-later. See [`LICENSE`](LICENSE). The bundled fonts are under the SIL
Open Font License; their license files sit beside them in
`crates/tgg-ui/assets/fonts/`.

Parts of this work build on others': `gx-texture` follows Dolphin's texture
decoder and adapts its tile walk from libWiiSharp (see its README), and the
structure tables in `docs/hal_dat/` derive from MKWiiki (see that folder's
README). What the game does was learned from the
[Melee decompilation](https://github.com/doldecomp/melee).

Super Smash Bros. Melee is Nintendo's and HAL Laboratory's. This project is not
affiliated with either.
