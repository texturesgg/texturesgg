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

## Playing

```text
tgg port install                    the game, and the mods it comes with
tgg port run                        play
tgg mod sets                        the sets of mods on textures.gg
tgg mod install --set competitive   install one
```

`port install` asks once for your Melee disc image (NTSC 1.02) and remembers it. The
game runs on Linux (x86-64), Windows (x86-64) and macOS (Apple silicon). `tgg doctor`
checks everything the game and mod making need, and says how to fix what's missing.

## Mods from textures.gg

```text
tgg mod sets                             the registry's sets, and which are installed
tgg mod install [--set NAME] [-y]        install a set (pick one with none named)
tgg mod install ID[@RANGE]... [-y]       install mods, such as tgg.netplay or tgg.netplay@^0.2
tgg mod update [--set NAME] [-y]         update installed sets and mods
tgg mod list [--json]                    installed mods and sets (works offline)
tgg mod on|off [ID...]                   turn installed mods on or off (pick them with none named)
tgg mod remove ID...                     remove mods
tgg mod remove --set NAME                remove a set and the mods only it installed
```

A set is a named list of mods on textures.gg, such as Competitive. `install` and `update`
fetch the sets and the catalog for the game in use, show what they'll install or update
and the download size, and ask before going ahead (only in a terminal; `-y` skips it).
Each download is checked against the catalog's SHA-256. A mod is installed at the newest
release that fits its range and has a build for the game in use; one already installed
at a fitting version is kept. `update` also says when there's a newer tgg-melee.

`list` reads only the mods folder and its `mods.lock.json`, which records each mod's
version and the sets that installed it. A set shows as complete, customized (a mod turned
off), has updates, or incomplete (a mod missing or not built for the game). Turning a
set's mod off keeps it installed; `on` brings the set back to complete. Changes wait while
the game runs, and while another tgg (or the app) is changing the mods folder.

## Skins

```text
tgg skin install FILE [--for PlMrNr.dat] [--name NAME]   a costume or menu file in place of the game's
tgg skin list                                              the skins tgg-melee loads
tgg skin remove PlMrNr.dat                                 the game's own file again
tgg skin check FILE [--for PlMrNr.dat]                     whether a costume only changes looks
```

The tgg.skins mod loads them when the game starts (`tgg mod install tgg.skins`). The game
file comes from the skin's name (`PlMrNr-red.dat` is for `PlMrNr.dat`) unless `--for` says.
A costume that only changes looks shows in netplay games too; one that changes the skeleton
shows offline, and netplay games show the normal costume instead.

## Signing in

```text
tgg login [--no-browser]    confirm a code on textures.gg in your browser
tgg logout
tgg account ticket meet [--machine]
```

`login` shows a code and opens textures.gg's `/device` page, where you confirm it while
signed in. The token it gets is saved in `textures.gg/credentials.json` in your config
folder (`~/.config` on Linux), readable only by you, and sent with every request that
acts as you. It lasts a week from its last use. `logout` ends that session on the site
and forgets the token.

`account ticket meet` gets a ticket for the netplay meeting point: a pass that works there
for two minutes, so the game can play online as you without ever holding your token. It
signs in first if you aren't. With `--machine` it prints JSON lines for the game
(`tgg/account.h`): `{"event":"signin","code","url"}` while a sign-in waits to be
confirmed, then `{"event":"ticket","ticket","name","expires"}`, or
`{"event":"error","message"}`.

`--api URL` (or `TGG_API`) points every command at another textures.gg API, such as a
local one; the default is `https://api.textures.gg`.

## In a terminal

When tgg talks to a person at a terminal, it shows pickers, confirmations and progress
bars, and colors versions, paths and commands. Piped, in CI (`CI` set), or with `NO_COLOR`
or `TERM=dumb`, output is plain text and nothing asks: every choice comes from an argument
or a flag, and a command that would pick says what to name instead.

```text
$ tgg mod install
◆  Install which set?
│  ● Competitive (Online rules, netplay and direct connect. · 2 mods)
│  ○ Replays
└
```

Errors print as one line, `Error: ...`, never a backtrace (`TGG_BACKTRACE=1` with
`RUST_BACKTRACE=1` shows one).

## `tgg port` and `tgg sdk`

```text
tgg port install [VERSION|latest] [--debug]   download a release of tgg-melee and install it
tgg port list                                 the installed versions, and the one in use
tgg port use [VERSION]                        use this version by default (pick one with none named)
tgg port remove VERSION
tgg port path [VERSION]                       the installed game's folder
tgg port run [--version VERSION] [-v] [-- ARGS]
                                              run the game with your disc image
tgg sdk install [VERSION | --port EXE]        the SDK mods build against
tgg sdk list
tgg sdk path [VERSION | --port EXE]           its folder, for cmake -DTGG_SDK=$(tgg sdk path)
tgg config set|get|unset iso [PATH]           your Melee disc image (NTSC 1.02)
```

tgg-melee is built for Linux on x86-64 (glibc 2.34 or later), Windows on x86-64 and
macOS on Apple silicon. `port install` downloads the release's
`<version>/release-<target>.json` for this system (`x86_64-linux-gnu`,
`x86_64-windows-gnu` or `aarch64-apple-darwin`; a Linux release from before the others
has only `release.json`) from `https://dl.textures.gg/tgg-melee` (`TGG_RELEASES` names
another mirror: a URL, or a folder laid out the same way), checks the game's archive
against the SHA-256 it gives, and unpacks it to `<data>/tgg/ports/<version>/`. `latest`
is the version `latest.json` names, or before the first release `latest-prerelease.json`.
With no version, `port install` installs the newest and switches to it; a named
version becomes the one in use when it is the first installed, or when you say so (in a
terminal it asks). `ports/current` names the one in use. `port use` with no version
lists the installed ones to pick from, with how many of your mods each lacks a build for.
`--debug` adds the debug info gdb reads (Linux). Downloads are cached in
`<cache>/tgg/downloads/`. `<data>` and `<cache>` are `~/.local/share` and `~/.cache`
on Linux, `~/Library/Application Support` and `~/Library/Caches` on macOS, and
`%LOCALAPPDATA%` on Windows. Every installed version shares the game's own data: its
mods folder, pack and saves.

A release comes with mods for players, built mod folders in the game's
`default-mods/`. Installing or switching to a version installs those the mods folder
lacks, and replaces older builds of them, keeping each one on or off. A mod tgg
installed this way once and you removed stays removed (`<data>/tgg/default-mods.json`
lists them). `tgg mod off <id>` turns one off.

`port run` writes the game's output to `game.log` in `<data>/tgg/`, and shows its last
lines when the game fails; `-v` shows it as the game runs.

On Windows the game needs Microsoft's Visual C++ runtime (`msvcp140.dll`,
`msvcp140_atomic_wait.dll`, `vcruntime140.dll`, `vcruntime140_1.dll`); `port install` and
`port run` check for it and name the fix, `winget install Microsoft.VCRedist.2015+.x64`.

An SDK is the headers, symbol list and examples mods build against. It installs to
`<data>/tgg/sdks/<game layout>/`, so releases that share a game layout share one
(a patch release keeps its layout, and a mod built for 0.3.0 loads on 0.3.2). With no
version, `sdk` commands use the game in use; `--port` reads an executable's version and
layout. Commands that build a mod install the SDK they need.

`port run` and `mod dev` start the game with `TGG_MELEE_ISO`, else the disc image set
with `tgg config set iso` (kept in `tgg/config.json` in your config folder: `~/.config`
on Linux, `~/Library/Application Support` on macOS, `%APPDATA%` on Windows). With
neither, they ask for it in the terminal and remember it. A plain image (`.iso`,
`.gcm`) must be Melee NTSC 1.02 (`GALE01`, revision 2).

## `tgg docs`

`tgg docs [PAGE]` opens the mod docs for the game in use,
`https://textures.gg/docs/mods/<major.minor>/<page>`, where a page is a file of the SDK's
`docs/` without `.md` (`writing-mods`, `hooks`); with no page, the index. When
textures.gg can't be reached it opens the SDK's own copy instead. `tgg mod new` puts the
same links in a new mod's README and `src/mod.c`.

## `tgg doctor`

`tgg doctor` checks what playing and making mods need, and prints the command that
fixes each problem. It exits with 1 when it finds something to fix.

Playing: an installed game, a disc image, and what the game opens at runtime. On Linux
that is glibc 2.34 or later, libvulkan and a Vulkan driver, and X11's or Wayland's
client libraries (with no GPU driver, `TGG_CPU_GPU=1` runs it on Mesa's lavapipe); on
Windows, the Visual C++ runtime.

Making mods: the SDK, a C compiler (`--cc` or `CC`) that is GCC 12 or later (clang lacks
the `scalar_storage_order` the game's headers use), CMake 3.25 or later, Ninja (needed on
Windows, faster elsewhere) and git. On Windows the GCC must link msvcrt, as the game does:
WinLibs' MSVCRT build (`winget install BrechtSanders.WinLibs.POSIX.MSVCRT`); a UCRT GCC
is a problem. On macOS, `gcc` is Apple's clang, so tgg uses Homebrew's `gcc-15` (or
`gcc-16`, `gcc-14`) by default, and needs Xcode's command line tools.
For a missing or old tool it names the install command for the system (`winget`, `brew`,
`apt`, `dnf`, `pacman`, `zypper`), a newer `gcc-<n>` already on `PATH`, or on NixOS
`nix shell` with the tool from the nixpkgs revision the game was built with. The
system's GCC is enough: the ABI fixes the layout, and the SDK's checks keep a mod
loadable on the oldest glibc the game runs on.

## `tgg mod`

```text
tgg mod new [DIR] [--id ID] [--name NAME] [--example NAME]
                                         start a mod from the SDK's template, and build it once
tgg mod build [DIR] [--debug] [--sdk SDK] [--source-zip ZIP|-] [-o OUT.zip] [--json]
                                         compile DIR's src/**/*.c against the game's SDK, check it,
                                         and pack it with DIR's files/, assets/ and include/
tgg mod dev [DIR] [--watch] [--gdb]     build it for debugging and run the game with it
tgg mod register [DIR]                   create the mod on textures.gg from its manifest
tgg mod publish [DIR]                    build it here, tag v<version> and push it to textures.gg (--skip-build)
tgg mod pack DIR [-o OUT.zip] [--json]   DIR is a built mod's folder
tgg mod inspect FILE                     a package zip, a mod library, or a game executable
tgg mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg mod install [--port EXE] ZIP...      install package zips, refusing another game layout or a conflict
```

`tgg mod list --json` gives each installed mod's manifest, whether it's on, its netplay
class, the sets that installed it, and whether it fits the game. `enable` and `disable`
are `on` and `off`.

`new` copies the SDK's `examples/template/` (or `--example`'s) into `DIR`, which may
already hold dotfiles, a README, a license and a Nix flake (it keeps those, and adds the
template's `.gitignore` lines to an existing one), sets the manifest's `id` (`DIR`'s name
unless `--id`) and `name`, and the template README's title and log lines to them,
runs `git init` and commits the template (without git, or before git knows your name
and email, it says what to do and goes on), and builds once so
`build/compile_commands.json` is there for the editor: the template's `.clangd` points clangd at it, so completion and go-to-definition
reach the game's headers. On NixOS, an editor's own clangd may not find the C library's
headers; run the clangd of the project's dev shell (`direnv exec . clangd`) with
`--query-driver=/nix/store/*/bin/gcc`.

A mod's source is `manifest.json` at its root, C sources under `src/`, headers for other
mods under `include/`, disc files under `files/`, mirroring the disc (`files/PlMrNr.dat`
replaces `/PlMrNr.dat`), and new files under `assets/`; names starting with `.` are left
out. A mod needs sources, files or assets. `build` builds the sources with the SDK's own
CMake files (`TggMod.cmake`): it writes a small CMake project in `DIR/build/tgg-project/`
naming `DIR`'s manifest and `src/**/*.c`, whatever `CMakeLists.txt` the mod has, and runs
CMake (with Ninja when it's installed, and always on Windows) in `DIR/build/tgg/`, with GCC
(`--cc` or `CC`, version 12 or later). An SDK that ships `toolchain.cmake` (macOS's) builds
with it as CMake's toolchain file, and gets the GCC as `TGG_MACOS_GCC`. Sources see `DIR`'s
`include/` and the `include/` of each mod `depends` names, from the game's mods folder, so a
mod uses a provider's header as the provider ships it; a dependency that isn't installed is
named. So a mod builds the same way here and on textures.gg, the same
source and SDK give the same package from any folder, and each SDK compiles, links and
checks mods its own way: what a mod takes from the game, the warning for a game function
used as a value, and the link and glibc check are the SDK's. `--debug` builds with
`-O0 -g` instead of `-O2`. CMake's `compile_commands.json` is copied to `DIR/build/` for
editors. `SDK` is the SDK's folder or its `tgg-game-sdk.json`, or `TGG_GAME_SDK`; by
default it is the SDK of the game in use, installed if it's missing. The package goes to
`DIR/build/<id>-<version>.zip` unless `-o` says otherwise. A mod without sources needs no
SDK: `build` packs it without compiling, and leaves `entry` out of its manifest.
`--source-zip` first unpacks `manifest.json` and those folders from a zip (up to 256 MiB),
or from stdin with `-`, into an empty `DIR`; the registry's builder sends each mod's source
that way.

Before packing, `build` checks the library's records as the game will at load, and fails
with what to change: its game layout, target and mod API version must be the SDK's, and
every hook and game symbol it names must be in the SDK's `symbols.txt` (no static whose
name several files share, no hook on a static the compiler also copied or on the mod
runtime's own functions). `--json` reports the package, its manifest, each hook's
canonical name (`name` for an exported function, `file.c:name` for a static, which is what
conflict checks compare) and its netplay class: `code` or `files` (it counts), `costumes`
(it counts unless the game finds its costumes only change looks) or `data` (free). CMake's
own output goes to stderr, so stdout is only the report.

`register` and `publish` act as the signed-in user. `register` creates the mod on
textures.gg from the manifest's id, name and description (an id is unique there).
`publish` refuses uncommitted changes, creates the mod the first time, tags
`v<version>` from manifest.json at HEAD (or reuses that tag if it already points there),
gets a short-lived push token for the mod's repository, and pushes HEAD to `main` with
the tag; textures.gg builds every tag pushed to it.

Installed mods live in the folder the game loads: `TGG_MODS_DIR`, else `mods` in the
game's data folder, which is `$XDG_DATA_HOME/tgg-melee` when that is set, else
`~/.local/share/tgg-melee` on Linux, `~/Library/Application Support/tgg-melee` on macOS
and `%LOCALAPPDATA%\tgg-melee` on Windows. A mod's library is `mod.so` (a shared object on
Linux, a bundle on macOS) or `mod.dll` on Windows. A turned-off mod's folder is `.<id>`.
`install` refuses packages built for another game layout than the game in use (or the
executable `--port` or `TGG_PORT` names); a file two mods ship is a warning that names
the mod whose copy the game uses (the one that loads later).

## `tgg mod dev`

```text
tgg mod dev [DIR] [--watch] [--gdb] [--port VERSION] [--keep] [-- GAME ARGS]
```

`dev` builds the mod with `-O0 -g` into `DIR/build/dev/<id>/`, links the game's mods
folder's `<id>` to it (on Windows, which won't follow a junction a standard user made,
copies it there instead), and runs the game (the one in use, `--port`'s version, or
`--game`'s build folder) with your disc image, its output passed through with this mod's
refusals in red. An installed copy of the same mod is moved aside to `.~dev-<id>` for the
session and put back after; the link or copy is removed, unless `--keep`. Before the game starts it warns about installed mods that
replace the same function or that the mod needs and are missing. `--watch` rebuilds on
each change to `manifest.json`, `src/`, `include/`, `files/` or `assets/` and restarts
the game when it builds; Ctrl-C ends the session. Ctrl-C or SIGTERM to `tgg` (from
`timeout`, say) reaches the game too, here and in `tgg port run`, and a game still running
five seconds later is killed. `--gdb` runs the game under gdb, which needs its debug
info (`tgg port install --debug`). The game's own switches pass through the environment: `TGG_SKIP_INTRO=1`, `TGG_INPUT_SCRIPT`, `TGG_SYNCTEST=1` (the rollback
check), `TGG_UNLOCK_ALL=0`. A dev build never matches a registry build, so
it never matches a netplay peer's.

## License

GPL-3.0-or-later.
