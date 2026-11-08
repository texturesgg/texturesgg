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
