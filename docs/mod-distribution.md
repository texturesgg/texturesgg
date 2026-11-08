# Mod distribution: the app, the CLI and the registry

How a player gets tgg-melee and its mods, and the contract between the pieces
that make it happen: the registry on textures.gg, the `tgg-mod` crate, and the
front ends over it. The front ends today are tgg-studio and the `tgg` CLI. An
in-game mod browser (a mod itself, in C) comes later, so the registry's HTTP
API and the lock file format are part of the contract, not internals of the
crate.

## The player's path

1. Install the textures.gg app. It finds or asks for the player's Melee disc
   image (it already does for skins).
2. The app installs the port: the newest tgg-melee release for the player's
   system. Port releases ship no mods.
3. The Mods place shows **sets**. A set is a named, curated list of mods:
   "Competitive" (the Slippi-style setup: online rules, competitive rules,
   legal stages, skip results, quick start, netplay, direct connect) and
   "Replays" (replay player, browser and recorder). One button installs a set.
   Before it does, the app shows what it will install and how big it is.
4. Play launches the port with the player's disc image. Netplay is part of the
   game: the direct-connect mod's screen, from the in-game Mods page. The port
   needs no launch flag for it.
5. Updates: the app shows when the port, an installed set or a mod has a newer
   version, and updates them in one step.

The CLI does the same with `tgg port install`, `tgg mod install --set
competitive` and `tgg port run`. Mod makers keep `tgg mod publish`.

## Pieces and who owns them

| Piece | Where | Owner |
|---|---|---|
| Registry: mods, releases, builds, sets, catalog | site (`packages/api`, `packages/mod-build`) | platform session |
| Port releases (`dl.textures.gg/tgg-melee/...`) | tgg-melee release workflow | platform session |
| `tgg-mod` crate: ports, registry queries, plans, installs, local state, launching | core `crates/tgg-mod` | platform session |
| `tgg` CLI commands over the crate | core `crates/tgg-cli` | platform session |
| App: first run, Mods and Play places | core `crates/tgg-studio`, `crates/tgg-app` | studio session |
| In-game mod browser (later) | a tgg-melee mod | platform session |

## Concepts

- **Port install**: one installed tgg-melee release in its own folder under the
  ports folder. Several versions can be installed; one is the current one. A
  port has a target (`x86_64-linux-gnu`, `x86_64-windows-gnu`,
  `aarch64-apple-darwin`) and a game layout id (`Port::game_abi`). A mod build
  fits a port when both match.
- **Mods folder**: the folder the game loads mods from, shared by every port
  version the player runs. A mod's folder is its id; a turned-off mod's folder
  is `.<id>` (the game skips dot-folders). `tgg_mod::install::ModsDir` already
  does this.
- **Registry mod**: an id with releases (versions). Each release has one build
  per active layout. The catalog lists them.
- **Set**: a name, a title, a description, a version, and a list of `(mod id,
  version range)`. Sets live in the registry, edited by admins. Its version is
  bumped when its list or ranges change.
- **Plan**: what an install will do, resolved against one registry snapshot and
  one port: for each mod, the exact release and build (with its SHA-256), the
  download size, and whether it's new, an update, or already there. Plus the
  problems found (no build for this port, conflicts, missing dependencies). A
  plan records the catalog snapshot it came from.
- **Lock**: `mods.lock.json` in the mods folder. For each installed mod: id,
  version, build SHA-256, and its origin (a set and that set's version, a
  direct install, or a dev build). With it, "what's installed" and "what's out
  of date given a snapshot" need no network. Its format is versioned and
  documented, since the in-game browser will read and write it too.

## The disc image

The port converts the disc it's given into its own copies of the game's files,
keyed by each file's contents, and reads mods' files first. A disc with edited
files works, but netplay and replays assume every player has the same game
files, and the disc isn't part of the netplay identity. So the port wants a
clean NTSC 1.02 disc, and `launch` takes the clean one. A disc the app has
written skins into is for Dolphin.

## Open questions

- **Skins in the port.** A mod can carry game files (`files/`), and a
  texture-only costume is free to use in netplay, so a skin from the player's
  library could be installed into the port as a generated files mod instead of
  only into a disc. Whether the app does that, and who builds it, is for the
  owner to decide.

## Registry (HTTP, site)

All JSON, unauthenticated for reading. The crate reads it through `Net`; the
in-game browser will read it directly.

- `GET /api/registry/catalog?layout=<game_abi>`: the `tgg_mod::catalog::Catalog`
  for one layout: every approved release with a build for it (manifest,
  netplay class, package URL, SHA-256, size), plus `snapshot`, an id for this
  state of the catalog.
- `GET /api/registry/sets`: every set: `{name, title, description, version,
  mods: [{id, range}]}`.
- Package URLs are content-addressed (by SHA-256) and immutable.

Builds: the builder builds each approved release for every active layout,
which is one per target for each active port version. Adding a port release to
the builder adds three layouts (Linux, Windows, macOS).

## Network

Every network call is one the player asked for, or one they can turn off. The
app fetches sets and the catalog when the Mods place opens or the player asks,
and at launch only when the app's Settings allow update checks, as the app's
own update check works. No background polling. The CLI fetches only for the
command run.

## The crate (`tgg-mod`)

Plain Rust, blocking, no UI dependencies. HTTP is ureq 3 (no default features,
rustls), as in tgg-app and tgg-cli. Light dependencies: it builds in the
studio's workspace too. Errors are typed, and their `Display` reads well to a
player. Long work takes a progress callback and a cancel flag.

```rust
// Fetching, fakeable for tests
pub trait Net: Send + Sync {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, NetError>;
}
pub struct HttpNet { /* base URL (TGG_API_URL overrides), ureq agent */ }
pub struct FakeNet { /* canned catalog, sets, packages, port releases */ }

pub enum Error {
    Offline(NetError),            // the app shows "offline, here's what's installed"
    Busy,                         // another tgg (CLI or app) holds the mods folder
    GameRunning,                  // apply/set_enabled/uninstall while launch's game runs
    Cancelled,
    Checksum { id: ModId, expected: String },
    // ... the rest player-readable
}

pub struct Progress {
    pub step: usize,              // 0-based
    pub steps: usize,
    pub label: String,            // "Downloading Legal Stages"
    pub bytes: u64,               // of the current step
    pub total: Option<u64>,
}
type OnProgress<'a> = &'a (dyn Fn(Progress) + Sync);

// Default locations (pure; the app passes them back in)
pub fn default_mods_dir() -> PathBuf;    // the game's own rule: game_data_dir()/mods
pub fn default_ports_dir() -> PathBuf;

// Ports
pub fn port_releases(net: &dyn Net) -> Result<Vec<PortRelease>, Error>;   // newest first, for this system
pub fn installed_ports(ports: &Path) -> Result<Vec<PortInstall>, Error>;
pub fn port_update(installed: &[PortInstall], releases: &[PortRelease]) -> Option<PortRelease>;
pub fn install_port(net: &dyn Net, release: &PortRelease, ports: &Path,
                    progress: OnProgress, cancel: &AtomicBool) -> Result<PortInstall, Error>;
pub fn remove_port(ports: &Path, version: &str) -> Result<(), Error>;
pub fn launch(port: &PortInstall, iso: &Path, mods: &Path) -> Result<Running, Error>;

// Registry
pub fn sets(net: &dyn Net) -> Result<Vec<Set>, Error>;
pub fn catalog(net: &dyn Net, port: &Port) -> Result<Catalog, Error>;

// Planning: no network, no writes
pub fn plan_set(set: &Set, catalog: &Catalog, local: &Local) -> Plan;
pub fn plan_mods(want: &[(ModId, VersionReq)], catalog: &Catalog, local: &Local) -> Plan;
pub fn plan_updates(sets: &[Set], catalog: &Catalog, local: &Local) -> Plan;
impl Plan { pub fn size(&self) -> u64; pub fn problems(&self) -> &[Problem]; }

// Applying: network and disk
pub fn apply(net: &dyn Net, plan: &Plan, mods: &Path,
             progress: OnProgress, cancel: &AtomicBool) -> Result<Applied, Error>;

// Local state: disk only
pub fn local(mods: &Path, port: Option<&Port>) -> Result<Local, Error>;
pub fn set_enabled(mods: &Path, id: &ModId, on: bool) -> Result<(), Error>;
pub fn uninstall(mods: &Path, id: &ModId) -> Result<(), Error>;
pub fn uninstall_set(mods: &Path, name: &str) -> Result<(), Error>;
```

`Local` holds, with no network:

- each installed mod: manifest, on or off, lock entry, origin ("part of
  Competitive"), and whether it has a build for the given port;
- each installed set with a status: `Complete`, `Customized` (a mod of it
  turned off), `Outdated` (a newer set version or a newer fitting release in
  the last snapshot seen), `Incomplete` (a mod missing or not built for this
  port);
- conflicts, overlaps and unmet dependencies as data (`install::Conflict`,
  `Overlap`, unmet ids).

`local(mods, None)` works before any port is installed: it lists what's in the
mods folder (usually nothing) with no build checks. `Local::empty()` is the
state of a player with nothing installed.

`FakeNet` comes with the first version of the crate: a catalog for one layout,
a Competitive and a Replays set, small packages, and a port release, so the
app's flow tests drive the whole Mods place with no network.

## Behavior

- **Installing a set** installs the newest release of each listed mod that
  fits the set's range and has a build for the port. A mod already installed
  at a fitting version is left alone. The lock records the set's name and
  version.
- **On and off** are per mod. Turning a set's mod off keeps it installed and
  the set shows as `Customized`; turning it on again restores the set.
- **Updating** a set re-plans it against the newest set and catalog: new mods in
  the set are installed, newer fitting releases replace older ones, and a mod
  dropped from the set stays installed and loses that origin. The app shows the
  plan before applying it.
- **Uninstalling** a mod removes its folder and lock entry. Uninstalling a set
  removes the mods it installed that no other installed set lists and that
  weren't installed on their own.
- **Port updates**: installing a newer port makes it current and keeps the
  previous one until the player's mods all have builds for the new one (the app
  can switch back). Older ports are removed only when the player asks
  (`remove_port`). A new port with no builds yet for some mods shows those as
  not built; the app offers to keep the older port until they are.
- **Applying a plan** checks each package's SHA-256 against the plan, so a plan
  shown and applied minutes later can't install something else. Each mod is
  installed atomically (staged beside its folder, then swapped, as `ModsDir`
  does); cancelling between mods leaves every mod either old or new; the lock
  is written last.
- **Concurrency**: `apply`, `set_enabled`, `uninstall` and `uninstall_set` take
  an advisory lock file in the mods folder and return `Busy` when another tgg
  holds it.
- **While the game runs**: the game reads its mods once, at start, so changes
  made while it runs apply next start. On Windows a running game's mod
  libraries can't be replaced, so while the game runs `apply`, `set_enabled`,
  `uninstall` and `uninstall_set` refuse with `GameRunning`, on every system,
  for one behavior. `launch` writes a pid file in the mods folder and returns
  a `Running` handle; the crate's check reads the pid file, so it holds
  whichever front end launched the game, and after the app is closed and
  reopened.

## Netplay

Netplay compares the exact mod set (ids, versions, builds, counted settings) on
the same platform, and ids, versions and counted settings across platforms. Two
players with the same set installed match. A set's own version isn't part of
it; the mods are.

## Work split

Platform session, in this order:
1. `tgg-mod`: the types and functions above with `FakeNet`, first as a version
   the app can build against (real local state, fake registry), then the real
   registry calls. Port releases and installs move out of `tgg-cli` into it.
2. Registry: sets, the per-layout catalog endpoint, builds for Linux, Windows
   and macOS layouts of the current port release; wipe test data; publish the
   first-party mods; create the Competitive and Replays sets.
3. `tgg-cli`: `tgg mod install [--set]`, `list`, `update`, `remove`, `on`,
   `off`, over the crate.
4. A port release with no default mods.

Studio session: first run (find the clean disc, `install_port`), the Mods place
(set cards, installed list with on/off, update badges and "Update all", details
pane) on `plan_*`, `apply` and `local`, and the Play button on `launch`.

Later: the in-game mod browser, a mod over the registry HTTP API and the lock
format.
