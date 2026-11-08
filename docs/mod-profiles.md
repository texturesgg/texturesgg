# Mod profiles: playing with exactly the mods a game needs

A replay plays back correctly only with the game and mods it was played with. A netplay
session only starts when both games run the same mods. A tournament wants every setup
identical. This design gives them one answer: the **mod profile**, an exact list of mods
(and the game they need), which can be launched no matter what the player has installed.
It builds on [mod distribution](mod-distribution.md): the registry, `tgg-mod`, sets and
the lock.

## The player's view

- **Watching a replay.** The player picks a replay, in the replay browser, the app or
  `tgg`. If its game used mods (or a tgg-melee version) the player doesn't have, they're
  fetched and the replay plays with exactly those. The player's own mods and current game
  version are untouched. If something can't be had, the replay says so up front and offers
  to play anyway ("may not match").
- **Joining a netplay game.** If the host runs mods the joiner doesn't have, the joiner is
  offered the host's profile instead of the session ending.
- **Playing normally.** Nothing changes: the installed mods are the player's own profile.

## Profiles

```json
{
  "format": 1,
  "label": "Competitive v3",
  "game": { "name": "tgg-melee", "version": "0.1.0-rc.9" },
  "mods": [
    { "id": "tgg.neutral-spawns", "version": "0.1.2", "sha256": "…" },
    { "id": "tgg.competitive-rules", "version": "0.1.2", "sha256": "…",
      "settings": { "stocks": 4, "stock-time": 8 } }
  ],
  "missing": [ { "id": "tgg.example", "version": "1.0.0", "why": "unknown" } ]
}
```

- `game`: the tgg-melee version the profile's mod builds are for. A profile from a
  tgg-melee replay names the version it was recorded on. A profile from a Slippi replay
  has `"version": "any"` and resolves against the player's current port.
- `mods`: id, exact version and, when known, the package's SHA-256 for this platform.
  Two platforms' builds of one release differ, so a profile that travels between machines
  (a replay, a netplay hello) names versions, and each machine resolves them to its own
  build.
- `settings`: the values of the mod's counted settings (the ones in the mod-set id).
  Settings a player keeps to themselves (`TGG_SETTING_*_LOCAL`) never appear.
- `missing`: what the source asked for that can't be resolved, and why.
- `label`: optional display text ("Competitive v3", "Slippi 3.19 online"). Nothing reads
  it to decide anything.

The port's built-in modules (UCF, unlocks) are part of the game; their on/off state is a
counted setting of the game and travels the same way.

## Where profiles come from

| Source | How it becomes a profile |
|---|---|
| The player's installs | the lock and the mods folder (`tgg_mod::local`) |
| A replay recorded on tgg-melee | `metadata.tgg.profile` in the .slp, written by `tgg.replay-recorder` |
| A Slippi replay | its recorded code list, through the registry's Slippi mapping |
| A netplay host | its hello carries its profile (ids, versions, counted settings) |
| A set | resolved against a catalog, as `plan_set` does today |
| A file | `profile.json`, for tournaments or sharing |

### Slippi replays

A Slippi replay records the gecko codes its console ran and Slippi's version. The
registry keeps a **Slippi mapping**: for each code (and a Slippi version range where it
matters), the mod and version that reproduces it. Each rule mod declares what it
reproduces in its manifest, so the mapping is data its author maintains:

```json
"slippi": [ { "code": "0x801D45EC", "name": "Preload Stadium Transformations",
              "versions": "<3.19" } ]
```

The registry builds the mapping from approved releases and serves it
(`GET /api/registry/slippi`), with a list of codes that don't affect gameplay. A replay's
profile is the mods mapped from its codes, plus the settings its Game Start implies
(frozen Stadium, UCF version), with every unmapped gameplay code under `missing`. This
replaces `rules.py` in the replay harness, which becomes a client of the same mapping.

## Formats

### Profile files

The full form of a profile, as `profile.json`, the netplay hello and `metadata.tgg.profile`
carry it. Every field but `format` and `game` may be left out.

```json
{
  "format": 1,
  "label": "Slippi 3.18 online",
  "game": {
    "name": "tgg-melee",
    "version": "0.1.0-rc.9",
    "layout": "a1b2c3d4e5f60718",
    "settings": { "tgg.ucf": true, "tgg.unlocks": true }
  },
  "mods": [
    { "id": "tgg.neutral-spawns", "version": "0.1.2", "sha256": "9f86d0…" },
    { "id": "tgg.competitive-rules", "version": "0.1.2",
      "settings": { "stocks": 4, "stock-time": 8, "items": "Off" } },
    { "id": "tgg.stadium-preload", "range": "^0.1" }
  ],
  "missing": [
    { "why": "unknown", "id": "tgg.example" },
    { "why": "not-built", "id": "tgg.example2", "version": "1.0.0" },
    { "why": "unapproved", "id": "tgg.example3", "version": "0.3.0" },
    { "why": "no-mod-for", "code": "0x801D24FC" }
  ],
  "slippi": { "version": "3.18.0", "codes": ["0x8016E510", "0x801D45EC", "frozen-stadium"] }
}
```

- `format`: 1. A reader refuses a larger number.
- `game.name`: `tgg-melee`. `game.version`: a release, or `any` (the player's current
  port). `game.layout`: the game layout id (`Port::game_abi`) when the source knows it, so
  a front end can fetch the catalog the profile's mods are built for before that port is
  installed.
- `game.settings`: the on/off state of the port's built-in modules, by module id
  (`tgg.ucf`, `tgg.unlocks`); a module left out runs as the port defaults (on). A module
  that is off is started with its environment variable off (`TGG_UCF=0`,
  `TGG_UNLOCK_ALL=0`).
- `mods[].version` is exact. A mod may give `range` instead (one of the ranges a manifest's
  `depends` takes): the newest approved release in it. Profiles from a Slippi replay do,
  since the mapping names ranges. `sha256` is the package for the platform the profile was
  written on; another platform ignores it.
- `mods[].settings`: counted settings by key, as `settings.json` holds them (a choice is
  its text).
- `missing[].why`: `unknown` (no such mod), `not-built` (not built for the profile's game;
  `version` is the version or range asked for), `unapproved` (no approved release at that
  version), `no-mod-for` (a Slippi code nothing reproduces).
- `slippi`: the facts read from a Slippi replay: its Slippi version and every code its
  console ran, as the mapping names codes (below). `resolve_profile` maps these again
  with the mapping it's given, so a profile read with an old mapping (or none) resolves
  with the newest. Mods mapped from codes are listed in `mods` as well, for display.

A replay's profile also names `tgg.replay-player` (`"range": "*"`) when its source
doesn't, since playing the replay takes it.

### `metadata.tgg.profile` in a .slp

`tgg.replay-recorder` writes the game's profile into the replay's UBJSON `metadata`, beside
the `game`, `modSetId` and `mods` it writes today:

```text
metadata: {
  ..., "playedOn": "tgg-melee",
  "tgg": { "game": "tgg-melee 0.1.0-rc.9 a1b2c3d4e5f60718",
           "modSetId": "…", "mods": ["tgg.ucf 0.84.0", "tgg.neutral-spawns 0.1.2"],
           "profile": { …the profile as JSON… } }
}
```

`profile` is a UBJSON object holding exactly the JSON above (objects `{`, arrays `[`,
strings `S`, integers `i`/`U`/`I`/`l`/`L`, `T`/`F`, `Z`); a UBJSON string holding the
JSON text is read too. The recorder writes: `format` 1, `game` with `name`, `version`,
`layout` and the built-in modules' state as `settings`, and every mod that counts toward
the mod-set id (built-in modules aside) with its exact `version`, its package `sha256`
when it has one, and its counted `settings`. No `missing`, `slippi` or `label`.

Without `profile`, a tgg-melee replay's profile is read from `game` (`"<name> <version>
<layout>"`) and `mods` (`"<id> <version>"`, built-in modules giving `game.settings`: listed
means on, left out means off), with no settings.

### The Slippi mapping: `GET /api/registry/slippi`

```json
{
  "format": 1,
  "snapshot": "2026-11-07T18:00:00Z",
  "codes": [
    { "code": "0x8016E510", "name": "Neutral spawns", "versions": "*",
      "id": "tgg.neutral-spawns", "range": "^0.1" },
    { "code": "0x801D45EC", "name": "Preload Stadium Transformations", "versions": "<3.19",
      "id": "tgg.stadium-preload", "range": "^0.1" },
    { "code": "0x800908F4", "name": "UCF 0.8 (Logic)", "versions": "*",
      "id": "tgg.ucf-legacy", "range": "^0.1", "game": { "tgg.ucf": false } },
    { "code": "0x8006B460", "name": "UCF 0.84", "versions": "*", "game": { "tgg.ucf": true } },
    { "code": "frozen-stadium", "name": "Frozen Pokémon Stadium", "versions": "*",
      "id": "tgg.frozen-stadium", "range": "^0.1" }
  ],
  "ignore": [
    { "code": "0x801A50AC", "name": "…", "versions": "*" }
  ]
}
```

- `codes[].code`: a code's address, `0x` and eight upper-case hex digits (the address a
  `C2` inject or an `04`/`06`/`00`-`03` write names), or a Game Start fact:
  `frozen-stadium` (the frozen Stadium flag on, on Pokémon Stadium) or `ucf-off` (every
  player's dashback and shield-drop fix off).
- `versions`: the Slippi versions the entry holds for, as a semver requirement
  (`*`, `<3.19`, `>=3.9, <3.17`). The first entry for a code whose `versions` match the
  replay's Slippi version wins.
- `id`, `range`: the mod and the releases that reproduce the code; left out when the game
  itself does (`game` alone). `settings`: counted settings of that mod the code implies
  (optional). `game`: built-in module states the code implies (optional).
- `ignore`: codes that don't change gameplay (display, netcode, menus); `versions` as above.
  A code a replay ran that no `codes` or `ignore` entry matches is `missing` as
  `no-mod-for`.
- `format` 1; `snapshot` names this state of the mapping. 404 reads as an empty mapping.

The registry builds `codes` from the `slippi` field of each approved release's manifest
(each release's id and the range of its releases that declare the code), plus entries for
what the game does itself; `ignore` is kept by the registry's admins.

### Catalog entries

Two optional fields on each `GET /api/registry/catalog` entry, beside `netplay` and
`package`:

- `publisher`: `{ "name", "official" }`, as the mods list gives it. Without it a mod is
  unofficial and published by "unknown".
- `changes_game`: whether the build changes how the game plays. Absent means `true`. The
  registry fills it from each build's synctest results later; until then every mod changes
  the game, so a replay plays as installed only when the installed mods are exactly its
  own.

### `tgg profile prepare <profile.json|replay.slp> --machine`

JSON lines on stdout, one object each, told apart by `event`:

```json
{"event":"plan","key":"3f9a…","fit":"needs-profile","game":{"version":"0.1.0-rc.8","state":"available","size":18000000},"mods":[{"id":"tgg.neutral-spawns","name":"Neutral spawns","version":"0.1.2","publisher":{"name":"textures.gg","official":true},"cached":false,"size":4096,"needs_trust":false}],"missing":[{"why":"no-mod-for","code":"0x801D24FC"}]}
{"event":"needs-trust","publishers":[{"name":"someone","mods":["someone.mod"]}]}
{"event":"progress","step":0,"steps":3,"label":"Downloading Neutral spawns","bytes":4096,"total":4096}
{"event":"done","fit":"needs-profile","folder":"/home/me/.local/share/tgg/profiles/3f9a…","settings":"/home/me/.local/share/tgg/profiles/3f9a…/settings.json","port":{"version":"0.1.0-rc.8","executable":"/home/me/.local/share/tgg/ports/0.1.0-rc.8/tgg-melee"},"env":{"TGG_MODS_DIR":"…","TGG_SETTINGS":"…","TGG_UCF":"0"}}
{"event":"error","message":"can't reach textures.gg; what's installed still works"}
```

- `plan` comes first. `game.state` is `installed`, `available` (`size` in bytes, or null
  when tgg hasn't seen that release) or `unavailable`. `fit` is `as-installed` or
  `needs-profile`.
- `needs-trust` ends the run (exit 0) when a mod needs the player's yes and its publisher
  wasn't allowed: the caller asks, then runs again with `--trust <publisher>` for each yes
  (`--remember` also stores the answers, as `remember_trust` does).
- `progress` lines follow while packages and the port download.
- `done` ends a run that worked. With `fit` `as-installed`, `folder` and `settings` are
  null and `port` is the current port: no restart needed. Otherwise `env` is what to start
  `port.executable` with (the disc image as its argument, as usual).
- `error` ends a run that failed (exit 1).

### On disk

Under `<data>/tgg/` (`<data>` is `$XDG_DATA_HOME`, `~/.local/share`, `~/Library/Application
Support` or `%LOCALAPPDATA%`, the folder `ports/` is in):

- `packages/<sha256>.zip`: the package cache.
- `cache/catalog-<layout>.json`, `cache/slippi.json`: the last catalog per layout and the
  last mapping, as served (with their `snapshot`).
- `profiles/<key>/`: a profile's mods folder: a folder per mod, `profile.json` (the
  resolved profile, with each package's `sha256`) and `settings.json` (its counted
  settings, laid out as the port's `settings.json`).
- `running.json`: `[{ "pid", "executable", "key", "folder" }]`, the profile games running.
- `trust.json`: `{ "publishers": { "<name>": true|false } }`.

### What the port reads

`launch_profile` starts the port with `TGG_MODS_DIR` set to the profile's folder,
`TGG_SETTINGS` to its `settings.json`, the built-in module variables for modules that are
off, and `TGG_REPLAY_PLAYER_FILE` for a replay. The port reads its settings from
`<data>/tgg-melee/settings.json` alone today: it needs to take `TGG_SETTINGS` as that
path when set (`host_settings_path`, as `host_mods_dir` takes `TGG_MODS_DIR`), so a profile
game reads and saves its own settings. Until it does, `launch_profile` also passes the
profile's settings as `TGG_SET`, which the port applies for the run without saving.

## The `tgg-mod` API

Blocking, like the rest of the crate; front ends run it on their own threads. Typed data
throughout.

```rust
// Reading: disk only, cheap enough to call per row of a replay list
pub fn profile_from_slp(path: &Path, mapping: Option<&SlippiMapping>) -> Result<Profile, Error>;
pub fn profile_from_local(local: &Local) -> Profile;
pub fn profile_from_file(path: &Path) -> Result<Profile, Error>;

// Registry: network, cached on disk (below)
pub fn slippi_mapping(net: &dyn Net) -> Result<SlippiMapping, Error>;

// Resolving: no network, no writes
pub fn resolve_profile(profile: &Profile, catalog: &Catalog, mapping: &SlippiMapping,
                       ports: &Path, trust: &Trust) -> ProfilePlan;
pub struct ProfilePlan {
    pub game: GameNeed,            // version, Installed(PortInstall) | Available{size} | Unavailable
    pub mods: Vec<ProfileMod>,     // id, version, publisher {name, official}, cached, size, needs_trust
    pub missing: Vec<Missing>,     // Unknown{id} | NotBuilt{id, version} | Unapproved{id, version} | NoModFor{code}
    pub key: String,               // hash of the resolved profile; names its folder
    pub fit: Fit,                  // AsInstalled | NeedsProfile
}

// Preparing: network and disk (fetches into the cache, builds the folder; installs the
// needed port version alongside the others without making it current)
pub fn prepare_profile(net: &dyn Net, plan: &ProfilePlan, data: &Path,
                       progress: OnProgress, cancel: &AtomicBool) -> Result<Prepared, Error>;

// Launching: runs that port version with the profile's folder for one session
pub fn launch_profile(port: &PortInstall, iso: &Path, prepared: &Prepared,
                      replay: Option<&Path>, log: Option<&Path>) -> Result<Running, Error>;

// Trust: answers remembered per publisher, shared by every front end
pub fn trust(data: &Path) -> Result<Trust, Error>;
pub fn remember_trust(data: &Path, publisher: &str, allow: bool) -> Result<(), Error>;
```

`fit` is `AsInstalled` when the player's current port and installed mods already satisfy
the profile, with any extra installed mods all ones that don't change the game (the same
rule as the in-game no-restart check below). Then a front end plays it with the normal
mods folder: no `prepare_profile`, no profile folder.

`resolve_profile` reports the game the profile needs. For a tgg-melee replay recorded on
rc.8 while the player is on rc.9, it says "needs rc.8: installed" or "available, 18 MB",
and `launch_profile` runs rc.8 for that session without changing the player's current
version.

## The package cache and offline data

- `tgg-mod` keeps every downloaded package in `<data>/tgg/packages/<sha256>.zip`, shared
  by profiles and by `apply`, which checks it before downloading. Reinstalls, version
  switches and offline reinstalls are then cheap.
- The last catalog per layout and the last Slippi mapping are cached on disk with their
  snapshots, so a replay list can mark "won't match" offline.
- Pruning: a package is removed when neither the lock nor a live profile names it and it's
  older than a few weeks.

## Profile folders and the profile game's lifecycle

- A profile's mods folder is `<data>/tgg/profiles/<key>/`, keyed by the hash of the
  resolved profile, so the same replay or set reuses one. It's built from the package
  cache by copying (never from the player's mods folder), so a running profile game can't
  lock the player's own mod libraries, on Windows least of all. Its settings are its own.
- A profile game is marked running globally (`<data>/tgg/running.json`, next to the mods
  folder's pid file), so Play, the Mods page and `tgg` all know one is up and refuse to
  change mods underneath it.
- When the profile game ends, the crate (through `Running`, or `finish_profile` for a
  front end that lost the handle) clears the running mark and deletes the folder unless
  it's still in use.

## From inside the game

The in-game replay browser (and a future in-game netplay lobby) can't launch a second
game, and the port does no package fetching or TLS of its own. Each port release ships
the `tgg` binary beside the game, and the port gets one call:

```c
/* Restart into a profile: prepare it with the helper, then restart the game with its
 * folder, and after the restart give `then` to the asking mod (a `restarted` event).
 * Presentation code only. */
int tgg_restart_with(const char* profile_json, const char* then);
```

The game loads its mods once, at start, so a profile that needs a mod the running game
doesn't have means a restart. It only restarts when it must:

- The running mods are exactly the profile's: no restart.
- The running game has extra mods the profile doesn't name: no restart when every extra is
  one that doesn't change the game (the same check synctest makes per mod, recorded with
  each build), since it can't affect how the replay plays. An extra that changes the game
  means a restart.
- The profile needs a mod or game version the running game doesn't have: restart.

A restart boots straight into what was asked (no intro or menus, as quick-start does) with
the native pack already built, so it reads as a short loading screen, and leaving playback
restarts back into the player's own mods and the screen they came from.

`tgg_restart_with` runs `tgg profile prepare <profile.json> --machine` beside the game.
The helper prints its plan (including `needs_trust` entries), progress and finally the
prepared folder as JSON lines on stdout; the port draws the progress and any trust
prompt, then re-executes itself with the folder (and the needed port version, which may
mean executing that version instead). One implementation of fetching and trust
(`tgg-mod`), no package code in C, and it works however the game was started: the app,
`tgg`, or a desktop shortcut. Leaving playback restarts back into the player's own mods.

## Trust

Playing a replay means running code the player didn't pick.

- Only approved registry releases are ever fetched. Unapproved, unknown or unbuilt mods
  are `missing`, never fetched from anywhere else.
- Official mods (publisher `official`) fetch without asking.
- Other mods ask once: "This replay used *Name* by *Publisher*. Get it for playback?"
  Answers are remembered per publisher in tgg's config (`remember_trust`), shared by the
  app, `tgg` and the in-game prompt.
- A profile's mods run only for that session; nothing is installed into the player's own
  mods.

## Missing mods

Playback can still start when a profile isn't fully resolved, with the missing list shown
first. The replay browser marks a replay that won't match before the player picks it,
from the cached mapping.

## Netplay

- The host's hello carries its profile (versions, counted settings; no SHA-256).
- A joiner whose mods differ is shown the difference and offered the host's profile; on
  yes, its game restarts into it (`tgg_restart_with`) and rejoins.
- The check after that is the one netplay makes today.

## Work split

Platform session:

1. `tgg-mod`: the profile type and format; the package cache (also used by `apply`);
   catalog and mapping caches; `profile_from_*`, `resolve_profile`, `prepare_profile`,
   `launch_profile`, `finish_profile`; trust storage.
2. Registry: the `slippi` manifest field, `GET /api/registry/slippi` with the ignore list;
   first-party rule mods declare their Slippi codes.
3. Port: `tgg_restart_with` and the `restarted` event; the release ships `tgg` beside
   the game.
4. Mods: `tgg.replay-recorder` writes `metadata.tgg.profile`; `tgg.replay-player` and
   `tgg.replay-browser` resolve a replay's profile and restart into it; netplay offers
   the host's profile.
5. CLI: `tgg replay <file.slp>`, `tgg port run --profile <file>`,
   `tgg profile prepare --machine`.

Studio session: a replay browser in the app (the player's Slippi and tgg replay folders,
each row marked "plays as is" or "needs …" from `profile_from_slp` and `resolve_profile`
against the cached catalog and mapping), and playing a replay from it: what it needs
including the game version, the trust prompt, then Play through `prepare_profile` and
`launch_profile`. The in-game browser and the app's call the same `tgg-mod` code.

## Open questions

- How long profile folders and cached packages are kept.
- Whether the in-game trust prompt is the port's own screen or a mod's (the replay
  browser's).
