//! The files a mod ships beside its library: `files/`, which mirrors the game
//! disc, `assets/`, new files the game serves at `/mods/<id>/`, and
//! `include/`, headers for mods that build on this one.
//!
//! `files/PlMrNr.dat` takes the place of the disc's `/PlMrNr.dat`; a path the
//! disc lacks adds a file. Paths are written without the leading slash, keep
//! the case they were written in, and compare without case, as the disc's
//! own lookup does. When two mods ship the same path, the one that loads
//! later wins.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// A path must be shorter than this, in bytes.
pub const PATH_LIMIT: usize = 256;
/// Each file must be smaller than this.
pub const FILE_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// A file the package ships, as its manifest lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModFile {
    /// The disc path, without the leading slash: `PlMrNr.dat`, `Sd/Custom.dat`.
    pub path: String,
    pub size: u64,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
}

/// A mod's files by disc path, in byte order of path.
pub type Files = BTreeMap<String, Vec<u8>>;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("{0} is not a disc path: use '/' between non-empty names, none starting with '.'")]
    Path(String),
    #[error("{0} has a path of {PATH_LIMIT} bytes or more")]
    PathTooLong(String),
    #[error("{0} is 4 GiB or larger")]
    TooLarge(String),
    #[error("{0} and {1} are the same path; disc paths compare without case")]
    SameCase(String, String),
    #[error("{0}: a name that isn't UTF-8")]
    Name(String),
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
}

/// Check one disc path of the folder `folder` (`files` or `assets`).
pub fn check_path(folder: &str, path: &str) -> Result<(), FileError> {
    if path.len() >= PATH_LIMIT {
        return Err(FileError::PathTooLong(format!("{folder}/{path}")));
    }
    let valid = !path.is_empty()
        && !path.contains(['\\', '\0'])
        && path
            .split('/')
            .all(|name| !name.is_empty() && !name.starts_with('.'));
    if valid {
        Ok(())
    } else {
        Err(FileError::Path(format!("{folder}/{path}")))
    }
}

/// The key two paths share when they name the same disc file.
pub fn path_key(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// Check every path and size of the folder `folder`, and that no two paths
/// differ only in case.
pub fn check(folder: &str, files: &Files) -> Result<(), FileError> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    for (path, bytes) in files {
        check_path(folder, path)?;
        if bytes.len() as u64 >= FILE_LIMIT {
            return Err(FileError::TooLarge(format!("{folder}/{path}")));
        }
        if let Some(other) = seen.insert(path_key(path), path) {
            return Err(FileError::SameCase(
                format!("{folder}/{other}"),
                format!("{folder}/{path}"),
            ));
        }
    }
    Ok(())
}

/// The manifest's list of `files`, in byte order of path.
pub fn list(files: &Files) -> Vec<ModFile> {
    files
        .iter()
        .map(|(path, bytes)| ModFile {
            path: path.clone(),
            size: bytes.len() as u64,
            sha256: crate::package::sha256_hex(bytes),
        })
        .collect()
}

/// Read one of a mod's folders under `dir` (`files`, `assets` or `include`):
/// every regular file under it, skipping any name that starts with '.'. A
/// missing folder has no files.
pub fn read_dir(dir: &Path, folder: &str) -> Result<Files, FileError> {
    let root = dir.join(folder);
    let mut files = Files::new();
    let mut pending = vec![String::new()];
    while let Some(relative) = pending.pop() {
        let dir = root.join(&relative);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && relative.is_empty() => {
                return Ok(files);
            }
            Err(e) => return Err(FileError::Io(dir.display().to_string(), e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| FileError::Io(dir.display().to_string(), e))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| FileError::Name(entry.path().display().to_string()))?;
            if name.starts_with('.') {
                continue;
            }
            let path = if relative.is_empty() {
                name.to_owned()
            } else {
                format!("{relative}/{name}")
            };
            let kind = entry
                .file_type()
                .map_err(|e| FileError::Io(entry.path().display().to_string(), e))?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                let bytes = std::fs::read(entry.path())
                    .map_err(|e| FileError::Io(entry.path().display().to_string(), e))?;
                files.insert(path, bytes);
            }
        }
    }
    check(folder, &files)?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_paths() {
        for good in ["PlMrNr.dat", "Sd/Custom.dat", "a/b/c"] {
            assert!(check_path("files", good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "/PlMrNr.dat",
            "Sd//x",
            "../x",
            "Sd/.hidden",
            "a\\b",
            "x/",
        ] {
            assert!(check_path("files", bad).is_err(), "{bad}");
        }
        assert!(check_path("files", &"a".repeat(PATH_LIMIT)).is_err());
    }

    #[test]
    fn paths_differing_only_in_case_are_one_path() {
        let files = Files::from([
            ("PlMrNr.dat".to_owned(), vec![1]),
            ("plmrnr.dat".to_owned(), vec![2]),
        ]);
        assert!(matches!(
            check("files", &files),
            Err(FileError::SameCase(..))
        ));
    }

    #[test]
    fn reading_skips_dot_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("files");
        std::fs::create_dir_all(root.join("Sd")).expect("mkdir");
        std::fs::create_dir_all(root.join(".git")).expect("mkdir");
        std::fs::write(root.join("PlMrNr.dat"), b"mario").expect("write");
        std::fs::write(root.join("Sd/Custom.dat"), b"stage").expect("write");
        std::fs::write(root.join(".DS_Store"), b"junk").expect("write");
        std::fs::write(root.join(".git/HEAD"), b"junk").expect("write");
        let files = read_dir(dir.path(), "files").expect("read");
        assert_eq!(
            files.keys().collect::<Vec<_>>(),
            ["PlMrNr.dat", "Sd/Custom.dat"]
        );
        assert!(read_dir(dir.path(), "missing").expect("read").is_empty());
    }
}
