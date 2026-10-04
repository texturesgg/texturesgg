# tgg-cli

`tgg`, the textures.gg command line. Its `mod` commands build, publish and install
code mods for [tgg-mod-runtime](https://github.com/texturesgg/tgg-mod-runtime), on the
[`tgg-mod`](../tgg-mod) library.

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
                                         compile DIR's src/**/*.c against a game SDK and pack it
                                         with DIR's files/
tgg mod new [DIR]                        create the mod on textures.gg from its manifest
tgg mod publish [DIR]                    tag v<version> and push it to textures.gg, which builds it
tgg mod layout EXE -o LAYOUT.json       the functions a port build lets mods name
tgg mod pack DIR [-o OUT.zip] [--json]   DIR holds manifest.json and the built library
tgg mod inspect FILE                     a package zip, a mod library, or a port executable
tgg mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg mod list --port PORT [--json]        the mods installed in a port, in load order
tgg mod install --port PORT ZIP...       install packages, refusing another game layout or a conflict
tgg mod enable|disable|remove --port PORT ID...
```

A mod's source is `manifest.json` at its root, C sources under `src/`, and game files
under `files/`, mirroring the disc (`files/PlMrNr.dat` replaces `/PlMrNr.dat`); names
starting with `.` are left out. A mod needs sources, files, or both. `build`
compiles every `src/**/*.c` with GCC (`--cc` or `CC`) in one call, with the SDK's include
path, definitions and options plus `-shared -fPIC -fvisibility=hidden -O2` and the
`TGG_GAME_ABI`, `TGG_GAME_TARGET` and `TGG_SELF_<id>` defines, on paths
relative to `DIR`, so the same source and SDK give the same package anywhere. `SDK` is
the SDK's folder or its `tgg-game-sdk.json`, or the `TGG_GAME_SDK` environment variable.
A mod with only `files/` needs no SDK: `build` packs it without compiling, and leaves
`entry` out of its manifest. `--source-zip` first unpacks `manifest.json`, `src/` and
`files/` from a zip (up to 256 MiB), or from stdin with `-`, into an empty `DIR`; the
registry's builder sends each mod's source that way.

`build --layout LAYOUT.json` also checks every hook against a port build's symbols and
refuses a function the game doesn't have, a static whose name several files share, or a
static the compiler also copied; `--json` then reports each hook's canonical name
(`name` for an exported function, `file.c:name` for a static), which is what conflict
checks compare.

`new` and `publish` act as the signed-in user. `new` registers the manifest's id, name
and description as a mod on textures.gg (an id is unique there) and runs `git init` in
`DIR` if it isn't a repository yet. `publish` refuses uncommitted changes, tags
`v<version>` from manifest.json at HEAD (or reuses that tag if it already points there),
gets a short-lived push token for the mod's repository, and pushes HEAD to `main` with
the tag; textures.gg builds every tag pushed to it.

For `pack`, `DIR` is what tgg-mod-runtime's `tgg_add_mod` writes for each mod. `PORT` is
the port's executable, or the `TGG_PORT` environment variable; mods install beside it in
`mods/`.

## License

GPL-3.0-or-later.
