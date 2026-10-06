# tgg-mod

Mod packages for tgg-melee, the Melee port with a mod loader built in. This crate is
the Rust implementation of its package format (`package-format.md` in the game's docs,
which the SDK carries), manifest API `tgg-melee/0`.

A package is a zip holding `manifest.json`, the mod's shared library, and the folders it
ships: `files/` (disc files it replaces or adds), `assets/` (new files the game serves at
`/mods/<id>/`) and `include/` (headers for mods that build on it). A mod's hooks, events,
exports, imports and game layout come from the records its library carries, and its lists
of files from the files, not from what its author writes. That makes a conflict between two
mods, or a missing dependency, a fact about their code and data. A mod may ship only files:
it has no library and no game layout, so one package fits every build of the game.

## What it owns

- **Manifests.** The fields an author writes (`depends` among them), plus the ones packing
  fills from the library and folders. A manifest with a `netplay` field is refused, as the
  game refuses it.
- **Library records.** Reading the `tggdecls` section of an x86-64 ELF mod library: hooks,
  events, exports, imports, state, the init function, the game layout id, target and mod
  API version. A record kind the game doesn't have is refused.
- **Netplay class.** Whether a package counts toward the mod set netplay peers compare:
  code and disc files that can affect play count; menu and trophy files and assets don't;
  costumes count unless the game finds they only change looks.
- **Files.** The rules for the paths in `files/`, `assets/` and `include/` (no names
  starting with `.`, under 256 bytes, compared without case) and sizes (under 4 GiB each).
- **Packages.** Packing a built mod and its folders into a zip that is byte-identical for
  the same inputs. Opening a zip re-reads its library and files and refuses a manifest that
  disagrees.
- **Catalogs.** The list of packages a registry offers, with each package's location, size,
  SHA-256 and netplay class.
- **Game SDKs.** Reading the SDK's `tgg-game-sdk.json`: the game layout, target, API
  version and files a mod builds with. The SDK's own CMake files build mods.
- **Symbols.** The SDK's `symbols.txt`: which names a mod may hook or refer to, and each
  hook's canonical name (`name` for an exported function, `file.c:name` for a static).
- **Game builds.** Reading an executable's `tgg_port` section: the game layout mods must
  match, the target and the game's version.
- **Installing.** The game's mods folder: listing, installing over an older version, turning
  mods on and off (`.<id>`), removing them, load order, conflicts (two mods replacing one
  function), shared files (the mod that loads later wins), and unmet dependencies and
  imports.

It leaves downloading, signing, and any user interface to its callers.

The `tgg` command ([`tgg-cli`](../tgg-cli)) builds, publishes and installs mods with it.

A manifest's `version` is a semantic version.

## License

GPL-3.0-or-later.
