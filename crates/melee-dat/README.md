# melee-dat

Super Smash Bros. Melee on top of [`dat-parser`](../dat-parser), which reads
HAL's HSD format but knows nothing about a particular game.

`MeleeModel` is the way in. Load any of the game's DATs and it is a fighter,
a stage, or a model that draws in the pose it was saved in. Advance it,
evaluate it, and hand the scene and draw work to a renderer such as
[`hsd-render`](../hsd-render). Nothing here touches a GPU.

```rust
use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use melee_dat::MeleeModel;

let mut model = MeleeModel::open(&bytes, HsdDrawEvaluationPolicy::MELEE_FIGHTER)?;
model.advance()?;                      // one 60 Hz tick; a stage's animations move
let (scene, work) = model.evaluate()?; // what a renderer draws this frame
```

A stage plays as soon as it loads. A fighter costume needs the fighter's own
files to move, and the `MELEE_FIGHTER` policy it was opened with above:
`MeleeModel::attach_fighter` takes the reference catalog
(`MeleeReferenceCatalog::checked_in()`) and a `MeleeReferenceStore` of original
game files, which the caller supplies and the store admits only by size and
SHA-256. It hands the model back with a `FighterAttachOutcome`: attached, not a
fighter the catalog recognizes, or failed with the `MeleeError` that says why.

The game's own numbers and names have types: `FighterKind` and `CostumeIndex`
for the fighter tables, `StagePointKind` for a stage's general points, and
`MeleeSlot` (a `Character` and `CostumeColor`, a `Character`'s data file, an
`Effects` file, or a `Stage`) for the file a skin replaces. `parse_filename`
finds the slot a descriptively named file was made for;
`MeleeSlot::from_file_name` takes only the slot's own name. `SharedModel` is a
model every costume of a fighter shares (Fox's laser, his shine): where it
lives in its data or effects file, whether it still draws as shipped, and how
the game shows it: spawned on the fighter as a move plays (`SpawnedModel`, held
to a fighter part as the game's constraints hold it) or fired from it at the
frames the move's script says (`Firing`, `Shot`).

## Layout

| Module                 | What it holds                                                                                           |
| ---------------------- | ------------------------------------------------------------------------------------------------------- |
| `model`                | `MeleeModel`: fighter, stage, or static, behind one interface.                                          |
| `fighter::playback`    | `MeleeFighterPlayback`: the idle on attach, then any of the fighter's animations.                       |
| `fighter::animation`   | Binding a fighter's animation archive to its joints, as the game does.                                  |
| `fighter::parts`       | Which model parts the game shows (faces, hands, items, detail levels).                                  |
| `fighter::moves`       | What each animation is called as a move ("Jab 1", "Up smash").                                          |
| `fighter::places`      | Where a stock costume draws each texture ("Head", "Eyes").                                              |
| `fighter::shared`      | `SharedModel`: what costumes share (a laser, a shine), named, fingerprinted, spawned and fired.         |
| `fighter::script`      | Move scripts read for the frames they act on (when a laser fires).                                      |
| `stage::points`        | A stage's general points: camera range, blast zone, spawn points.                                       |
| `stage::playback`      | `MeleeStagePlayback`: the joint animations a stage starts when it loads.                                |
| `stage::texture_names` | HAL's own names for a stage's textures.                                                                 |
| `catalog`              | `MeleeReferenceCatalog`: the checked-in table of every fighter's files, joint hierarchy and idle setup. |
| `references`           | `MeleeReferenceStore`: the original game files a caller supplies.                                       |
| `file_names`           | `MeleeSlot` and how the game names its files (`PlFcRe.dat` is Falco's Red, `PlFc.dat` his data).        |
| `vanilla`              | The size and SHA-256 of every slot's file as shipped, and each shared model's fingerprint.              |

Types a host holds carry the `Melee` prefix; their parts are named by subject
(`Stage…`, `Fighter…`, `Costume…`).

## What is not here

No game files. The catalog and the vanilla table hold names, sizes, hashes and
source-derived constants only. Tests that need the game read a clean NTSC 1.02
disc image the developer supplies:
`TGG_MELEE_ISO=melee.iso cargo test -p melee-dat --features melee-iso`.

Known gaps in what plays are listed in
[`docs/RENDERER_KNOWN_ISSUES.md`](https://github.com/texturesgg/texturesgg/blob/main/docs/RENDERER_KNOWN_ISSUES.md).
