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
- **Library records.** Reading the `tgg_decls` section of an x86-64 ELF mod library:
  hooks, exports, imports, and the game layout id.
- **Packages.** Packing a built mod into a zip that is byte-identical for the same
  inputs. Opening a zip re-reads its library and refuses a manifest that disagrees.
- **Catalogs.** The list of packages a registry offers, with each package's location,
  size, and SHA-256.
- **Ports.** Reading a port executable's `tgg_port` section: whether it carries the
  runtime, the game layout mods must match, and the port's name.
- **Installing.** A port's `mods/` folder: listing, installing over an older version,
  turning mods on and off, removing them, conflicts between mods, and unmet imports.

It leaves downloading, signing, and any user interface to its callers.

## The `tgg-mod` tool

```text
tgg-mod pack DIR [-o OUT.zip] [--json]   DIR holds manifest.json and the built library
tgg-mod inspect FILE                     a package zip, a mod library, or a port executable
tgg-mod catalog -o CATALOG.json ZIP...   list packages, with URLs relative to the catalog
tgg-mod list --port PORT [--json]        the mods installed in a port, in load order
tgg-mod install --port PORT ZIP...       install packages, refusing another game layout or a conflict
tgg-mod enable|disable|remove --port PORT ID...
```

`DIR` is what tgg-mod-runtime's `tgg_add_mod` writes for each mod. `PORT` is the
port's folder or executable, or the `TGG_PORT` environment variable. Hosts that only
need the library turn off the default `cli` feature.

A manifest's `version` is a semantic version.

## License

GPL-3.0-or-later.
