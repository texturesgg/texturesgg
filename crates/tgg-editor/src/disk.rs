//! Reading and writing the player's files: never more of a file than the
//! app can use, and never a file left half-written.

use std::io::{Read, Write};
use std::path::Path;

/// Why a file wasn't read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    #[error("it's {} MB, more than the {} MB the app reads", size / 1_000_000, limit / 1_000_000)]
    TooLarge { size: u64, limit: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Read the file at `path` when it is at most `limit` bytes. The size is
/// checked before anything is read, so a disc image dropped where a skin
/// belongs is not loaded into memory first.
pub(crate) fn read_capped(path: &Path, limit: u64) -> Result<Vec<u8>, ReadError> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size > limit {
        return Err(ReadError::TooLarge { size, limit });
    }
    // A file can grow, or report no size at all, so the read is bounded too.
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(ReadError::TooLarge {
            size: bytes.len() as u64,
            limit,
        });
    }
    Ok(bytes)
}

/// Write `bytes` to `path` without ever leaving it half-written: write a
/// temporary file beside it, flush it to disk, then rename it over the
/// target. A symlinked target is written through to the file it names.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temporary = directory.join(format!(".{name}.{}.tgg-save", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        // Keep the original's permissions.
        if let Ok(metadata) = std::fs::metadata(&target) {
            file.set_permissions(metadata.permissions())?;
        }
        file.sync_all()?;
        std::fs::rename(&temporary, &target)
    })();
    if written.is_err() {
        std::fs::remove_file(&temporary).ok();
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_the_file_and_leaves_nothing_behind() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("PlFcRe.dat");
        std::fs::write(&path, b"old").unwrap();
        write_atomically(&path, b"new bytes").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new bytes");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_file_over_the_limit_is_refused_and_one_at_it_is_read() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("skin.dat");
        std::fs::write(&path, [7; 16]).unwrap();
        assert_eq!(read_capped(&path, 16).unwrap(), [7; 16]);
        assert!(matches!(
            read_capped(&path, 15),
            Err(ReadError::TooLarge {
                size: 16,
                limit: 15
            })
        ));
    }
}
