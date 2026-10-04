//! Package zips: `manifest.json` and the library it names.
//!
//! [`Package::pack`] fills the manifest's hooks, exports, imports and game
//! layout from the library. [`Package::from_zip`] reads them from the
//! library again and refuses a package whose manifest disagrees, so a catalog or installer
//! can trust a package's manifest once it opens.

use crate::decls::{self, DeclError};
use crate::manifest::{Manifest, ManifestError};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

/// The largest library a package may hold.
pub const LIBRARY_LIMIT: u64 = 64 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub manifest: Manifest,
    pub library: Vec<u8>,
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
    #[error("{0} in the package is too large")]
    TooLarge(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Package {
    /// A package of `library` described by `manifest`, with the manifest's
    /// hooks and game layout taken from the library.
    pub fn pack(library: Vec<u8>, mut manifest: Manifest) -> Result<Self, PackageError> {
        manifest.validate()?;
        let declared = decls::read(&library)?;
        manifest.game_abi = Some(declared.game_abi.ok_or(PackageError::NoGameLayout)?);
        manifest.target = Some(declared.target.ok_or(PackageError::NoTarget)?);
        manifest.hooks = declared.hooks;
        manifest.exports = declared.exports;
        manifest.imports = declared.imports;
        Ok(Self { manifest, library })
    }

    /// Open a package zip, checking its manifest against its library.
    pub fn from_zip(bytes: &[u8]) -> Result<Self, PackageError> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let manifest =
            Manifest::parse(&read_entry(&mut archive, "manifest.json", MANIFEST_LIMIT)?)?;
        let library = read_entry(&mut archive, &manifest.entry, LIBRARY_LIMIT)?;
        let declared = decls::read(&library)?;
        if declared.game_abi.is_none() {
            return Err(PackageError::NoGameLayout);
        }
        if declared.target.is_none() {
            return Err(PackageError::NoTarget);
        }
        if declared.game_abi != manifest.game_abi
            || declared.target != manifest.target
            || declared.hooks != manifest.hooks
            || declared.exports != manifest.exports
            || declared.imports != manifest.imports
        {
            return Err(PackageError::Mismatch);
        }
        Ok(Self { manifest, library })
    }

    /// The package as a zip. The same package always gives the same bytes
    /// from the same build of this crate; the deflate encoder is part of that
    /// build, so a registry packs with one pinned build.
    pub fn to_zip(&self) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default())
            .unix_permissions(0o644);
        let files = [
            ("manifest.json", self.manifest.to_json().into_bytes()),
            (self.manifest.entry.as_str(), self.library.clone()),
        ];
        for (name, bytes) in files {
            writer.start_file(name, options).expect("write to memory");
            writer.write_all(&bytes).expect("write to memory");
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
            entry: "mod.so".into(),
            netplay: Netplay::Gameplay,
            description: None,
            license: None,
            game_abi: None,
            target: None,
            hooks: Default::default(),
            exports: Vec::new(),
            imports: Vec::new(),
        }
    }

    #[test]
    fn hooks_come_from_the_library_and_a_manifest_claiming_others_is_refused() {
        let lib = library(&[
            record(GAME_ABI, "e954031487f52421"),
            record(TARGET, "x86_64-linux-gnu"),
            record(REPLACE, "ftCo_Landing_IASA"),
            record(BEFORE, "ftCo_Jump.c:ftCo_Jump_Anim"),
            record(EXPORT, "register_clone"),
            record(IMPORT, "ref.core/clone_count"),
            record(SYMBOL, "ftCo_Damage.c:ftCo_803C1A20"),
        ]);
        let package = Package::pack(lib, manifest()).expect("pack");
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
}
