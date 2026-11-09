//! Mod packages for [tgg-melee](https://github.com/texturesgg/tgg-melee), the
//! Melee port with a mod loader built in, and getting the game and its mods
//! onto a player's machine.
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
//! - [`port`]: a game executable, and which mods fit it.
//! - [`sdk`]: the game's SDK, which mods are built with.
//! - [`install`]: the mods folder, load order, and what stops a mod from
//!   loading beside others.
//!
//! And distribution, the API front ends (the app, the `tgg` CLI) build on.
//! It is blocking; long work takes a progress callback and a cancel flag:
//!
//! - [`net`]: fetching, behind the [`Net`] trait; [`HttpNet`] over HTTPS and
//!   [`FakeNet`] in memory for tests.
//! - [`ports`]: tgg-melee releases, installed ports, and launching the game.
//! - [`registry`]: the textures.gg registry's sets, per-layout catalog, and
//!   store listings.
//! - [`plan`]: what an install or update will do, and applying it.
//! - [`local`]: what a mods folder holds, turning mods on and off, and
//!   uninstalling.
//! - [`lock`]: `mods.lock.json` and the busy and pid files.
//! - [`profile`], [`prepare`], [`trust`], [`cache`]: mod profiles, the exact
//!   mods a replay or a netplay host needs: reading them ([`slp`] for
//!   replays), resolving, preparing and launching them; and the package
//!   cache.
//! - [`skins`]: skins from the player's library installed as mods of one
//!   file each.
//! - [`scaffold`]: starting a new mod from an SDK example, a release's SDK,
//!   the build tools on `PATH`, and the mod docs' address.

pub mod cache;
pub mod catalog;
pub mod decls;
pub mod depends;
pub mod error;
pub mod fake;
pub mod files;
pub mod install;
pub mod local;
pub mod lock;
pub mod manifest;
pub mod net;
pub mod netplay;
pub mod package;
pub mod plan;
pub mod port;
pub mod ports;
pub mod prepare;
mod process;
pub mod profile;
pub mod registry;
pub mod scaffold;
pub mod sdk;
pub mod skins;
pub mod slp;
pub mod symbols;
pub mod trust;

pub use cache::{
    cached_catalog, cached_mapping, packages_dir, prune_packages, refresh_catalog, refresh_mapping,
    save_catalog, save_mapping,
};
pub use catalog::{Catalog, CatalogEntry, PackageRef};
pub use decls::{Declarations, Hooks};
pub use depends::Range;
pub use error::{Error, OnProgress, Progress, no_progress};
pub use fake::FakeNet;
pub use files::{Files, ModFile};
pub use install::{
    Conflict, Installed, ModsDir, Overlap, conflicts, game_data_dir, load_order, overlaps, unmet,
};
pub use local::{
    Local, LocalMod, LocalSet, SetStatus, local, set_enabled, uninstall, uninstall_set,
};
pub use lock::{Lock, LockEntry, game_running};
pub use manifest::{Manifest, ModId};
pub use net::{HttpNet, Net, NetError};
pub use netplay::Netplay;
pub use package::Package;
pub use plan::{
    Action, Applied, Plan, Problem, Step, apply, apply_cached, plan_mods, plan_set,
    plan_set_update, plan_updates,
};
pub use port::Port;
pub use ports::{
    PortInstall, PortRelease, Running, install_port, installed_ports, launch, port_releases,
    port_update, remove_port,
};
pub use prepare::{
    Prepared, RunningProfile, finish_profile, launch_profile, plan_profile, prepare_profile,
    profile_folder, profile_games,
};
pub use profile::{
    Fit, GameNeed, Missing, Profile, ProfileEntry, ProfileGame, ProfileMod, ProfilePlan, Settings,
    SlippiCode, SlippiFacts, SlippiIgnore, SlippiMapping, profile_from_file, profile_from_local,
    profile_from_slp, profile_layout, resolve_profile, slippi_mapping,
};
pub use registry::{
    Image, ModDetails, ModPage, ModRelease, ModSummary, Publisher, Set, SetMod, System, catalog,
    mod_details, mods, sets,
};
pub use scaffold::{
    CreatedMod, NewMod, Repository, Tools, docs_url, new_mod, release_sdk, sdk_examples, tools,
};
pub use sdk::Sdk;
pub use skins::{Skin, install_skin, is_skin, skin_for, skin_id, skins, uninstall_skin};
pub use slp::{ReplayInfo, ReplayPlayer, replay_info};
pub use symbols::Symbols;
pub use trust::{Trust, remember_trust, trust};

use std::path::PathBuf;

/// The manifest API this crate's packages target.
pub const API: &str = "tgg-melee/0";

/// The folder the game loads mods from, by the game's own rule:
/// `$TGG_MODS_DIR`, else `mods` in [`game_data_dir`].
pub fn default_mods_dir() -> PathBuf {
    ModsDir::game().root().to_owned()
}

/// The folder tgg installs ports into: `tgg/ports` in [`default_data_dir`].
pub fn default_ports_dir() -> PathBuf {
    default_data_dir().join("tgg").join("ports")
}

/// The user's local data folder, which tgg keeps its own folder `tgg/` in
/// (ports, the package cache, profiles): `$XDG_DATA_HOME`, else
/// `~/.local/share` on Linux; `~/Library/Application Support` on macOS;
/// `%LOCALAPPDATA%` on Windows. The `data` the profile functions take.
pub fn default_data_dir() -> PathBuf {
    let env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
    let home = || {
        PathBuf::from(
            env("HOME")
                .or_else(|| env("USERPROFILE"))
                .unwrap_or_else(|| ".".into()),
        )
    };
    if cfg!(windows) {
        env("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("AppData/Local"))
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home().join(".local/share"))
    }
}
