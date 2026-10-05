# tgg-cli

`tgg`, the textures.gg command line. It installs tgg-melee, the Melee port with a mod
loader built in, and the SDK mods build against, and its `mod` commands build, publish
and install code mods for it, on the [`tgg-mod`](../tgg-mod) library.

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

## `tgg port` and `tgg sdk`

```text
tgg port install [VERSION|latest] [--debug]   download a release of tgg-melee and install it
tgg port list                                 the installed versions, and the one in use
tgg port use VERSION                          use this version by default
tgg port remove VERSION
tgg port path [VERSION]                       the installed game's folder
tgg port run [--version VERSION] [-- ARGS]    run the game with your disc image
tgg sdk install [VERSION | --port EXE]        the SDK mods build against
tgg sdk list
tgg sdk path [VERSION | --port EXE]           its folder, for cmake -DTGG_SDK=$(tgg sdk path)
tgg config set|get|unset iso [PATH]           your Melee disc image (NTSC 1.02)
```

Releases of tgg-melee are for Linux x86-64 and run on glibc 2.34 or later. `port
install` downloads a release's `release.json` from `https://dl.textures.gg/tgg-melee`
(`TGG_RELEASES` names another mirror: a URL, or a folder laid out the same way), checks
the game's archive against the SHA-256 it gives, and unpacks it to
`~/.local/share/tgg/ports/<version>/`; the first version installed becomes the one in
use (`ports/current`). `--debug` adds the debug info gdb reads. Downloads are cached in
`~/.cache/tgg/downloads/`. Every installed version shares the game's own data: its mods
folder, pack and saves.

An SDK is the headers, symbol list and examples mods build against. It installs to
`~/.local/share/tgg/sdks/<game layout>/`, so releases that share a game layout share one
(a patch release keeps its layout, and a mod built for 0.3.0 loads on 0.3.2). With no
version, `sdk` commands use the game in use; `--port` reads an executable's version and
layout. Commands that build a mod install the SDK they need.

`port run` and `mod dev` start the game with `TGG_MELEE_ISO`, else the disc image set
with `tgg config set iso` (kept in `~/.config/tgg/config.json`).

## `tgg doctor`

`tgg doctor` checks what building and running mods needs, and prints the command that
fixes each problem: glibc 2.34 or later, an installed game and its SDK, a C compiler
(`--cc` or `CC`) that is GCC 12 or later (clang lacks the `scalar_storage_order` the
game's headers use), and a disc image. For a missing or old GCC it names the install
command for the system (`apt`, `dnf`, `pacman`, `zypper`), a newer `gcc-<n>` already on
`PATH`, or on NixOS `nix shell` with the GCC of the nixpkgs revision the game was built
with. The system's GCC is enough: the x86-64 ABI fixes the layout, and `build`'s checks
keep a mod loadable on the oldest glibc the game runs on.

## `tgg mod`

```text
tgg mod new [DIR] [--id ID] [--name NAME] [--example NAME]
                                         start a mod from the SDK's template, and build it once
tgg mod build [DIR] [--debug] [--sdk SDK] [--source-zip ZIP|-] [-o OUT.zip] [--json]
                                         compile DIR's src/**/*.c against the game's SDK, check it,
                                         and pack it with DIR's files/, assets/ and include/
tgg mod dev [DIR] [--watch] [--gdb]     build it for debugging and run the game with it
tgg mod register [DIR]                   create the mod on textures.gg from its manifest
tgg mod publish [DIR]                    tag v<version> and push it to textures.gg, which builds it
tgg mod pack DIR [-o OUT.zip] [--json]   DIR is a built mod's folder
tgg mod inspect FILE                     a package zip, a mod library, or a game executable
tgg mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg mod list [--json]                    the installed mods, and whether each counts for netplay
tgg mod install [--port EXE] ZIP...      install packages, refusing another game layout or a conflict
tgg mod enable|disable|remove ID...
```

`new` copies the SDK's `examples/template/` (or `--example`'s) into an empty `DIR`, sets
the manifest's `id` (`DIR`'s name unless `--id`) and `name`, runs `git init`, and builds
once so `build/compile_commands.json` is there for the editor: the template's `.clangd`
points clangd at it, so completion and go-to-definition reach the game's headers.

A mod's source is `manifest.json` at its root, C sources under `src/`, headers for other
mods under `include/`, disc files under `files/`, mirroring the disc (`files/PlMrNr.dat`
replaces `/PlMrNr.dat`), and new files under `assets/`; names starting with `.` are left
out. A mod needs sources, files or assets. `build` compiles every `src/**/*.c` with GCC
(`--cc` or `CC`, version 12 or later) in one call, with the SDK's include path,
definitions, options, force includes and libraries plus `-shared -fPIC
-fvisibility=hidden -O2` and the `TGG_SELF_<id>` define, on paths relative to `DIR`, so
the same source and SDK give the same package anywhere; `--debug` builds with `-O0 -g`
instead. `SDK` is the SDK's folder or its `tgg-game-sdk.json`, or `TGG_GAME_SDK`; by
default it is the SDK of the game in use, installed if it's missing. The package goes to
`DIR/build/<id>-<version>.zip` unless `-o` says otherwise. A mod without sources
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

`register` and `publish` act as the signed-in user. `register` creates the mod on
textures.gg from the manifest's id, name and description (an id is unique there).
`publish` refuses uncommitted changes, creates the mod the first time, tags
`v<version>` from manifest.json at HEAD (or reuses that tag if it already points there),
gets a short-lived push token for the mod's repository, and pushes HEAD to `main` with
the tag; textures.gg builds every tag pushed to it.

Installed mods live in the folder the game loads: `TGG_MODS_DIR`, else
`$XDG_DATA_HOME/tgg-melee/mods`, else `~/.local/share/tgg-melee/mods`. A turned-off mod's
folder is `.<id>`. `install` refuses packages built for another game layout than the game
in use (or the executable `--port` or `TGG_PORT` names); a file two mods ship is a
warning that names the mod whose copy the game uses (the one that loads later).

## `tgg mod dev`

```text
tgg mod dev [DIR] [--watch] [--gdb] [--port VERSION] [--keep] [-- GAME ARGS]
```

`dev` builds the mod with `-O0 -g` into `DIR/build/dev/<id>/`, links the game's mods
folder's `<id>` to it, and runs the game (the one in use, or `--port`'s version) with your
disc image, its output passed through with this mod's refusals in red. An installed copy
of the same mod is moved aside to `.~dev-<id>` for the session and put back after; so is
the link, unless `--keep`. Before the game starts it warns about installed mods that
replace the same function or that the mod needs and are missing. `--watch` rebuilds on
each change to `manifest.json`, `src/`, `include/`, `files/` or `assets/` and restarts
the game when it builds; Ctrl-C ends the session. `--gdb` runs the game under gdb, which
needs its debug info (`tgg port install --debug`). The game's own switches pass through
the environment: `TGG_SKIP_INTRO=1`, `TGG_INPUT_SCRIPT`, `TGG_SYNCTEST=1` (the rollback
check), `TGG_UCF=0`, `TGG_UNLOCK_ALL=0`. A dev build never matches a registry build, so
it never matches a netplay peer's.

## License

GPL-3.0-or-later.
