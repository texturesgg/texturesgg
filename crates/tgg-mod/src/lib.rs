//! Mod packages for [tgg-melee](https://github.com/texturesgg/tgg-melee), the
//! Melee port with a mod loader built in.
//!
//! A package is a zip holding `manifest.json`, the mod's shared library, and
//! the folders it ships: `files/` (disc files), `assets/` (new files) and
//! `include/` (headers for other mods); a mod may ship only files. The hooks
//! a mod installs are not taken from its manifest: they are read from the
//! records its library carries ([`decls`]), so a conflict between two mods is
//! a fact about their code. This crate owns that format, from building a mod
//! against the game's SDK to installing it into the game's mods folder:
//!
//! - [`manifest`]: what a mod says about itself.
//! - [`depends`]: the version ranges in a manifest's `depends`.
//! - [`decls`]: the hooks, events, exports, imports and game layout a
//!   library declares.
//! - [`files`]: the files a mod ships, and the rules for their paths.
//! - [`netplay`]: whether a package counts toward the mod set netplay peers
//!   compare.
//! - [`package`]: packing and opening package zips.
//! - [`catalog`]: the list of packages a registry offers.
//! - [`symbols`]: the game symbols the SDK lets mods name, and each hook's
//!   canonical name.
//! - [`links`]: whether a library would link when the game loads it.
//! - [`port`]: a game executable, and which mods fit it.
//! - [`sdk`]: the game's SDK, and the compiler command that builds a mod
//!   against it.
//! - [`install`]: the mods folder, load order, and what stops a mod from
//!   loading beside others.

pub mod catalog;
pub mod decls;
pub mod depends;
pub mod files;
pub mod install;
pub mod links;
pub mod manifest;
pub mod netplay;
pub mod package;
pub mod port;
pub mod sdk;
pub mod symbols;

pub use catalog::{Catalog, CatalogEntry, PackageRef};
pub use decls::{Declarations, Hooks};
pub use depends::Range;
pub use files::{Files, ModFile};
pub use install::{Conflict, Installed, ModsDir, Overlap, conflicts, load_order, overlaps, unmet};
pub use manifest::{Manifest, ModId};
pub use netplay::Netplay;
pub use package::Package;
pub use port::Port;
pub use sdk::Sdk;
pub use symbols::Symbols;

/// The manifest API this crate's packages target.
pub const API: &str = "tgg-melee/0";
