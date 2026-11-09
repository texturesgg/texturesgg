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
  per active layout. The catalog lists them. No registry id starts with
  `skin.`: the prefix is kept for skins.
- **Set**: a name, a title, a description, a version, and a list of `(mod id,
  version range)`. Sets live in the registry, edited by admins. Its version is
  bumped when its list or ranges change.
- **Plan**: what an install will do, resolved against one registry snapshot and
  one port: for each mod, the exact release and build (with its SHA-256), the
  download size, and whether it's new, an update, or already there. Plus the
  problems found (a mod the registry lacks, one not built for this port,
  conflicts, missing dependencies). A
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
  state of the catalog, and `ids`, every mod id the registry has (built for
  this layout or not). 404 when the layout has no builds: the crate says
  "textures.gg has no mods for tgg-melee X yet".
- `GET /api/registry/sets`: every set: `{name, title, description, version,
  mods: [{id, range}]}`. 404 reads as no sets.
- `GET /api/registry/mods?page=&pageSize=&sort=`: every mod with an approved
  release, 50 to a page by default (at most 100), official mods first and
  then most installed (`sort=official`, the default; `sort=installs` is
  most installed alone). Each mod is described by its newest approved
  release. `publisher` is who a store shows as publishing it: the name
  textures.gg's admins set for the mod (first-party mods say `textures.gg`),
  or else its author's account name, and `official`, whether the admins
  vouch for it. `author` is the author's account name. `port` is the newest
  tgg-melee release the registry builds mods for; `targets` are the systems
  that release has builds for on it. `installs` counts first installs: a
  package download with `?for=install`, which tgg sends when the player
  doesn't have the mod yet. Updates, reinstalls and replay playback don't
  count.

  ```json
  {
    "port": "0.1.0",
    "items": [
      {
        "id": "tgg.ucf",
        "name": "UCF",
        "description": "Universal controller fix.",
        "author": "texturesgg",
        "publisher": { "name": "textures.gg", "official": true },
        "icon": { "url": "https://api.textures.gg/api/registry/images/<sha256>.png", "width": 256, "height": 256 },
        "tags": ["competitive", "netplay"],
        "installs": 412,
        "version": "0.84.0",
        "updatedAt": "2026-11-07T18:00:00.000Z",
        "sets": ["competitive"],
        "targets": ["linux", "windows", "macos"],
        "netplay": "code"
      }
    ],
    "page": 1, "pageSize": 50, "total": 1, "totalPages": 1
  }
  ```

- `GET /api/registry/mods/<id>`: one mod's page: the summary above, `port`,
  and `readme` (markdown: the README at the newest release, or null),
  `screenshots` (images as `icon`), `homepage`, `source` (its public source
  repository, or null), `license`, `releases` (every approved release,
  newest first: `{version, date, changelog}`, the changelog being that
  version's CHANGELOG.md section or null) and `depends` (`[{id, range}]` of
  the newest release). 404 for a mod it doesn't list.
- `GET /api/registry/slippi`: the Slippi mapping, which mods reproduce the gecko
  codes a Slippi replay ran; its shape is in [mod profiles](mod-profiles.md#the-slippi-mapping-get-apiregistryslippi).
  Catalog entries also carry `publisher` and `changes_game` for profiles
  ([catalog entries](mod-profiles.md#catalog-entries)).
- `GET /api/registry/layouts`: every game layout, newest port release
  first: `[{id, target, system, portVersion, active}]`, `system` being
  `linux`, `windows`, `macos` or `other`; `active` is false for a retired
  layout, whose packages stay downloadable.
- `GET /api/registry/images/<sha256>.png`: an icon or screenshot.
- Package and image URLs are content-addressed (by SHA-256) and immutable.
  Lists and catalogs may be a minute old.

A release's listing is read from its source when the release is recorded, so
review covers it: the README, the CHANGELOG.md section whose heading names the
version (`## 1.2.0`, `## [1.2.0] - 2026-11-07`), and the manifest's `icon`,
`screenshots`, `tags` and `homepage`. An icon or screenshot that is missing
or isn't a PNG fails the release. Images are at most 4096 pixels a side, 512
KiB for the icon and 4 MiB for a screenshot. The mods list and pages read
nothing of a port, so they work before one is installed.

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
    Offline(NetError),            // "can't reach textures.gg; what's installed still works"; why is its source()
    NoCatalog(String),            // "textures.gg has no mods for tgg-melee {0} yet"
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

// Store listings: what the Mods place and the in-game browser show
pub fn mods(net: &dyn Net, page: u32) -> Result<ModPage, Error>;      // page from 1, PAGE_SIZE (50) each
pub fn mod_details(net: &dyn Net, id: &ModId) -> Result<ModDetails, Error>;  // NoSuchMod on a 404
pub struct ModPage { port: Option<String>, items: Vec<ModSummary>, page: u32,
                     page_size: u32, total: u64, total_pages: u32 }
pub struct ModSummary { id: ModId, name: String, description: Option<String>,
                        author: String, publisher: Publisher,
                        icon: Option<Image>, tags: Vec<String>,
                        installs: u64, version: Version, updated_at: String,
                        sets: Vec<String>, targets: Vec<System>, netplay: Option<Netplay> }
pub struct ModDetails { summary: ModSummary /* flattened */, port: Option<String>,
                        readme: Option<String>, screenshots: Vec<Image>,
                        homepage: Option<String>, source: Option<String>,
                        license: Option<String>, releases: Vec<ModRelease>,
                        depends: Vec<SetMod> }
pub struct Publisher { name: String, official: bool }
pub struct Image { url: String /* absolute */, width: u32, height: u32 }
pub struct ModRelease { version: Version, date: String, changelog: Option<String> }
pub enum System { Linux, Windows, Macos, Other }

// Planning: no network, no writes
pub fn plan_set(set: &Set, catalog: &Catalog, local: &Local) -> Plan;
pub fn plan_mods(want: &[(ModId, Range)], catalog: &Catalog, local: &Local) -> Plan;
pub fn plan_updates(sets: &[Set], catalog: &Catalog, local: &Local) -> Plan;
pub fn plan_set_update(name: &str, sets: &[Set], catalog: &Catalog, local: &Local)
    -> Result<Plan, Error>;                 // one installed set; SetNotInstalled otherwise
impl Plan { pub fn size(&self) -> u64; pub fn problems(&self) -> &[Problem]; }
pub enum Problem {
    Unknown { id },               // the registry has no such mod
    Unavailable { id, ranges },   // it has it, but nothing in range is built for this port
    Conflict { id, conflict },
    Unmet { id, missing },        // dependencies or imports nothing meets
}

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
a Competitive and a Replays set, small packages, store listings of every mod
(an icon, two tags, a README, changelogs, and installs counted from its own
package downloads), and a port release, so the app's flow tests drive the
whole Mods place with no network.

## Sign-in (`tgg-account`)

The app and the CLI share one sign-in: `credentials.json` in the textures.gg
config folder, one token per API base URL (`{ api: { token, name, id?,
display_name? } }`), replaced atomically and readable only by the user. Every
call reads the file afresh, so signing in from one shows up in the other.
Login is the device flow: the user confirms a code on the site's /device page.
Same style as `tgg-mod`: blocking, no UI dependencies, player-readable errors.

```rust
pub struct Credentials;           // the file; Credentials::at(path) for another
impl Credentials {
    pub fn default_path() -> Option<PathBuf>;
    pub fn open_default() -> Result<Self, Error>;
    pub fn at(path: impl Into<PathBuf>) -> Self;
    pub fn get(&self, api: &str) -> Result<Option<Credential>, Error>;
    pub fn set(&self, api: &str, credential: &Credential) -> Result<(), Error>;
    pub fn remove(&self, api: &str) -> Result<Option<Credential>, Error>;
}

pub struct Account { pub handle: String, pub display_name: Option<String>, pub id: Option<String> }
pub struct SignedIn { pub account: Account, pub verified: bool } // false: offline, as saved

pub struct Site;                  // an API base URL and the sign-in saved for it
impl Site {
    pub fn new(api: &str, user_agent: &str) -> Result<Self, Error>;
    pub fn with_credentials(api: &str, user_agent: &str, credentials: Credentials) -> Self;

    // Device login, driven by the app: show the code, poll every `interval`
    pub fn begin_device_login(&self) -> Result<DeviceLogin, Error>;
    pub fn poll_device_login(&self, login: &DeviceLogin) -> Result<Poll, Error>;
    // ...or blocking, for the CLI
    pub fn login(&self, open_browser: bool, on_code: impl FnOnce(&DeviceLogin)) -> Result<Account, Error>;

    pub fn whoami(&self) -> Result<Option<SignedIn>, Error>;
    pub fn logout(&self) -> Result<Option<Account>, Error>; // ends the session, forgets the token
    pub fn token(&self) -> Result<Option<String>, Error>;

    // As the signed-in user
    pub fn net(&self, net: tgg_mod::HttpNet) -> Result<tgg_mod::HttpNet, Error>;
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error>;
    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, Error>;
    pub fn exists(&self, path: &str) -> Result<bool, Error>;
    pub fn signed_in(&self) -> Result<&Self, Error>;
}

pub struct DeviceLogin {
    pub user_code: String,        // display_code(): ABCD-EFGH
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>, // link() prefers it
    pub expires_at: Instant,
    pub interval: Duration,
}
pub enum Poll { Pending, SlowDown /* add 5 s to the interval */, Done(Account), Expired, Denied }

pub enum Error {
    NotSignedIn, SessionExpired, Expired, Denied,
    Offline { api, reason },      // whoami answers the saved account instead
    // ... the rest player-readable
}
```

`HttpNet::with_token` sends the token to URLs under the API only, never to the
release mirror.

## Behavior

- **Installing a set** installs the newest release of each listed mod that
  fits the set's range and has a build for the port. A mod already installed
  at a fitting version is left alone. What the chosen releases depend on (their
  manifests' `depends`) comes along, and takes the origin of the mods that need
  it; a dependency that can't be had shows as its dependent's `Unmet`. Installing
  mods on their own resolves dependencies the same way. A mod with no build in
  the catalog is `Unknown` unless the catalog's `ids` name it (without `ids`: a
  set lists it, or it is installed), and `Unavailable` otherwise. The lock records the set's name and
  version.
- **On and off** are per mod. Turning a set's mod off keeps it installed and
  the set shows as `Customized`; turning it on again restores the set.
- **Updating** a set re-plans it against the newest set and catalog: new mods in
  the set are installed, newer fitting releases replace older ones, and a mod
  dropped from the set stays installed and loses that origin. The app shows the
  plan before applying it. `plan_set_update` updates one set alone;
  `plan_updates` every set and every mod installed on its own.
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
the same platform, and ids, versions, sources and counted settings across
platforms. A mod's source is the commit of the reviewed source the registry
built it from (the manifest's `source`, which the builder writes with `tgg mod
build --source-commit`), so two systems' builds of one release match and a
build of anything else doesn't. Two players with the same set installed
match. A set's own version isn't part of it; the mods are.

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
