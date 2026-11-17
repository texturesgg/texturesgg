# Sets on the player's machine

A set is a name and a list. Each line is a mod or a textures.gg set it builds on. The
player picks one and plays; the gameplay mods that load decide who they meet. This is
what `tgg_mod::sets` keeps and runs, and the calls the app makes for each part of Play
and the Mods page. The design behind it is `tgg-melee-goals/sets-model.md`.

Every call is blocking, like the rest of the crate: run it off the UI thread. `data` is
`tgg_mod::default_data_dir()`, `mods` is `tgg_mod::default_mods_dir()`, and `local` is
`tgg_mod::local(&mods, port)` for the current port.

## On disk

`<data>/tgg/sets/<key>.json`, one per set, the key made from its name
(`friday-night.json`):

```json
{
  "format": 1,
  "name": "Friday night",
  "lines": [
    { "set": "competitive" },
    { "mod": "tgg.prevent-wobbling" },
    { "mod": "tgg.ucf", "version": "^0.84" },
    { "mod": "tgg.quick-start", "left_out": true }
  ],
  "resolved": { "tgg.prevent-wobbling": "1.0.0", "tgg.ucf": "0.84.0" }
}
```

- A set line (`set`) names a textures.gg set by its link name and follows it as it is now.
- A mod line (`mod`) takes `latest` when `version` is absent, else an exact version or a
  range. `left_out` keeps a mod from loading, whether it's the set's own or comes from a
  set it builds on.
- `locked`: a textures.gg set the player added (`sets::get`). One set line; it can't be
  changed, only built on.
- `resolved`: what the last launch used, for launching offline.

Beside it, `<key>.settings.json` is the set's own mod settings: the game reads and saves
them there while it runs from the set (`TGG_SETTINGS`). A new set starts from each mod's
defaults. `<data>/tgg/sets/.active` holds the active set's key.

The game's data folder is shared by every set: saves, the memory card and `skins.json`.

## Installed mods

The player's mods folder is the built-in set `sets::INSTALLED` ("Installed mods"):

- No file. `sets::list` puts it first, built from `local`: each mod at its installed
  version, left out when it's off. Hide it when it has no lines.
- It's the active set until the player picks another.
- It runs the way the game always has: `check_withdrawn`, then `ports::launch` with the
  mods folder. Not through `sets::plan`.
- It changes the way the Mods page always has: `apply`, `set_enabled`, `uninstall`.
  `sets::save` and `sets::rename` refuse it.
- "Save as a new set" is `sets::from_installed`.

## The calls

| In the app | Call |
|---|---|
| List the sets for the picker | `sets::list(data, &local)`, `sets::active(data)` |
| Use a set | `sets::activate(data, key)` |
| New set (builds on Online play) | `sets::new(data, name)`, then `sets::build_on(net, data, &local, key, sets::ONLINE_PLAY)` |
| New set with nothing in it | `sets::new(data, name)` |
| First run, several picks | `sets::new`, then `sets::build_on` for each pick |
| Add a textures.gg set as it is | `sets::get(net, data, "competitive")` |
| Make your own (build on a set) | `sets::build_on(net, data, &local, key, link)` |
| Add a mod | `set.add(id, Want::Latest)`, `sets::save(data, &set)` |
| Hold a mod to a version line | `set.add(id, Want::Range(..))`, `sets::save` |
| Take a line out | `set.drop_line("tgg.ucf" or "competitive")`, `sets::save` |
| Turn a mod off ("Left out") | `set.leave_out(&id, true)`, `sets::save` |
| Save Installed mods as a set | `sets::from_installed(data, &local, name)`: the set, and the mods left out because they aren't on textures.gg |
| Copy, rename, delete | `sets::copy`, `sets::rename` (returns the set under its new key), `sets::remove` |
| The Mods page's groups, the queue line | `sets::plan(net, data, mods, &set, sets::skins_on())` |
| Play | `sets::plan`, trust, `sets::prepare`, `launch_profile` (below) |

`sets::load(data, &local, key)` reads one set. `Want` parses `latest`, `1.2.0` and `^1.2`.

### Building on

- A set builds on up to 4 sets (`Error::TooManySets`).
- Only a textures.gg set that lists only mods can be built on: `Set::can_build_on`, or
  `Error::CantBuildOn` (offer to build on the same sets instead).
- When there's no Online play set on textures.gg yet, `build_on` gives `Error::NoSuchSet`:
  make the set without it and say "textures.gg has no Online play set yet".
- A locked set (`set.locked`) gives `Error::SetLocked` on any change: offer "Make your own",
  which is `sets::new` and `build_on` with the locked set's link name.

`GET /api/registry/sets` gives each set an optional `owner` (`{id, name, official}`; absent
means textures.gg) and `builds_on` (link names; absent means none).

### What a set resolves to

`sets::plan` returns a `SetPlan`:

- `mods`: every mod, each with
  - `group`: `FromSet { set, title }` ("From Competitive"), `Added`, `LeftOut`, `Skins` (the
    skin loader, there because the player has a skin on), `Needed` (a dependency nobody
    listed);
  - `version` and `class` when it loads: `Gameplay` ("Has to match in netplay"), `Display`
    ("Only you see it"), `Offline` ("Offline only");
  - `skipped`: why it doesn't load, in the player's words ("textures.gg withdrew
    tgg.turbo 1.2.0: crashes on Windows", "tgg.turbo 1.2.0 isn't built for this game",
    "the sets it comes from take versions no release has in common").
- `gameplay()`: the mods that decide its queue, `[{id, version}]` by id, the same shape as a
  queue's `mods` in `GET /v1/queues`. Equal lists search together ("Searches in
  Competitive"); otherwise "Has its own queue".
- `offline`: set when textures.gg can't be reached. The plan then uses `resolved`, from the
  package cache. A set never played before needs a connection once.
- `notices`: things to say once, such as "textures.gg took Competitive down, so its mods
  are now part of Friday night" (its mods became the set's own lines).
- `plan`: the profile plan underneath, for the game version, sizes and trust.

How it resolves: each mod loads once. The set's own line wins (its version, or left out).
When two sets it builds on take different versions, the newest release both accept; with
none, the mod is skipped and says so (the app can offer a pick, which becomes the set's own
line). Online, every mod takes the newest approved release that works with the current
game. A mod that's withdrawn, not built for the game or unknown is skipped with the reason,
and the set still plays. Skins aren't lines: pass `sets::skins_on()` and `tgg.skins` is
added for the session.

Calling `sets::plan` fetches the sets list and the catalog (both cached for offline), so
call it when a set is opened or Play is pressed, not per frame.

### Play

```rust
let mut plan = sets::plan(net, data, mods, &set, sets::skins_on())?;
// Show plan.notices, and "Playing without X: why" for each of plan.skipped().
// Native code from a publisher the player hasn't trusted asks first; scripts never do.
for publisher in plan.plan.untrusted() { /* ask; on yes: */ }
plan.plan.trust_for_session(&yes_ids);           // or remember_trust(data, publisher, true)
let prepared = sets::prepare(net, &plan, data, &set, &progress, &cancel)?;
let running = tgg_mod::launch_profile(&prepared.port, &iso, &prepared, None, log)?;
```

`prepare` fetches what isn't in the package cache, builds the set's folder under
`<data>/tgg/profiles/`, points the game at the set's settings file and writes `resolved`.
Switching sets is this again with another set: nothing is installed or uninstalled.

### While a set's game is running

A set's game is a profile game: `tgg_mod::profile_games(data)` lists it, and
`launch_profile` refuses a second one (`Error::GameRunning`). Its mods can't change until it
restarts, so while it runs the Mods page shows the set read-only with a line like "Comp is
running. Changes apply next time you play." Edits to the set's file are still fine (they
take effect at the next launch). When the game ends, the folder is removed; a front end
that lost the `Running` handle calls `tgg_mod::finish_profile(data)`.

## The command line

`tgg set list|get|new|save|show|add|drop|on|off|copy|rename|remove|use|run` does the same
through these calls (`crates/tgg-cli/README.md`). `tgg port run` starts the active set.
