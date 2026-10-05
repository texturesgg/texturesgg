//! Package zips: `manifest.json`, the library it names, and the mod's
//! `files/`, `assets/` and `include/` folders.
//!
//! [`Package::pack`] fills the manifest's hooks, events, exports, imports,
//! game layout and API version from the library, and its lists of files and
//! assets from the folders. [`Package::from_zip`] reads them all again and
//! refuses a package whose manifest disagrees, so a catalog or installer can
//! trust a package's manifest once it opens. A package without a library
//! ships only files: it names no entry and declares no game layout, hooks or
//! exports.

use crate::decls::{self, DeclError};
use crate::files::{self, FileError, Files};
use crate::manifest::{DEFAULT_ENTRY, Manifest, ManifestError};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

/// The largest library a package may hold.
pub const LIBRARY_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;
/// The largest header a package may hold.
const INCLUDE_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    /// Absent for a mod that only ships files.
    pub library: Option<Vec<u8>>,
    /// Disc files, by disc path.
    pub files: Files,
    /// New files at `/mods/<id>/`, by path.
    pub assets: Files,
    /// Headers for mods that build on this one, by path under `include/`.
    pub include: Files,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Declarations(#[from] DeclError),
    #[error("the library declares no game layout; build it with the SDK")]
    NoGameLayout,
    #[error("the library declares no target; build it with the SDK")]
    NoTarget,
    #[error("the library declares no mod API version; build it with the SDK")]
    NoApiVersion,
    #[error(
        "the manifest's hooks, events, exports, imports, game layout, target or API version differ from its library's"
    )]
    Mismatch,
    #[error("not a package zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the package has no {0}")]
    MissingFile(String),
    #[error(transparent)]
    Files(#[from] FileError),
    #[error("the manifest's {0} differ from the package's {0}/")]
    FilesMismatch(&'static str),
    #[error(
        "a mod without a library names no entry and declares no game layout, hooks, events or exports"
    )]
    FilesOnlyDeclares,
    #[error("a package needs a library, files or assets")]
    Empty,
    #[error("{0} in the package is too large")]
    TooLarge(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Package {
    /// A package described by `manifest`, of `library` and the folders,
    /// with the manifest's packer fields taken from them. Without a library,
    /// the mod ships only files.
    pub fn pack(
        mut manifest: Manifest,
        library: Option<Vec<u8>>,
        files: Files,
        assets: Files,
        include: Files,
    ) -> Result<Self, PackageError> {
        manifest.validate()?;
        files::check("files", &files)?;
        files::check("assets", &assets)?;
        files::check("include", &include)?;
        manifest.files = files::list(&files);
        manifest.assets = files::list(&assets);
        match &library {
            Some(library) => {
                let declared = decls::read(library)?;
                manifest.entry = Some(manifest.library_name().to_owned());
                manifest.game_abi = Some(declared.game_abi.ok_or(PackageError::NoGameLayout)?);
                manifest.target = Some(declared.target.ok_or(PackageError::NoTarget)?);
                manifest.api_version =
                    Some(declared.api_version.ok_or(PackageError::NoApiVersion)?);
                manifest.state = (declared.state > 0).then_some(declared.state);
                manifest.hooks = declared.hooks;
                manifest.events = declared.events;
                manifest.exports = declared.exports;
                manifest.imports = declared.imports;
            }
            None if files.is_empty() && assets.is_empty() => return Err(PackageError::Empty),
            None => {
                if manifest.entry.is_some() {
                    return Err(PackageError::MissingFile(
                        manifest.library_name().to_owned(),
                    ));
                }
                manifest.game_abi = None;
                manifest.target = None;
                manifest.api_version = None;
                manifest.state = None;
                manifest.hooks = Default::default();
                manifest.events = Vec::new();
                manifest.exports = Vec::new();
                manifest.imports = Vec::new();
            }
        }
        Ok(Self {
            manifest,
            library,
            files,
            assets,
            include,
        })
    }

    /// Open a package zip, checking its manifest against its library and
    /// folders.
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
                if declared.api_version.is_none() {
                    return Err(PackageError::NoApiVersion);
                }
                if declared.game_abi != manifest.game_abi
                    || declared.target != manifest.target
                    || declared.api_version != manifest.api_version
                    || (declared.state > 0).then_some(declared.state) != manifest.state
                    || declared.hooks != manifest.hooks
                    || declared.events != manifest.events
                    || declared.exports != manifest.exports
                    || declared.imports != manifest.imports
                {
                    return Err(PackageError::Mismatch);
                }
            }
            None => {
                if manifest.game_abi.is_some()
                    || manifest.target.is_some()
                    || manifest.api_version.is_some()
                    || manifest.state.is_some()
                    || !manifest.hooks.is_empty()
                    || !manifest.events.is_empty()
                    || !manifest.exports.is_empty()
                    || !manifest.imports.is_empty()
                {
                    return Err(PackageError::FilesOnlyDeclares);
                }
            }
        }

        let files = read_listed(&mut archive, "files", &manifest.files)?;
        let assets = read_listed(&mut archive, "assets", &manifest.assets)?;
        let mut include = Files::new();
        let names: Vec<String> = archive
            .file_names()
            .filter(|name| name.starts_with("include/") && !name.ends_with('/'))
            .map(str::to_owned)
            .collect();
        for name in names {
            let bytes = read_entry(&mut archive, &name, INCLUDE_LIMIT)?;
            include.insert(name["include/".len()..].to_owned(), bytes);
        }
        files::check("include", &include)?;
        if library.is_none() && files.is_empty() && assets.is_empty() {
            return Err(PackageError::Empty);
        }
        Ok(Self {
            manifest,
            library,
            files,
            assets,
            include,
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
                    .chain(self.assets.values())
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
        for (folder, files) in [
            ("files", &self.files),
            ("assets", &self.assets),
            ("include", &self.include),
        ] {
            for (path, bytes) in files {
                write(&format!("{folder}/{path}"), bytes);
            }
        }
        writer.finish().expect("write to memory").into_inner()
    }
}

/// The files the manifest lists under `folder`, each checked against its
/// size and hash, refusing any the manifest leaves out.
fn read_listed(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    folder: &'static str,
    listed: &[files::ModFile],
) -> Result<Files, PackageError> {
    let mut files = Files::new();
    for file in listed {
        files::check_path(folder, &file.path)?;
        let name = format!("{folder}/{}", file.path);
        let bytes = read_entry(archive, &name, files::FILE_LIMIT - 1)?;
        if bytes.len() as u64 != file.size || sha256_hex(&bytes) != file.sha256 {
            return Err(PackageError::FilesMismatch(folder));
        }
        files.insert(file.path.clone(), bytes);
    }
    let prefix = format!("{folder}/");
    let shipped = archive
        .file_names()
        .filter(|name| name.starts_with(&prefix) && !name.ends_with('/'))
        .count();
    if shipped != files.len() || files::list(&files) != listed {
        return Err(PackageError::FilesMismatch(folder));
    }
    files::check(folder, &files)?;
    Ok(files)
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
        BEFORE, EVENT, EXPORT, IMPORT, REPLACE, SYMBOL, built_with_sdk, library, record,
        state_record,
    };

    fn manifest() -> Manifest {
        Manifest::new(
            "test.mod".parse().expect("id"),
            "Test".into(),
            semver::Version::new(1, 0, 0),
        )
    }

    #[test]
    fn hooks_come_from_the_library_and_a_manifest_claiming_others_is_refused() {
        let mut records = built_with_sdk();
        records.extend([
            state_record("counter", 4),
            state_record("table", 60),
            record(REPLACE, "ftCo_Landing_IASA"),
            record(BEFORE, "ftCo_Jump.c:ftCo_Jump_Anim"),
            record(EVENT, "match_start"),
            record(EXPORT, "register_clone@1"),
            record(IMPORT, "ref.core/clone_count@1"),
            record(SYMBOL, "ftCo_Damage.c:ftCo_803C1A20"),
        ]);
        let package = Package::pack(
            manifest(),
            Some(library(&records)),
            Files::new(),
            Files::new(),
            Files::new(),
        )
        .expect("pack");
        assert_eq!(package.manifest.hooks.replaces, ["ftCo_Landing_IASA"]);
        assert_eq!(package.manifest.events, ["match_start"]);
        assert_eq!(package.manifest.api_version.as_deref(), Some("0.1"));
        assert_eq!(package.manifest.state, Some(64));
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
    fn files_and_assets_travel_in_the_package_and_a_mod_may_ship_only_files() {
        let files = Files::from([
            ("PlMrNr.dat".to_owned(), b"mario".to_vec()),
            ("Sd/Custom.dat".to_owned(), b"stage".to_vec()),
        ]);
        let assets = Files::from([("icon.png".to_owned(), b"png".to_vec())]);
        let include = Files::from([("me/api.h".to_owned(), b"#pragma once".to_vec())]);
        let package = Package::pack(
            manifest(),
            Some(library(&built_with_sdk())),
            files.clone(),
            assets.clone(),
            include,
        )
        .expect("pack");
        assert_eq!(package.manifest.files.len(), 2);
        assert_eq!(package.manifest.assets[0].path, "icon.png");
        assert_eq!(Package::from_zip(&package.to_zip()).expect("open"), package);

        let only = Package::pack(manifest(), None, files, assets, Files::new()).expect("pack");
        assert_eq!(only.manifest.entry, None);
        assert_eq!(only.manifest.game_abi, None);
        assert_eq!(Package::from_zip(&only.to_zip()).expect("open"), only);
        assert!(matches!(
            Package::pack(manifest(), None, Files::new(), Files::new(), Files::new()),
            Err(PackageError::Empty)
        ));

        // An asset that differs from the manifest's list must not open.
        let mut swapped = only.clone();
        swapped.assets.insert("icon.png".into(), b"gif".to_vec());
        assert!(matches!(
            Package::from_zip(&swapped.to_zip()),
            Err(PackageError::FilesMismatch("assets"))
        ));
    }
}
