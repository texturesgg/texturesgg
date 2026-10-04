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

The `tgg` command ([`tgg-cli`](../tgg-cli)) builds, publishes and installs mods with it.

A manifest's `version` is a semantic version.

## License

GPL-3.0-or-later.
