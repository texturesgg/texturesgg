//! Package zips: `manifest.json`, the library it names, and the game files
//! under `files/`.
//!
//! [`Package::pack`] fills the manifest's hooks, exports, imports and game
//! layout from the library, and its list of files from the files.
//! [`Package::from_zip`] reads them all again and refuses a package whose
//! manifest disagrees, so a catalog or installer can trust a package's
//! manifest once it opens. A package without a library ships only files: it
//! names no entry and declares no game layout, hooks or exports.

use crate::decls::{self, DeclError};
use crate::files::{self, FileError, Files};
use crate::manifest::{DEFAULT_ENTRY, Manifest, ManifestError};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

/// The largest library a package may hold.
pub const LIBRARY_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    /// Absent for a mod that only ships files.
    pub library: Option<Vec<u8>>,
    pub files: Files,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Declarations(#[from] DeclError),
    #[error("the library declares no game layout; build it with tgg_add_mod")]
    NoGameLayout,
    #[error("the library declares no target; build it with the SDK's flags")]
    NoTarget,
    #[error(
        "the manifest's hooks, exports, imports, game layout or target differ from its library's"
    )]
    Mismatch,
    #[error("not a package zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the package has no {0}")]
    MissingFile(String),
    #[error(transparent)]
    Files(#[from] FileError),
    #[error("the manifest's files differ from the package's files/")]
    FilesMismatch,
    #[error("a mod without a library names no entry and declares no game layout, hooks or exports")]
    FilesOnlyDeclares,
    #[error("a package needs a library or files")]
    Empty,
    #[error("{0} in the package is too large")]
    TooLarge(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Package {
    /// A package of `library` and `files` described by `manifest`, with the
    /// manifest's hooks and game layout taken from the library and its list
    /// of files from the files. Without a library, the mod ships only files.
    pub fn pack(
        library: Option<Vec<u8>>,
        mut manifest: Manifest,
        files: Files,
    ) -> Result<Self, PackageError> {
        manifest.validate()?;
        files::check(&files)?;
        manifest.files = files::list(&files);
        match &library {
            Some(library) => {
                let declared = decls::read(library)?;
                manifest.entry = Some(manifest.library_name().to_owned());
                manifest.game_abi = Some(declared.game_abi.ok_or(PackageError::NoGameLayout)?);
                manifest.target = Some(declared.target.ok_or(PackageError::NoTarget)?);
                manifest.state = (declared.state > 0).then_some(declared.state);
                manifest.hooks = declared.hooks;
                manifest.exports = declared.exports;
                manifest.imports = declared.imports;
            }
            None if files.is_empty() => return Err(PackageError::Empty),
            None => {
                if manifest.entry.is_some() {
                    return Err(PackageError::MissingFile(
                        manifest.library_name().to_owned(),
                    ));
                }
                manifest.game_abi = None;
                manifest.target = None;
                manifest.state = None;
                manifest.hooks = Default::default();
                manifest.exports = Vec::new();
                manifest.imports = Vec::new();
            }
        }
        Ok(Self {
            manifest,
            library,
            files,
        })
    }

    /// Open a package zip, checking its manifest against its library and
    /// files.
    pub fn from_zip(bytes: &[u8]) -> Result<Self, PackageError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let manifest =
            Manifest::parse(&read_entry(&mut archive, "manifest.json", MANIFEST_LIMIT)?)?;
        // With no entry named, a package still has a library if it holds the
        // default one.
        let library = match &manifest.entry {
            Some(entry) => Some(read_entry(&mut archive, entry, LIBRARY_LIMIT)?),
            None if archive.index_for_name(DEFAULT_ENTRY).is_some() => {
                Some(read_entry(&mut archive, DEFAULT_ENTRY, LIBRARY_LIMIT)?)
            }
            None => None,
        };
        match &library {
            Some(library) => {
                let declared = decls::read(library)?;
                if declared.game_abi.is_none() {
                    return Err(PackageError::NoGameLayout);
                }
                if declared.target.is_none() {
                    return Err(PackageError::NoTarget);
                }
                if declared.game_abi != manifest.game_abi
                    || declared.target != manifest.target
                    || (declared.state > 0).then_some(declared.state) != manifest.state
                    || declared.hooks != manifest.hooks
                    || declared.exports != manifest.exports
                    || declared.imports != manifest.imports
                {
                    return Err(PackageError::Mismatch);
                }
            }
            None => {
                if manifest.game_abi.is_some()
                    || manifest.target.is_some()
                    || manifest.state.is_some()
                    || !manifest.hooks.is_empty()
                    || !manifest.exports.is_empty()
                    || !manifest.imports.is_empty()
                {
                    return Err(PackageError::FilesOnlyDeclares);
                }
            }
        }

        let mut files = Files::new();
        for listed in &manifest.files {
            files::check_path(&listed.path)?;
            let name = format!("files/{}", listed.path);
            let bytes = read_entry(&mut archive, &name, files::FILE_LIMIT - 1)?;
            if bytes.len() as u64 != listed.size || sha256_hex(&bytes) != listed.sha256 {
                return Err(PackageError::FilesMismatch);
            }
            files.insert(listed.path.clone(), bytes);
        }
        let shipped = archive
            .file_names()
            .filter(|name| name.starts_with("files/") && !name.ends_with('/'))
            .count();
        if shipped != files.len() || files::list(&files) != manifest.files {
            return Err(PackageError::FilesMismatch);
        }
        files::check(&files)?;
        if library.is_none() && files.is_empty() {
            return Err(PackageError::Empty);
        }
        Ok(Self {
            manifest,
            library,
            files,
        })
    }

    /// The package as a zip. The same package always gives the same bytes
    /// from the same build of this crate; the deflate encoder is part of that
    /// build, so a registry packs with one pinned build.
    pub fn to_zip(&self) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644)
            .large_file(
                self.files
                    .values()
                    .any(|bytes| bytes.len() as u64 >= u32::MAX as u64),
            );
        let mut write = |name: &str, bytes: &[u8]| {
            writer.start_file(name, options).expect("write to memory");
            writer.write_all(bytes).expect("write to memory");
        };
        write("manifest.json", self.manifest.to_json().as_bytes());
        if let Some(library) = &self.library {
            write(self.manifest.library_name(), library);
        }
        for (path, bytes) in &self.files {
            write(&format!("files/{path}"), bytes);
        }
        writer.finish().expect("write to memory").into_inner()
    }
}

fn read_entry(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, PackageError> {
    let file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => {
            return Err(PackageError::MissingFile(name.to_owned()));
        }
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(PackageError::TooLarge(name.to_owned()));
    }
    Ok(bytes)
}

/// Lowercase hex SHA-256 of `bytes`, as catalogs name packages.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decls::tests::{
        BEFORE, EXPORT, GAME_ABI, IMPORT, REPLACE, SYMBOL, TARGET, library, record,
    };
    use crate::manifest::Netplay;

    fn manifest() -> Manifest {
        Manifest {
            api: "tgg/1".into(),
            id: "test.mod".parse().expect("id"),
            name: "Test".into(),
            version: semver::Version::new(1, 0, 0),
            entry: None,
            netplay: Netplay::Gameplay,
            description: None,
            license: None,
            game_abi: None,
            target: None,
            state: None,
            hooks: Default::default(),
            exports: Vec::new(),
            imports: Vec::new(),
            files: Vec::new(),
        }
    }

    #[test]
    fn hooks_come_from_the_library_and_a_manifest_claiming_others_is_refused() {
        let lib = library(&[
            record(GAME_ABI, "e954031487f52421"),
            record(TARGET, "x86_64-linux-gnu"),
            crate::decls::tests::state_record("counter", 4),
            crate::decls::tests::state_record("table", 60),
            record(REPLACE, "ftCo_Landing_IASA"),
            record(BEFORE, "ftCo_Jump.c:ftCo_Jump_Anim"),
            record(EXPORT, "register_clone"),
            record(IMPORT, "ref.core/clone_count"),
            record(SYMBOL, "ftCo_Damage.c:ftCo_803C1A20"),
        ]);
        let package = Package::pack(Some(lib), manifest(), Files::new()).expect("pack");
        assert_eq!(package.manifest.hooks.replaces, ["ftCo_Landing_IASA"]);
        assert_eq!(
            package.manifest.hooks.before,
            ["ftCo_Jump.c:ftCo_Jump_Anim"]
        );
        assert_eq!(
            package.manifest.game_abi.as_deref(),
            Some("e954031487f52421")
        );
        assert_eq!(package.manifest.exports, ["register_clone"]);
        assert_eq!(package.manifest.state, Some(64));
        assert_eq!(package.manifest.imports, ["ref.core/clone_count"]);
        assert_eq!(Package::from_zip(&package.to_zip()).expect("open"), package);

        // A manifest that hides a replacement must not open.
        let mut lying = package.clone();
        lying.manifest.hooks.replaces.clear();
        assert!(matches!(
            Package::from_zip(&lying.to_zip()),
            Err(PackageError::Mismatch)
        ));
    }

    #[test]
    fn files_travel_in_the_package_and_a_mod_may_ship_only_files() {
        let files = Files::from([
            ("PlMrNr.dat".to_owned(), b"mario".to_vec()),
            ("Sd/Custom.dat".to_owned(), b"stage".to_vec()),
        ]);
        let lib = library(&[
            record(GAME_ABI, "e954031487f52421"),
            record(TARGET, "x86_64-linux-gnu"),
        ]);
        let package = Package::pack(Some(lib), manifest(), files.clone()).expect("pack");
        assert_eq!(package.manifest.files.len(), 2);
        assert_eq!(package.manifest.files[0].path, "PlMrNr.dat");
        assert_eq!(Package::from_zip(&package.to_zip()).expect("open"), package);

        let only = Package::pack(None, manifest(), files).expect("pack files only");
        assert_eq!(only.manifest.entry, None);
        assert_eq!(only.manifest.game_abi, None);
        assert_eq!(Package::from_zip(&only.to_zip()).expect("open"), only);
        assert!(matches!(
            Package::pack(None, manifest(), Files::new()),
            Err(PackageError::Empty)
        ));

        // A file that differs from the manifest's list must not open.
        let mut swapped = only.clone();
        swapped.files.insert("PlMrNr.dat".into(), b"luigi".to_vec());
        assert!(matches!(
            Package::from_zip(&swapped.to_zip()),
            Err(PackageError::FilesMismatch)
        ));
    }
}
