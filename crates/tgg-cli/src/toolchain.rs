//! The tools mods build with on each system: the C compiler tgg uses when
//! none is named, and the CMake generator.
//!
//! The game's headers need GCC (clang lacks `scalar_storage_order`). On
//! macOS `gcc` is Apple's clang, so tgg looks for Homebrew's `gcc-<n>`, as
//! the SDK's toolchain file does. On Windows CMake's default generator is
//! NMake, which MinGW's GCC can't drive, so builds there need Ninja.

use crate::paths::{find_program, program_in};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// Homebrew's GCC programs, in the order the SDK's macOS toolchain file
/// prefers them.
pub const MAC_GCCS: &[&str] = &["gcc-15", "gcc-16", "gcc-14", "gcc-13", "gcc-12"];
/// Where Homebrew puts programs on Apple silicon and on Intel.
pub const HOMEBREW_BIN: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

/// The C compiler to use when none is named: `gcc`, or on macOS Homebrew's
/// newest supported `gcc-<n>`.
pub fn default_cc() -> PathBuf {
    if cfg!(target_os = "macos")
        && let Some(gcc) = mac_gcc()
    {
        return gcc;
    }
    PathBuf::from("gcc")
}

/// Homebrew's GCC on this Mac, on `PATH` or in Homebrew's folder.
pub fn mac_gcc() -> Option<PathBuf> {
    MAC_GCCS.iter().find_map(|name| {
        find_program(name).or_else(|| {
            HOMEBREW_BIN
                .iter()
                .find_map(|dir| program_in(Path::new(dir), name))
        })
    })
}

/// The CMake generator mod builds use: Ninja when it's installed, else
/// CMake's default (make), except on Windows, where only Ninja works.
pub fn generator() -> Result<Option<&'static str>> {
    if find_program("ninja").is_some() {
        return Ok(Some("Ninja"));
    }
    if cfg!(windows) {
        bail!("building mods on Windows needs Ninja: winget install Ninja-build.Ninja");
    }
    Ok(None)
}
