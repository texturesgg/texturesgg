# tgg-mod

Mod packages for [tgg-mod-runtime](https://github.com/texturesgg/tgg-mod-runtime), the
mod loader a Super Smash Bros. Melee port links. This crate is the Rust implementation
of the runtime's package format (`docs/package-format.md` in that repo), version
`tgg/1`.

A package is a zip holding `manifest.json` and the mod's shared library. A mod's
hooks, exports, imports, and game layout come from the records its library carries, not
from what its author writes. That makes a conflict between two mods, or a missing
dependency, a fact about their code.

## What it owns

- **Manifests.** The fields an author writes, plus the ones packing fills from the
  library. Validation covers what the runtime and installers rely on.
- **Library records.** Reading the `tggdecls` section of an x86-64 ELF mod library:
  hooks, exports, imports, the game layout id, and the target triple.
- **Packages.** Packing a built mod into a zip that is byte-identical for the same
  inputs. Opening a zip re-reads its library and refuses a manifest that disagrees.
- **Catalogs.** The list of packages a registry offers, with each package's location,
  size, and SHA-256.
- **Game SDKs.** Reading the `tgg-game-sdk.json` a port build writes, and the
  compiler command that builds a mod against it.
- **Ports.** Reading a port executable's `tgg_port` section: whether it carries the
  runtime, the game layout mods must match, and the port's name.
- **Installing.** A port's `mods/` folder: listing, installing over an older version,
  turning mods on and off, removing them, conflicts between mods, and unmet imports.

It leaves downloading, signing, and any user interface to its callers.

## The `tgg-mod` tool

```text
tgg-mod build [DIR] --sdk SDK [--source-zip ZIP|-] [-o OUT.zip] [--json]
                                         compile DIR's src/**/*.c against a game SDK and pack it
tgg-mod publish [DIR] --remote URL     tag v<version> and push it to textures.gg, which builds it
tgg-mod layout EXE -o LAYOUT.json       the functions a port build lets mods name
tgg-mod pack DIR [-o OUT.zip] [--json]   DIR holds manifest.json and the built library
tgg-mod inspect FILE                     a package zip, a mod library, or a port executable
tgg-mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg-mod list --port PORT [--json]        the mods installed in a port, in load order
tgg-mod install --port PORT ZIP...       install packages, refusing another game layout or a conflict
tgg-mod enable|disable|remove --port PORT ID...
```

A mod's source is `manifest.json` at its root and C sources under `src/`. `build`
compiles every `src/**/*.c` with GCC (`--cc` or `CC`) in one call, with the SDK's include
path, definitions and options plus `-shared -fPIC -fvisibility=hidden -O2` and the
`TGG_GAME_ABI`, `TGG_GAME_TARGET` and `TGG_SELF_<id>` defines, on paths
relative to `DIR`, so the same source and SDK give the same package anywhere. `SDK` is
the SDK's folder or its `tgg-game-sdk.json`, or the `TGG_GAME_SDK` environment variable.
`--source-zip` first unpacks `manifest.json` and `src/` from a zip, or from stdin with
`-`, into an empty `DIR`; the registry's builder sends each mod's source that way.

`build --layout LAYOUT.json` also checks every hook against a port build's symbols and
refuses a function the game doesn't have, a static whose name several files share, or a
static the compiler also copied; `--json` then reports each hook's canonical name
(`name` for an exported function, `file.c:name` for a static), which is what conflict
checks compare.

`publish` takes the remote and a push token (`--token` or `TGG_PUSH_TOKEN`) from the mod's
page on textures.gg. It refuses uncommitted changes, tags `v<version>` from manifest.json
at HEAD (or reuses that tag if it already points there), and pushes HEAD to `main` with
the tag.

For `pack`, `DIR` is what tgg-mod-runtime's `tgg_add_mod` writes for each mod. `PORT` is
the port's executable, or the `TGG_PORT` environment variable; mods install beside it in
`mods/`. Hosts that only need the library turn off the default `cli` feature.

A manifest's `version` is a semantic version.

## License

GPL-3.0-or-later.
