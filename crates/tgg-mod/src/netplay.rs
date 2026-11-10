//! Whether a mod counts toward the mod-set id netplay peers compare, as far
//! as its package shows.
//!
//! Authors don't declare it; the game works it out at load and logs each
//! mod's class. A mod with code counts, unless the registry stamped it
//! display on review and its records allow that ([`display_check`]). So does one that ships a disc file
//! that can affect play: every file but the menu and trophy archives in the
//! disc's root (`Mn*.dat`, `Mn*.usd`, `Ty*.dat`, `Ty*.usd`). Assets are new
//! files at `/mods/<id>/`, and never count on their own. A costume is the one
//! file the game looks inside: it is free when its joint tree matches the
//! disc's, which takes the disc to tell, so a package of costumes is
//! [`Netplay::Costumes`]. A mod that a counted mod depends on also counts, which
//! depends on the mods installed beside it, not on its package.

use crate::manifest::{Manifest, ModClass};
use serde::{Deserialize, Serialize};

/// The class a package's own contents give it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Netplay {
    /// It has a library: it counts.
    Code,
    /// It ships a disc file that can affect play: it counts.
    Files,
    /// The only files it ships that could count are fighter files
    /// (`Pl*.dat`, `Pl*.usd`): it counts unless each differs from the disc's
    /// only in how it looks, which the game checks at load.
    Costumes,
    /// It ships only menu and trophy files and assets: it is free.
    Data,
    /// A moderator reviewed its source as changing only what a player sees,
    /// and its records allow that ([`display_check`]): it is free.
    Display,
    /// Its author declared it offline: the game switches it off for a whole
    /// netplay session, so it is free.
    Offline,
}

impl Netplay {
    /// Whether peers must match it to play each other. `Costumes` counts
    /// until the game has compared them with the disc.
    pub fn counts(self) -> bool {
        !matches!(self, Self::Data | Self::Display | Self::Offline)
    }
}

/// A disc path (no leading slash) netplay peers may differ on.
fn is_free(path: &str) -> bool {
    root_name_matches(path, &["mn", "ty"], &[".dat", ".usd"])
}

/// A fighter file the game can clear by looking inside it.
fn is_costume(path: &str) -> bool {
    root_name_matches(path, &["pl"], &[".dat", ".usd"])
}

/// Whether `path` is a file in the disc's root whose name starts with one of
/// `prefixes` and ends with one of `suffixes`, without case.
fn root_name_matches(path: &str, prefixes: &[&str], suffixes: &[&str]) -> bool {
    if path.contains('/') {
        return false;
    }
    let name = path.to_ascii_lowercase();
    prefixes.iter().any(|prefix| name.starts_with(prefix))
        && suffixes
            .iter()
            .any(|suffix| name.len() >= 2 + suffix.len() && name.ends_with(suffix))
}

/// The events that rewrite a player's input before netplay sends it. A change
/// there reaches both games the same way, so frame checksums can't catch it.
const INPUT_EVENTS: &[&str] = &["pad_read"];

/// Whether a library's records allow the display class: no function it
/// replaces and no input it rewrites. Anything else a library changes in the
/// match makes the two games differ, which the frame checksums catch; review
/// is what keeps such code out. Says why not, in the player's words.
pub fn display_check(manifest: &Manifest) -> Result<(), String> {
    if let Some(replaced) = manifest.hooks.replaces.first() {
        return Err(format!("it replaces a game function ({replaced})"));
    }
    if let Some(event) = manifest
        .events
        .iter()
        .find(|e| INPUT_EVENTS.contains(&e.as_str()))
    {
        return Err(format!("it rewrites input (the {event} event)"));
    }
    Ok(())
}

/// The class of the package `manifest` describes.
pub fn classify(manifest: &Manifest) -> Netplay {
    if manifest.class == Some(ModClass::Offline) {
        return Netplay::Offline;
    }
    if manifest.class == Some(ModClass::Display) && display_check(manifest).is_ok() {
        return Netplay::Display;
    }
    if manifest.entry.is_some() || manifest.game_abi.is_some() {
        return Netplay::Code;
    }
    let counting = manifest.files.iter().filter(|file| !is_free(&file.path));
    let mut costumes = false;
    for file in counting {
        if !is_costume(&file.path) {
            return Netplay::Files;
        }
        costumes = true;
    }
    if costumes {
        Netplay::Costumes
    } else {
        Netplay::Data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::ModFile;

    #[test]
    fn a_package_counts_unless_it_ships_only_menus_trophies_and_assets() {
        let file = |path: &str| ModFile {
            path: path.into(),
            size: 1,
            sha256: String::new(),
        };
        let with = |files: &[&str]| {
            let mut manifest = Manifest::new(
                "me.x".parse().expect("id"),
                "X".into(),
                semver::Version::new(1, 0, 0),
            );
            manifest.files = files.iter().map(|path| file(path)).collect();
            manifest.assets = vec![file("icon.png")];
            classify(&manifest)
        };
        assert_eq!(with(&[]), Netplay::Data);
        assert_eq!(with(&["MnSlChr.usd", "TyMario.dat"]), Netplay::Data);
        assert_eq!(with(&["MnSlChr.usd", "PlMrNr.dat"]), Netplay::Costumes);
        assert_eq!(with(&["PlMrNr.dat", "GrNBa.dat"]), Netplay::Files);
        // Only the disc's root holds the menu archives.
        assert_eq!(with(&["audio/MnSlChr.usd"]), Netplay::Files);
    }

    #[test]
    fn a_display_stamp_holds_only_while_the_records_allow_it() {
        let mut manifest = Manifest::new(
            "me.hud".parse().expect("id"),
            "HUD".into(),
            semver::Version::new(1, 0, 0),
        );
        manifest.entry = Some("mod.so".into());
        manifest.events = vec!["hud_draw".into()];
        assert_eq!(classify(&manifest), Netplay::Code);
        manifest.class = Some(ModClass::Display);
        assert_eq!(classify(&manifest), Netplay::Display);
        manifest.events.push("pad_read".into());
        assert_eq!(classify(&manifest), Netplay::Code, "input rewriting counts");
        manifest.events.pop();
        manifest.hooks.replaces = vec!["ftCo_Damage".into()];
        assert_eq!(
            classify(&manifest),
            Netplay::Code,
            "a replaced function counts"
        );
    }

    #[test]
    fn an_offline_mod_is_free_whatever_its_records() {
        let mut manifest = Manifest::new(
            "me.save-states".parse().expect("id"),
            "Save states".into(),
            semver::Version::new(1, 0, 0),
        );
        manifest.entry = Some("mod.so".into());
        manifest.events = vec!["pad_read".into()];
        manifest.hooks.replaces = vec!["ftCo_Damage".into()];
        assert_eq!(classify(&manifest), Netplay::Code);
        // The game switches it off for a netplay session, so nothing it does
        // reaches one
        manifest.class = Some(ModClass::Offline);
        assert_eq!(classify(&manifest), Netplay::Offline);
        assert!(!Netplay::Offline.counts());
    }
}
