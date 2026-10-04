# tgg-editor

The textures.gg desktop app, on [gpui-ce](https://github.com/gpui-ce/gpui-ce).
It finds the player's Melee NTSC 1.02 ISO, shows every fighter and stage in it
moving, keeps a library of skins, installs them into the ISO, and edits a
costume's textures and colors with the result drawn live.

```bash
cargo run --release -p tgg-editor
cargo run --release -p tgg-editor -- PlFcRe.dat --iso path/to/melee.iso
```

It runs on Linux and macOS. Windows is not supported.

## Using it

The app opens on the player's game: a roster of fighters and stages, each with
its slots. A fighter's Costumes pane lists its costumes; its Shared pane lists
what every costume shares, from its fighter file (`PlFc.dat`) and the effects
file it may share with others (`EfFxData.dat`, Fox and Falco's). Where the app
knows those files, each model is a row of its own with its picture, such as
Falco's Laser and Shine, and says whether the installed file changes it;
choosing one plays the move that shows it on the costume on stage. Whatever
plays a move, that row or the Moves pane, draws what it spawns on the fighter
as the game does: the shine at the hip, the blaster in the hand, lasers fired
from it. Any other file is one row. While a fighter file is installed over,
costumes preview with the original the install kept. The library holds skins
added from files or zip archives, kept by content hash. Installing writes a
skin into its slot in the ISO and records what was there, so an install can be
undone back to the original file.

Edit textures opens a slot as a document. The model takes the window, and the
Textures, Texture, Moves and Colors panes float over it. Textures import from
and export to PNG, by menu, by dropping a file on the window, or through an
external image editor that the app watches for saves. Vertex and material
colors are edited as swatches. Save keeps the edit in the library; Save and
install also writes the slot. Shared models can't be edited yet.

## How it is built

The viewport is `hsd-render` drawing on gpui's own wgpu device, composited as a
surface. `melee-dat` supplies the model and its animation,
reading the fighter files a costume plays with from the player's game and
admitting each by the SHA-256 its catalog records. `dat-edit` owns every write
to a DAT, `gc-iso` every write to the ISO, and `tgg-ui` the theme and
components.

- Settings live in `textures.gg/editor.json` under the platform's config
  directory.
- Skins and install history live in `textures.gg/` under the platform's data
  directory.
- Diagnostics, and any panic with its backtrace, go to
  `textures.gg/logs/editor.log` there.

## Network

The app works offline and sends nothing on its own, with two
exceptions the player controls:

- Report a Problem (the Help menu or Settings, and offered at the launch after
  a panic) shows the app's version, the system and the end of the log, then
  copies it or sends it to textures.gg.
- At launch it reads `https://assets.textures.gg/editor/latest.json`
  (`{"version": "0.2.0"}`) and links to the download page when that is newer.
  It installs nothing, and Settings turns the check off.

`TGG_API_URL` and `TGG_UPDATE_URL` point these at another API or file.

## Tests

The editor's flows (opening, click-to-select, imports, undo, the
unsaved-changes prompt, installs) run through gpui's test app on a model and a
disc image written by hand. What only a real fighter shows (named textures,
moves, scrubbing) reads a clean Melee NTSC 1.02 disc image:

```bash
cargo test -p tgg-editor
TGG_MELEE_ISO=melee.iso cargo test -p tgg-editor --features melee-iso
```

`--exit-after SECONDS` quits on a timer, for measuring headless runs, and
`--stress-test` animates every costume in the game at once.
