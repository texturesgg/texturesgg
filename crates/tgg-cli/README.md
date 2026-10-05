# tgg-cli

`tgg`, the textures.gg command line. Its `mod` commands build, publish and install
code mods for tgg-melee, on the [`tgg-mod`](../tgg-mod) library.

## Installing

```bash
curl -fsSL https://textures.gg/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://textures.gg/install.ps1 | iex
```

Either installs the latest release's `tgg` for your system into `~/.local/bin`
(`%LOCALAPPDATA%\Programs\tgg` on Windows), after checking it against the release's
`SHA256SUMS`. Each release on GitHub (`cli-v<version>`) has the archives for
Linux (static, x86_64 and arm64), macOS (universal, signed and notarized) and Windows,
and a build provenance attestation for each: `gh attestation verify <archive>
--repo texturesgg/texturesgg` checks an archive was built from this repository. With a
Rust toolchain, `cargo install --git https://github.com/texturesgg/texturesgg tgg-cli`
builds it instead.

## Signing in

```text
tgg login [--no-browser]    confirm a code on textures.gg in your browser
tgg logout
```

`login` shows a code and opens textures.gg's `/device` page, where you confirm it while
signed in. The token it gets is saved in `textures.gg/credentials.json` in your config
folder (`~/.config` on Linux), readable only by you, and sent with every request that
acts as you. It lasts a week from its last use. `logout` ends that session on the site
and forgets the token.

`--api URL` (or `TGG_API`) points every command at another textures.gg API, such as a
local one; the default is `https://api.textures.gg`.

## `tgg mod`

```text
tgg mod build [DIR] [--sdk SDK] [--source-zip ZIP|-] [-o OUT.zip] [--json]
                                         compile DIR's src/**/*.c against the game's SDK, check it,
                                         and pack it with DIR's files/, assets/ and include/
tgg mod new [DIR]                        create the mod on textures.gg from its manifest
tgg mod publish [DIR]                    tag v<version> and push it to textures.gg, which builds it
tgg mod pack DIR [-o OUT.zip] [--json]   DIR is a built mod's folder
tgg mod inspect FILE                     a package zip, a mod library, or a game executable
tgg mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg mod list [--json]                    the installed mods, and whether each counts for netplay
tgg mod install [--port EXE] ZIP...      install packages, refusing another game layout or a conflict
tgg mod enable|disable|remove ID...
```

A mod's source is `manifest.json` at its root, C sources under `src/`, headers for other
mods under `include/`, disc files under `files/`, mirroring the disc (`files/PlMrNr.dat`
replaces `/PlMrNr.dat`), and new files under `assets/`; names starting with `.` are left
out. A mod needs sources, files or assets. `build` compiles every `src/**/*.c` with GCC
(`--cc` or `CC`, version 12 or later) in one call, with the SDK's include path,
definitions, options, force includes and libraries plus `-shared -fPIC
-fvisibility=hidden -O2` and the `TGG_SELF_<id>` define, on paths relative to `DIR`, so
the same source and SDK give the same package anywhere. `SDK` is the SDK's folder or its
`tgg-game-sdk.json`, or the `TGG_GAME_SDK` environment variable. A mod without sources
needs no SDK: `build` packs it without compiling, and leaves `entry` out of its manifest.
`--source-zip` first unpacks `manifest.json` and those folders from a zip (up to 256 MiB),
or from stdin with `-`, into an empty `DIR`; the registry's builder sends each mod's source
that way.

Before packing, `build` checks the library as the game will at load, and fails with what
to change: its game layout, target and mod API version must be the SDK's; every hook and
game symbol it names must be in the SDK's `symbols.txt` (no static whose name several
files share, no hook on a static the compiler also copied or on the mod runtime's own
functions); everything it links against must be something the game exports, and no glibc
symbol may be newer than the oldest glibc the game runs on. `--json` reports the package,
its manifest, each hook's canonical name (`name` for an exported function, `file.c:name`
for a static, which is what conflict checks compare) and its netplay class: `code` or
`files` (it counts), `costumes` (it counts unless the game finds its costumes only change
looks) or `data` (free).

`new` and `publish` act as the signed-in user. `new` registers the manifest's id, name
and description as a mod on textures.gg (an id is unique there) and runs `git init` in
`DIR` if it isn't a repository yet. `publish` refuses uncommitted changes, tags
`v<version>` from manifest.json at HEAD (or reuses that tag if it already points there),
gets a short-lived push token for the mod's repository, and pushes HEAD to `main` with
the tag; textures.gg builds every tag pushed to it.

Installed mods live in the folder the game loads: `TGG_MODS_DIR`, else
`$XDG_DATA_HOME/tgg-melee/mods`, else `~/.local/share/tgg-melee/mods`. A turned-off mod's
folder is `.<id>`. `install --port EXE` (or `TGG_PORT`) refuses packages built for another
game layout than that executable's; a file two mods ship is a warning that names the mod
whose copy the game uses (the one that loads later).

## License

GPL-3.0-or-later.
