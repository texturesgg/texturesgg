//! Mod packages for [tgg-mod-runtime](https://github.com/texturesgg/tgg-mod-runtime),
//! the mod loader a Melee source port links.
//!
//! A package is a zip holding `manifest.json` and the mod's shared library.
//! The hooks a mod installs are not taken from its manifest: they are read
//! from the records its library carries ([`decls`]), so a conflict between
//! two mods is a fact about their code. This crate owns that format, from
//! packing a built mod to installing it into a port's `mods/` folder:
//!
//! - [`manifest`]: what a mod says about itself.
//! - [`decls`]: the hooks, exports, imports and game layout a library
//!   declares.
//! - [`package`]: packing and opening package zips.
//! - [`catalog`]: the list of packages a registry offers.
//! - [`port`]: a port's executable, and whether it carries the runtime.
//! - [`install`]: a port's `mods/` folder, conflicts between mods, and
//!   imports a mod needs.

pub mod catalog;
pub mod decls;
pub mod install;
pub mod manifest;
pub mod package;
pub mod port;

pub use catalog::{Catalog, CatalogEntry, PackageRef};
pub use decls::{Declarations, Hooks};
pub use install::{Conflict, Installed, ModsDir, conflicts, unmet_imports};
pub use manifest::{Manifest, Netplay};
pub use package::Package;
pub use port::Port;

/// The runtime API this crate's packages target.
pub const API: &str = "tgg/1";
