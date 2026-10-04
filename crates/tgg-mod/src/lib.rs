//! Mod packages for [tgg-mod-runtime](https://github.com/texturesgg/tgg-mod-runtime),
//! the mod loader a Melee source port links.
//!
//! A package is a zip holding `manifest.json`, the mod's shared library, and
//! the game files it ships under `files/`, if any; a mod may ship only files.
//! The hooks a mod installs are not taken from its manifest: they are read
//! from the records its library carries ([`decls`]), so a conflict between
//! two mods is a fact about their code. This crate owns that format, from
//! packing a built mod to installing it into a port's `mods/` folder:
//!
//! - [`manifest`]: what a mod says about itself.
//! - [`decls`]: the hooks, exports, imports and game layout a library
//!   declares.
//! - [`files`]: the game files a mod ships, and the rules for their paths.
//! - [`package`]: packing and opening package zips.
//! - [`catalog`]: the list of packages a registry offers.
//! - [`layout`]: the functions a game layout lets mods name, and each
//!   hook's canonical name.
//! - [`port`]: a port's executable, and whether it carries the runtime.
//! - [`sdk`]: a port's game SDK, and the compiler command that builds a mod
//!   against it.
//! - [`install`]: a port's `mods/` folder, conflicts between mods, and
//!   imports a mod needs.

pub mod catalog;
pub mod decls;
pub mod files;
pub mod install;
pub mod layout;
pub mod manifest;
pub mod package;
pub mod port;
pub mod sdk;

pub use catalog::{Catalog, CatalogEntry, PackageRef};
pub use decls::{Declarations, Hooks};
pub use files::{Files, ModFile};
pub use install::{Clash, Conflict, Installed, ModsDir, conflicts, unmet_imports};
pub use layout::{Layout, Symbols};
pub use manifest::{Manifest, ModId, Netplay};
pub use package::Package;
pub use port::Port;
pub use sdk::Sdk;

/// The runtime API this crate's packages target.
pub const API: &str = "tgg/1";
