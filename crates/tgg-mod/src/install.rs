//! A port's `mods/` folder: what is installed, installing and removing,
//! turning mods on and off, conflicts between mods, and imports a mod needs.
//!
//! An installed mod is a folder named by its id holding its manifest, its
//! library and its `files/`, the layout the runtime loads. A turned-off mod moves under
//! `mods/.disabled/`, which the runtime skips.

use crate::files::path_key;
use crate::manifest::{Manifest, ModId};
use crate::package::Package;
use std::path::{Path, PathBuf};

const DISABLED: &str = ".disabled";
const STAGING: &str = ".installing-";
const PREVIOUS: &str = ".previous-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}

/// Two mods that can't both load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub clash: Clash,
    /// The installed mod's id.
    pub with: ModId,
}

/// What two conflicting mods both claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clash {
    /// Both replace this function.
    Replaces(String),
    /// Both ship this game file (the candidate's spelling of its path).
    File(String),
}

impl std::fmt::Display for Clash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replaces(symbol) => write!(f, "replaces {symbol}"),
            Self::File(path) => write!(f, "ships files/{path}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModsDir {
    root: PathBuf,
}

impl ModsDir {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn folder(&self, id: &ModId, enabled: bool) -> PathBuf {
        if enabled {
            self.root.join(id.as_str())
        } else {
            self.root.join(DISABLED).join(id.as_str())
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
                let target = self.folder(&id, enabled);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::rename(&previous, target)?;
            }
        }
        Ok(())
    }

    /// Every installed mod whose manifest reads, sorted by id: the order the
    /// runtime loads them in.
    pub fn list(&self) -> std::io::Result<Vec<Installed>> {
        self.recover()?;
        let mut installed = Vec::new();
        for (dir, enabled) in [(self.root.clone(), true), (self.root.join(DISABLED), false)] {
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                let entry = entry?;
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let Ok(json) = std::fs::read(entry.path().join("manifest.json")) else {
                    continue;
                };
                if let Ok(manifest) = Manifest::parse(&json) {
                    installed.push(Installed { manifest, enabled });
                }
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
        for (path, bytes) in &package.files {
            // Packages check their paths, so each stays inside files/.
            let file = staging.join("files").join(path);
            std::fs::create_dir_all(file.parent().expect("a file has a parent"))?;
            std::fs::write(file, bytes)?;
        }
        std::fs::write(staging.join("manifest.json"), package.manifest.to_json())?;

        let current = [true, false]
            .into_iter()
            .map(|enabled| (enabled, self.folder(id, enabled)))
            .find(|(_, folder)| folder.exists());
        let target = self.folder(id, current.as_ref().is_none_or(|(enabled, _)| *enabled));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let Some((enabled, current)) = current else {
            return std::fs::rename(&staging, &target);
        };
        let previous = self.previous(id, enabled);
        remove_if_present(&previous)?;
        std::fs::rename(&current, &previous)?;
        if let Err(error) = std::fs::rename(&staging, &target) {
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
        let to = self.folder(id, enabled);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(from, to)
    }
}

fn remove_if_present(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The turned-on installed mods `candidate` can't load beside, other than
/// another version of itself. Hooking the same function before or after is
/// fine; replacing it twice is not, and neither is shipping the same game
/// file (paths compare without case).
pub fn conflicts(candidate: &Manifest, installed: &[Installed]) -> Vec<Conflict> {
    installed
        .iter()
        .filter(|other| other.enabled && other.manifest.id != candidate.id)
        .flat_map(|other| {
            let with = &other.manifest.id;
            let replaces = candidate
                .hooks
                .replaces
                .iter()
                .filter(|symbol| other.manifest.hooks.replaces.contains(symbol))
                .map(|symbol| Conflict {
                    clash: Clash::Replaces(symbol.clone()),
                    with: with.clone(),
                });
            let files = candidate
                .files
                .iter()
                .filter(|file| {
                    let key = path_key(&file.path);
                    other
                        .manifest
                        .files
                        .iter()
                        .any(|o| path_key(&o.path) == key)
                })
                .map(|file| Conflict {
                    clash: Clash::File(file.path.clone()),
                    with: with.clone(),
                });
            replaces.chain(files).collect::<Vec<_>>()
        })
        .collect()
}

/// The imports of `candidate` that no turned-on installed mod exports. The
/// runtime won't load a mod while any are missing.
pub fn unmet_imports(candidate: &Manifest, installed: &[Installed]) -> Vec<String> {
    candidate
        .imports
        .iter()
        .filter(|import| {
            let Some((provider, name)) = import.split_once('/') else {
                return true;
            };
            !installed.iter().any(|other| {
                other.enabled
                    && other.manifest.id == *provider
                    && other.manifest.exports.iter().any(|export| export == name)
            })
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decls::Hooks;
    use crate::manifest::Netplay;

    fn manifest(id: &str, hooks: Hooks) -> Manifest {
        Manifest {
            api: "tgg/1".into(),
            id: id.parse().expect("id"),
            name: id.into(),
            version: semver::Version::new(1, 0, 0),
            entry: Some("mod.so".into()),
            netplay: Netplay::Gameplay,
            description: None,
            license: None,
            game_abi: Some("layout".into()),
            target: Some("x86_64-linux-gnu".into()),
            state: None,
            hooks,
            exports: Vec::new(),
            imports: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn replacing_one_function_twice_or_shipping_one_file_twice_conflicts() {
        let replaces = |symbol: &str| Hooks {
            replaces: vec![symbol.into()],
            ..Default::default()
        };
        let file = |path: &str| crate::files::ModFile {
            path: path.into(),
            size: 1,
            sha256: String::new(),
        };
        let mut replacer = manifest("a.replacer", replaces("ftCo_Landing_IASA"));
        replacer.files = vec![file("plmrnr.dat")];
        let installed = vec![
            Installed {
                manifest: replacer,
                enabled: true,
            },
            Installed {
                manifest: manifest(
                    "b.hooker",
                    Hooks {
                        after: vec!["ftCo_Jump_Anim".into()],
                        ..Default::default()
                    },
                ),
                enabled: true,
            },
            Installed {
                manifest: manifest("c.off", replaces("ftCo_Jump_Anim")),
                enabled: false,
            },
        ];
        let mut candidate = manifest(
            "d.new",
            Hooks {
                replaces: vec!["ftCo_Landing_IASA".into(), "ftCo_Jump_Anim".into()],
                ..Default::default()
            },
        );
        candidate.files = vec![file("PlMrNr.dat"), file("PlFxNr.dat")];
        let with: ModId = "a.replacer".parse().expect("id");
        assert_eq!(
            conflicts(&candidate, &installed),
            [
                Conflict {
                    clash: Clash::Replaces("ftCo_Landing_IASA".into()),
                    with: with.clone(),
                },
                Conflict {
                    clash: Clash::File("PlMrNr.dat".into()),
                    with,
                },
            ]
        );
    }

    #[test]
    fn an_import_is_met_only_by_a_turned_on_provider_that_exports_it() {
        let mut core = manifest("ref.core", Hooks::default());
        core.exports = vec!["register_clone".into()];
        let mut user = manifest("ref.user", Hooks::default());
        user.imports = vec!["ref.core/register_clone".into(), "ref.core/missing".into()];
        let mut installed = vec![Installed {
            manifest: core,
            enabled: true,
        }];
        assert_eq!(unmet_imports(&user, &installed), ["ref.core/missing"]);
        installed[0].enabled = false;
        assert_eq!(unmet_imports(&user, &installed).len(), 2);
    }

    #[test]
    fn a_crash_mid_install_leaves_the_old_version_restorable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mods = ModsDir::new(dir.path().join("mods"));
        let package = Package {
            manifest: manifest("a.mod", Hooks::default()),
            library: Some(b"v1".to_vec()),
            files: Default::default(),
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
            manifest: manifest("a.mod", Hooks::default()),
            library: Some(b"v1".to_vec()),
            files: Default::default(),
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
            std::fs::read(dir.path().join("mods/.disabled/a.mod/mod.so")).expect("library"),
            b"v2"
        );
    }
}
