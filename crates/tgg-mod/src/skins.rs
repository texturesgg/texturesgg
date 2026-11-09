//! Skins in tgg-melee: a file from the player's skin library installed into
//! the port as a mod of that one file.
//!
//! Each disc file gets its own mod, `skin.<file name, lowercase>`
//! (`skin.plmrnr.dat` for Mario's default costume), holding the skin at
//! `files/<file name>`. Installing another skin for the same file replaces
//! it in place; turning it off or removing it is turning off or removing
//! that mod. A skin mod has no lock entry, so updates and sets leave it
//! alone, and the registry never offers an id under `skin.`.
//!
//! What it means for netplay is the game's call, made at load
//! ([`crate::netplay`]): a costume that differs from the disc's only in how
//! it looks is free, and every other file counts. Until the game has
//! looked, this crate takes a costume skin to be free, as nearly all are, so
//! it doesn't make the player's own profile ([`crate::profile_from_local`])
//! or a replay's fit ([`crate::resolve_profile`]) ask for it.

use crate::error::Error;
use crate::files::Files;
use crate::install::ModsDir;
use crate::local::{Local, LocalMod};
use crate::lock;
use crate::manifest::{Manifest, ModId, NAME_LIMIT};
use crate::netplay::{self, Netplay};
use crate::package::Package;
use std::path::Path;

/// The id prefix of skin mods.
pub const PREFIX: &str = "skin.";

/// The version every skin mod has. A skin changes in place: its file, not its
/// version, tells two apart.
pub const VERSION: semver::Version = semver::Version::new(1, 0, 0);

/// A skin installed in the port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skin {
    pub id: ModId,
    /// The disc file it replaces, as the skin was installed: `PlMrNr.dat`.
    pub path: String,
    /// The name it was installed with.
    pub name: String,
    pub enabled: bool,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    pub size: u64,
    /// [`Netplay::Costumes`] for a costume (free unless it changes the
    /// skeleton), [`Netplay::Files`] for anything else (stages, effects,
    /// fighter data), which players must match to play each other.
    pub netplay: Netplay,
}

/// Whether the mod `id` is a skin.
pub fn is_skin(id: &ModId) -> bool {
    id.as_str().starts_with(PREFIX)
}

/// The id of the skin mod for the disc file `path` (`PlMrNr.dat`): a
/// `.dat` or `.usd` file in the disc's root.
pub fn skin_id(path: &str) -> Result<ModId, Error> {
    let lower = path.to_ascii_lowercase();
    let archive = (lower.ends_with(".dat") || lower.ends_with(".usd")) && lower.len() > 4;
    if !archive || crate::files::check_path("files", path).is_err() || path.contains('/') {
        return Err(Error::NotASkinFile(path.to_owned()));
    }
    format!("{PREFIX}{lower}")
        .parse()
        .map_err(|_| Error::NotASkinFile(path.to_owned()))
}

/// Install `bytes` as the skin for the disc file `path`, named `name`,
/// replacing the skin installed for that file, if any, and keeping it off if
/// the player turned it off. A name longer than the game reads is cut short.
pub fn install_skin(mods: &Path, path: &str, name: &str, bytes: Vec<u8>) -> Result<Skin, Error> {
    let id = skin_id(path)?;
    let mut name = if name.trim().is_empty() {
        path
    } else {
        name.trim()
    };
    if name.len() > NAME_LIMIT {
        let end = (0..=NAME_LIMIT)
            .rev()
            .find(|&end| name.is_char_boundary(end))
            .unwrap_or(0);
        name = name[..end].trim_end();
    }
    let manifest = Manifest::new(id.clone(), name.to_owned(), VERSION);
    let files = Files::from([(path.to_owned(), bytes)]);
    let package = Package::pack(manifest, None, files, Files::new(), Files::new())
        .map_err(|error| Error::BadSkin(path.to_owned(), error.to_string()))?;
    let _busy = lock::take(mods)?;
    let dir = ModsDir::new(mods.to_owned());
    dir.install(&package).map_err(Error::io(mods))?;
    let enabled = dir.folder(&id, true).exists();
    Ok(skin_of(&package.manifest, enabled).expect("a skin mod ships its file"))
}

/// Remove the skin installed for the disc file `path`.
pub fn uninstall_skin(mods: &Path, path: &str) -> Result<(), Error> {
    crate::local::uninstall(mods, &skin_id(path)?)
}

/// Every skin in `local`, by id.
pub fn skins(local: &Local) -> Vec<Skin> {
    local
        .mods
        .iter()
        .filter_map(|m| skin_of(&m.manifest, m.enabled))
        .collect()
}

/// The skin installed for the disc file `path`, on or off.
pub fn skin_for(local: &Local, path: &str) -> Option<Skin> {
    let id = skin_id(path).ok()?;
    let m = local.get(&id)?;
    skin_of(&m.manifest, m.enabled)
}

fn skin_of(manifest: &Manifest, enabled: bool) -> Option<Skin> {
    if !is_skin(&manifest.id) || manifest.entry.is_some() {
        return None;
    }
    let [file] = manifest.files.as_slice() else {
        return None;
    };
    Some(Skin {
        id: manifest.id.clone(),
        path: file.path.clone(),
        name: manifest.name.clone(),
        enabled,
        sha256: file.sha256.clone(),
        size: file.size,
        netplay: netplay::classify(manifest),
    })
}

/// Whether `m` is a costume skin, which this crate takes to be free for
/// netplay until the game says otherwise.
pub(crate) fn free_costume(m: &LocalMod) -> bool {
    skin_of(&m.manifest, m.enabled).is_some_and(|skin| skin.netplay == Netplay::Costumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skin_is_one_mod_per_disc_file_replaced_in_place() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mods = dir.path();
        let first = install_skin(mods, "PlMrNr.dat", "Red", b"one".to_vec()).expect("install");
        assert_eq!(first.id.as_str(), "skin.plmrnr.dat");
        assert_eq!(first.netplay, Netplay::Costumes);
        assert_eq!(
            std::fs::read(mods.join("skin.plmrnr.dat/files/PlMrNr.dat")).expect("file"),
            b"one"
        );

        crate::set_enabled(mods, &first.id, false).expect("off");
        let second = install_skin(mods, "PlMrNr.dat", "Blue", b"two".to_vec()).expect("replace");
        assert!(!second.enabled, "a replaced skin stays off");
        let long = "Ä".repeat(40);
        let long = install_skin(mods, "PlMrNr.dat", &long, b"two".to_vec()).expect("long");
        assert_eq!(long.name, "Ä".repeat(31), "cut to the game's 63 bytes");
        let second = install_skin(mods, "PlMrNr.dat", "Blue", b"two".to_vec()).expect("rename");
        let stage = install_skin(mods, "GrNBa.dat", "", b"stage".to_vec()).expect("stage");
        assert_eq!(stage.name, "GrNBa.dat");
        assert_eq!(stage.netplay, Netplay::Files);

        let local = crate::local(mods, None).expect("local");
        let names: Vec<_> = skins(&local).into_iter().map(|skin| skin.name).collect();
        assert_eq!(names, ["GrNBa.dat", "Blue"]);
        assert!(local.lock.mods.is_empty(), "skins stay out of the lock");
        // A costume skin stays out of the player's profile; a stage counts.
        crate::set_enabled(mods, &second.id, true).expect("on");
        let local = crate::local(mods, None).expect("local");
        let profile = crate::profile_from_local(&local);
        let ids: Vec<_> = profile.mods.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["skin.grnba.dat"]);

        uninstall_skin(mods, "PlMrNr.dat").expect("remove");
        let local = crate::local(mods, None).expect("local");
        assert!(skin_for(&local, "PlMrNr.dat").is_none());
    }

    #[test]
    fn only_archives_in_the_discs_root_are_skins() {
        assert!(skin_id("PlMrNr.usd").is_ok());
        for path in ["audio/x.dat", "PlMrNr.hps", ".dat", "../PlMrNr.dat"] {
            assert!(
                matches!(skin_id(path), Err(Error::NotASkinFile(_))),
                "{path}"
            );
        }
    }
}
