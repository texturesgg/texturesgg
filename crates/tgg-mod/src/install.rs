//! The game's mods folder: what is installed, installing and removing,
//! turning mods on and off, the order the game loads them in, and what stops
//! a mod from loading beside the others.
//!
//! An installed mod is a folder named by its id holding its manifest, its
//! library and its `files/`, `assets/` and `include/`, the layout the game
//! loads. A turned-off mod's folder is `.<id>`: the game skips any folder
//! whose name starts with a dot.

use crate::files::path_key;
use crate::manifest::{Manifest, ModId};
use crate::package::Package;
use crate::symbols::Symbols;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// `~` can't appear in an id, so these never name a turned-off mod.
const STAGING: &str = ".~installing-";
const PREVIOUS: &str = ".~previous-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}

/// Two mods that both replace one function: the one that loads later is
/// refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The function, by its canonical name when the game's symbols were at
    /// hand.
    pub replaces: String,
    /// The installed mod's id.
    pub with: ModId,
}

/// Two mods that ship one disc file: the one that loads later wins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlap {
    /// The candidate's spelling of the path.
    pub path: String,
    pub with: ModId,
    /// Whichever of the two loads later.
    pub wins: ModId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModsDir {
    root: PathBuf,
}

impl ModsDir {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder the game loads mods from: `$TGG_MODS_DIR`, else
    /// `$XDG_DATA_HOME/tgg-melee/mods`, else `~/.local/share/tgg-melee/mods`,
    /// in that order, as the game resolves it. Every installed version of the
    /// game shares it.
    pub fn game() -> Self {
        let env = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        let root = match (env("TGG_MODS_DIR"), env("XDG_DATA_HOME")) {
            (Some(dir), _) => PathBuf::from(dir),
            (None, Some(data)) => PathBuf::from(data).join("tgg-melee/mods"),
            (None, None) => PathBuf::from(env("HOME").unwrap_or_else(|| ".".into()))
                .join(".local/share/tgg-melee/mods"),
        };
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the mod `id` is installed, turned on or off.
    pub fn folder(&self, id: &ModId, enabled: bool) -> PathBuf {
        if enabled {
            self.root.join(id.as_str())
        } else {
            self.root.join(format!(".{id}"))
        }
    }

    /// Where an install keeps the version it replaces until the new one is
    /// in place. The name records whether that version was on.
    fn previous(&self, id: &ModId, enabled: bool) -> PathBuf {
        let state = if enabled { "on" } else { "off" };
        self.root.join(format!("{PREVIOUS}{state}-{id}"))
    }

    /// Finish what an interrupted install left: put back a replaced version
    /// whose replacement never landed, and drop leftover staging folders.
    fn recover(&self) -> std::io::Result<()> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name.starts_with(STAGING) {
                remove_if_present(&self.root.join(&name))?;
                continue;
            }
            let Some(rest) = name.strip_prefix(PREVIOUS) else {
                continue;
            };
            let (enabled, id) = match (rest.strip_prefix("on-"), rest.strip_prefix("off-")) {
                (Some(id), _) => (true, id),
                (_, Some(id)) => (false, id),
                _ => continue,
            };
            let Ok(id) = id.parse::<ModId>() else {
                continue;
            };
            let previous = self.root.join(&name);
            if self.folder(&id, true).exists() || self.folder(&id, false).exists() {
                remove_if_present(&previous)?;
            } else {
                std::fs::rename(&previous, self.folder(&id, enabled))?;
            }
        }
        Ok(())
    }

    /// Every installed mod whose manifest reads, sorted by id.
    pub fn list(&self) -> std::io::Result<Vec<Installed>> {
        self.recover()?;
        let mut installed = Vec::new();
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(installed),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let (enabled, id) = match name.strip_prefix('.') {
                Some(id) => (false, id.to_owned()),
                None => (true, name),
            };
            let Ok(json) = std::fs::read(entry.path().join("manifest.json")) else {
                continue;
            };
            // The game refuses a folder whose manifest names another id.
            if let Ok(manifest) = Manifest::parse(&json)
                && manifest.id == *id.as_str()
            {
                installed.push(Installed { manifest, enabled });
            }
        }
        installed.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        Ok(installed)
    }

    /// Install `package`, replacing any installed version and keeping it
    /// off if the player had turned it off. The new files are written beside
    /// the old and swapped in, and the old folder is put back if the swap
    /// fails, so a failed install leaves the old version.
    pub fn install(&self, package: &Package) -> std::io::Result<()> {
        self.recover()?;
        let id = &package.manifest.id;
        let staging = self.root.join(format!("{STAGING}{id}"));
        remove_if_present(&staging)?;
        std::fs::create_dir_all(&staging)?;
        if let Some(library) = &package.library {
            std::fs::write(staging.join(package.manifest.library_name()), library)?;
        }
        for (folder, files) in [
            ("files", &package.files),
            ("assets", &package.assets),
            ("include", &package.include),
        ] {
            for (path, bytes) in files {
                // Packages check their paths, so each stays inside its folder.
                let file = staging.join(folder).join(path);
                std::fs::create_dir_all(file.parent().expect("a file has a parent"))?;
                std::fs::write(file, bytes)?;
            }
        }
        std::fs::write(staging.join("manifest.json"), package.manifest.to_json())?;

        let current = [true, false]
            .into_iter()
            .map(|enabled| (enabled, self.folder(id, enabled)))
            .find(|(_, folder)| folder.exists());
        let Some((enabled, current)) = current else {
            return std::fs::rename(&staging, self.folder(id, true));
        };
        let previous = self.previous(id, enabled);
        remove_if_present(&previous)?;
        std::fs::rename(&current, &previous)?;
        if let Err(error) = std::fs::rename(&staging, &current) {
            std::fs::rename(&previous, &current)?;
            return Err(error);
        }
        remove_if_present(&previous)
    }

    /// Remove the mod `id`, on or off. Nothing to remove is not an error.
    pub fn remove(&self, id: &ModId) -> std::io::Result<()> {
        for enabled in [true, false] {
            remove_if_present(&self.folder(id, enabled))?;
        }
        Ok(())
    }

    /// Turn the installed mod `id` on or off.
    pub fn set_enabled(&self, id: &ModId, enabled: bool) -> std::io::Result<()> {
        let from = self.folder(id, !enabled);
        if !from.exists() {
            return Ok(());
        }
        std::fs::rename(from, self.folder(id, enabled))
    }
}

fn remove_if_present(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The ids of `mods` in the order the game loads them: by id, compared byte
/// by byte, except that a mod loads after every mod it depends on. A mod whose
/// dependencies never all load is left out.
pub fn load_order(mods: &[&Manifest]) -> Vec<ModId> {
    let present: BTreeSet<&ModId> = mods.iter().map(|m| &m.id).collect();
    let mut sorted: Vec<&Manifest> = mods.to_vec();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    let mut order: Vec<ModId> = Vec::new();
    loop {
        // The first mod by id whose dependencies are all in the order.
        let next = sorted.iter().find(|m| {
            !order.contains(&m.id)
                && m.depends
                    .keys()
                    .all(|dep| present.contains(dep) && order.contains(dep))
        });
        match next {
            Some(m) => order.push(m.id.clone()),
            None => return order,
        }
    }
}

/// The turned-on installed mods `candidate` can't load beside, other than
/// another version of itself: both replacing one function. Hooking the same
/// function before or after is fine. With the game's `symbols`, functions
/// compare by canonical name, as the game compares them.
pub fn conflicts(
    candidate: &Manifest,
    installed: &[Installed],
    symbols: Option<&Symbols>,
) -> Vec<Conflict> {
    let canonical = |name: &String| match symbols {
        Some(symbols) => symbols.hook(name).unwrap_or_else(|_| name.clone()),
        None => name.clone(),
    };
    let replaces: Vec<String> = candidate.hooks.replaces.iter().map(canonical).collect();
    installed
        .iter()
        .filter(|other| other.enabled && other.manifest.id != candidate.id)
        .flat_map(|other| {
            let theirs: BTreeSet<String> = other
                .manifest
                .hooks
                .replaces
                .iter()
                .map(canonical)
                .collect();
            replaces
                .iter()
                .filter(move |name| theirs.contains(*name))
                .map(|name| Conflict {
                    replaces: name.clone(),
                    with: other.manifest.id.clone(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The disc files `candidate` ships that a turned-on installed mod ships
/// too (paths compare without case), and which of the two the game serves.
pub fn overlaps(candidate: &Manifest, installed: &[Installed]) -> Vec<Overlap> {
    let mut mods: Vec<&Manifest> = installed
        .iter()
        .filter(|other| other.enabled && other.manifest.id != candidate.id)
        .map(|other| &other.manifest)
        .collect();
    mods.push(candidate);
    let order = load_order(&mods);
    let position = |id: &ModId| order.iter().position(|o| o == id);
    mods.iter()
        .filter(|other| other.id != candidate.id)
        .flat_map(|other| {
            candidate
                .files
                .iter()
                .filter(|file| {
                    let key = path_key(&file.path);
                    other.files.iter().any(|o| path_key(&o.path) == key)
                })
                .map(|file| Overlap {
                    path: file.path.clone(),
                    with: other.id.clone(),
                    wins: if position(&candidate.id) > position(&other.id) {
                        candidate.id.clone()
                    } else {
                        other.id.clone()
                    },
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// What stops `candidate` from loading beside the turned-on installed mods:
/// each dependency that isn't installed and on, or whose version is out of
/// range, and each import no turned-on mod exports.
pub fn unmet(candidate: &Manifest, installed: &[Installed]) -> Vec<String> {
    let on = |id: &str| {
        installed
            .iter()
            .find(|other| other.enabled && other.manifest.id == *id)
    };
    let mut unmet = Vec::new();
    for (id, range) in &candidate.depends {
        match on(id.as_str()) {
            None => unmet.push(format!("{id} {range}")),
            Some(other) if !range.matches(&other.manifest.version) => unmet.push(format!(
                "{id} {range} ({} is installed)",
                other.manifest.version
            )),
            Some(_) => {}
        }
    }
    for import in &candidate.imports {
        let met = import.split_once('/').is_some_and(|(provider, name)| {
            on(provider).is_some_and(|other| other.manifest.exports.iter().any(|e| e == name))
        });
        if !met {
            unmet.push(import.clone());
        }
    }
    unmet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decls::Hooks;
    use crate::files::ModFile;

    fn manifest(id: &str) -> Manifest {
        let mut manifest = Manifest::new(
            id.parse().expect("id"),
            id.into(),
            semver::Version::new(1, 0, 0),
        );
        manifest.entry = Some("mod.so".into());
        manifest
    }

    fn file(path: &str) -> ModFile {
        ModFile {
            path: path.into(),
            size: 1,
            sha256: String::new(),
        }
    }

    fn on(manifest: Manifest) -> Installed {
        Installed {
            manifest,
            enabled: true,
        }
    }

    #[test]
    fn replacing_one_function_twice_conflicts_by_canonical_name() {
        let symbols = Symbols::parse("func jump.c:ftCo_Jump_Anim\n").expect("symbols");
        let mut replacer = manifest("a.replacer");
        replacer.hooks = Hooks {
            replaces: vec!["jump.c:ftCo_Jump_Anim".into()],
            ..Default::default()
        };
        let mut candidate = manifest("d.new");
        candidate.hooks = Hooks {
            replaces: vec!["ftCo_Jump_Anim".into()],
            ..Default::default()
        };
        let installed = [on(replacer)];
        assert_eq!(
            conflicts(&candidate, &installed, Some(&symbols)),
            [Conflict {
                replaces: "jump.c:ftCo_Jump_Anim".into(),
                with: "a.replacer".parse().expect("id"),
            }]
        );
    }

    #[test]
    fn a_shared_file_goes_to_the_mod_that_loads_later() {
        let mut base = manifest("z.base");
        base.files = vec![file("PlMrNr.dat")];
        // a.skin sorts first, but loads after z.base because it depends on it.
        let mut skin = manifest("a.skin");
        skin.files = vec![file("plmrnr.dat")];
        skin.depends
            .insert("z.base".parse().expect("id"), "^1".parse().expect("range"));
        let overlap = &overlaps(&skin, &[on(base)])[0];
        assert_eq!(overlap.wins, *"a.skin");
    }

    #[test]
    fn a_dependency_must_be_on_and_in_range() {
        let mut core = manifest("ref.core");
        core.version = semver::Version::new(2, 0, 0);
        core.exports = vec!["register_clone@1".into()];
        let mut user = manifest("ref.user");
        user.depends.insert(
            "ref.core".parse().expect("id"),
            "^1.2".parse().expect("range"),
        );
        user.imports = vec!["ref.core/register_clone@1".into()];
        let installed = [on(core)];
        assert_eq!(
            unmet(&user, &installed),
            ["ref.core ^1.2 (2.0.0 is installed)"]
        );
    }

    #[test]
    fn a_crash_mid_install_leaves_the_old_version_restorable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mods = ModsDir::new(dir.path().join("mods"));
        let package = Package {
            manifest: manifest("a.mod"),
            library: Some(b"v1".to_vec()),
            files: Default::default(),
            assets: Default::default(),
            include: Default::default(),
        };
        mods.install(&package).expect("install");
        let id = package.manifest.id.clone();
        mods.set_enabled(&id, false).expect("turn off");
        // The state between moving the old version aside and moving the new
        // one in.
        std::fs::rename(mods.folder(&id, false), mods.previous(&id, false)).expect("move aside");
        let installed = mods.list().expect("list");
        assert_eq!(installed.len(), 1);
        assert!(!installed[0].enabled);
    }

    #[test]
    fn an_update_keeps_a_turned_off_mod_off() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mods = ModsDir::new(dir.path().join("mods"));
        let mut package = Package {
            manifest: manifest("a.mod"),
            library: Some(b"v1".to_vec()),
            files: Default::default(),
            assets: Default::default(),
            include: Default::default(),
        };
        mods.install(&package).expect("install");
        let id = package.manifest.id.clone();
        mods.set_enabled(&id, false).expect("turn off");
        package.manifest.version = semver::Version::new(2, 0, 0);
        package.library = Some(b"v2".to_vec());
        mods.install(&package).expect("update");
        let installed = mods.list().expect("list");
        assert_eq!(installed.len(), 1);
        assert!(!installed[0].enabled);
        assert_eq!(installed[0].manifest.version, semver::Version::new(2, 0, 0));
        assert_eq!(
            std::fs::read(dir.path().join("mods/.a.mod/mod.so")).expect("library"),
            b"v2"
        );
    }
}
