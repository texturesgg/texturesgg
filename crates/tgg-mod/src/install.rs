//! A port's `mods/` folder: what is installed, installing and removing,
//! turning mods on and off, conflicts between mods, and imports a mod needs.
//!
//! An installed mod is a folder named by its id holding its manifest and
//! library, the layout the runtime loads. A turned-off mod moves under
//! `mods/.disabled/`, which the runtime skips.

use crate::manifest::Manifest;
use crate::package::Package;
use std::path::{Path, PathBuf};

const DISABLED: &str = ".disabled";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
}

/// Two mods that can't both load: both replace `symbol`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub symbol: String,
    /// The installed mod's id.
    pub with: String,
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

    fn folder(&self, id: &str, enabled: bool) -> PathBuf {
        if enabled {
            self.root.join(id)
        } else {
            self.root.join(DISABLED).join(id)
        }
    }

    /// Every installed mod whose manifest reads, sorted by id: the order the
    /// runtime loads them in.
    pub fn list(&self) -> std::io::Result<Vec<Installed>> {
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
                let Ok(json) = std::fs::read_to_string(entry.path().join("manifest.json")) else {
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

    fn find(&self, id: &str) -> std::io::Result<Option<Installed>> {
        Ok(self.list()?.into_iter().find(|mod_| mod_.manifest.id == id))
    }

    /// Install `package`, replacing any installed version and keeping it
    /// off if the player had turned it off. The new files are written beside
    /// the old and swapped in, so a failed write leaves the old version.
    pub fn install(&self, package: &Package) -> std::io::Result<()> {
        let id = &package.manifest.id;
        let enabled = self.find(id)?.is_none_or(|mod_| mod_.enabled);
        let staging = self.root.join(format!(".installing-{id}"));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        std::fs::write(staging.join(&package.manifest.entry), &package.library)?;
        std::fs::write(staging.join("manifest.json"), package.manifest.to_json())?;
        self.remove(id)?;
        let target = self.folder(id, enabled);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&staging, &target)
    }

    /// Remove the mod `id`, on or off. Nothing to remove is not an error.
    pub fn remove(&self, id: &str) -> std::io::Result<()> {
        for enabled in [true, false] {
            match std::fs::remove_dir_all(self.folder(id, enabled)) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    /// Turn the installed mod `id` on or off.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> std::io::Result<()> {
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

/// The turned-on installed mods `candidate` can't load beside, other than
/// another version of itself. Hooking the same function before or after is
/// fine; replacing it twice is not.
pub fn conflicts(candidate: &Manifest, installed: &[Installed]) -> Vec<Conflict> {
    installed
        .iter()
        .filter(|other| other.enabled && other.manifest.id != candidate.id)
        .flat_map(|other| {
            candidate
                .hooks
                .replaces
                .iter()
                .filter(|symbol| other.manifest.hooks.replaces.contains(symbol))
                .map(|symbol| Conflict {
                    symbol: symbol.clone(),
                    with: other.manifest.id.clone(),
                })
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
                    && other.manifest.id == provider
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
            id: id.into(),
            name: id.into(),
            version: "1.0.0".into(),
            entry: "mod.so".into(),
            netplay: Netplay::Gameplay,
            description: None,
            license: None,
            game_abi: Some("layout".into()),
            hooks,
            exports: Vec::new(),
            imports: Vec::new(),
        }
    }

    #[test]
    fn replacing_one_function_twice_conflicts_but_hooking_it_does_not() {
        let replaces = |symbol: &str| Hooks {
            replaces: vec![symbol.into()],
            ..Default::default()
        };
        let installed = vec![
            Installed {
                manifest: manifest("a.replacer", replaces("ftCo_Landing_IASA")),
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
        let candidate = manifest(
            "d.new",
            Hooks {
                replaces: vec!["ftCo_Landing_IASA".into(), "ftCo_Jump_Anim".into()],
                ..Default::default()
            },
        );
        assert_eq!(
            conflicts(&candidate, &installed),
            [Conflict {
                symbol: "ftCo_Landing_IASA".into(),
                with: "a.replacer".into()
            }]
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
    fn an_update_keeps_a_turned_off_mod_off() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mods = ModsDir::new(dir.path().join("mods"));
        let mut package = Package {
            manifest: manifest("a.mod", Hooks::default()),
            library: b"v1".to_vec(),
        };
        mods.install(&package).expect("install");
        mods.set_enabled("a.mod", false).expect("turn off");
        package.manifest.version = "2.0.0".into();
        package.library = b"v2".to_vec();
        mods.install(&package).expect("update");
        let installed = mods.list().expect("list");
        assert_eq!(installed.len(), 1);
        assert!(!installed[0].enabled);
        assert_eq!(installed[0].manifest.version, "2.0.0");
        assert_eq!(
            std::fs::read(dir.path().join("mods/.disabled/a.mod/mod.so")).expect("library"),
            b"v2"
        );
    }
}
