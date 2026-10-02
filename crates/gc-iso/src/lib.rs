//! Read GameCube (GCM) disc images a file at a time, and replace one file in
//! place.
//!
//! GCM ISO layout:
//!   0x0000: Boot header (0x440 bytes) — game ID, title, DOL/FST offsets
//!   0x0420: DOL offset (u32 BE)
//!   0x0424: FST offset (u32 BE)
//!   0x0428: FST size (u32 BE)
//!
//! FST entry format (12 bytes each):
//!   0x00: flags (u8) — 0=file, 1=directory
//!   0x01: name_offset (u24 BE) — offset into string table
//!   0x04: file_offset (u32 BE) or parent_dir_index (for directories)
//!   0x08: file_size (u32 BE) or next_entry_index (for directories)

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

mod disc;
mod fst;
mod io;

pub use disc::{Disc, DiscHeader};
pub use fst::FstEntry;

const MAX_DISK_FST_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid ISO: {0}")]
    InvalidIso(String),

    #[error("File not found in ISO: {0}")]
    FileNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Replace the file called `name` in the image at `path`, without loading or
/// rewriting the rest of it.
///
/// A replacement that fits goes into the old slot, zero-padded. A larger one
/// is appended at the end of the image, aligned to 32 bytes, and the file
/// table entry is pointed at it. A [`Disc`] opened before the call holds the
/// old table; open the image again to read the new file.
pub fn replace_file(path: impl AsRef<Path>, name: &str, new_data: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let disk_fst = read_disk_fst(&mut file)?;
    let entry = disk_fst
        .entries
        .iter()
        .find(|entry| !entry.is_dir && entry.name == name)
        .ok_or_else(|| Error::FileNotFound(name.into()))?;
    let length = file.metadata()?.len();
    let existing_end = u64::from(entry.offset) + u64::from(entry.size);
    if existing_end > length {
        return Err(Error::InvalidIso(format!(
            "File {name} extends past end of ISO"
        )));
    }
    let new_size = u32::try_from(new_data.len())
        .map_err(|_| Error::InvalidIso("Replacement exceeds u32 file size".into()))?;

    let (new_offset, should_zero_pad) = if new_size <= entry.size {
        (entry.offset, true)
    } else {
        let aligned = length
            .checked_add(31)
            .map(|length| length & !31)
            .ok_or_else(|| Error::InvalidIso("ISO alignment overflow".into()))?;
        let new_offset = u32::try_from(aligned)
            .map_err(|_| Error::InvalidIso("Appended file offset exceeds u32".into()))?;
        if aligned > length {
            file.set_len(aligned)?;
        }
        (new_offset, false)
    };

    file.seek(SeekFrom::Start(new_offset as u64))?;
    file.write_all(new_data)?;

    if should_zero_pad {
        write_zeros(&mut file, (entry.size - new_size) as usize)?;
    } else {
        let end = u64::from(new_offset) + u64::from(new_size);
        let padded_end = end
            .checked_add(31)
            .map(|length| length & !31)
            .ok_or_else(|| Error::InvalidIso("ISO tail alignment overflow".into()))?;
        if padded_end > end {
            file.set_len(padded_end)?;
        }
    }

    let entry_position = disk_fst.offset + entry.fst_index * 12;
    file.seek(SeekFrom::Start((entry_position + 4) as u64))?;
    file.write_all(&new_offset.to_be_bytes())?;
    file.write_all(&new_size.to_be_bytes())?;
    file.flush()?;

    debug_assert!(entry_position + 12 <= disk_fst.end);
    Ok(())
}

struct DiskFst {
    offset: usize,
    end: usize,
    entries: Vec<FstEntry>,
}

fn read_disk_fst(file: &mut (impl Read + Seek)) -> Result<DiskFst> {
    let mut header = [0_u8; 0x430];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut header)?;

    let fst_offset = io::read_u32_be(&header, 0x0424) as usize;
    let fst_size = io::read_u32_be(&header, 0x0428) as usize;
    let fst_end = fst_offset
        .checked_add(fst_size)
        .ok_or_else(|| Error::InvalidIso("FST range overflow".into()))?;
    if fst_size > MAX_DISK_FST_BYTES {
        return Err(Error::InvalidIso(format!(
            "FST exceeds {MAX_DISK_FST_BYTES} byte limit"
        )));
    }
    let file_len = file.seek(SeekFrom::End(0))?;
    if fst_end as u64 > file_len {
        return Err(Error::InvalidIso("FST extends past end of ISO".into()));
    }

    file.seek(SeekFrom::Start(fst_offset as u64))?;
    let mut fst_data = vec![0_u8; fst_size];
    file.read_exact(&mut fst_data)?;
    if fst_data.len() < 12 {
        return Err(Error::InvalidIso("FST is too small".into()));
    }

    let total_entries = io::read_u32_be(&fst_data, 8) as usize;
    let entries_size = total_entries
        .checked_mul(12)
        .ok_or_else(|| Error::InvalidIso("FST entry count overflow".into()))?;
    if entries_size > fst_data.len() {
        return Err(Error::InvalidIso("FST entries exceed FST size".into()));
    }

    Ok(DiskFst {
        offset: fst_offset,
        end: fst_end,
        entries: fst::parse_fst(&fst_data, 0, total_entries, entries_size)?,
    })
}

fn write_zeros(file: &mut impl Write, mut length: usize) -> std::io::Result<()> {
    const ZEROS: [u8; 8192] = [0; 8192];
    while length > 0 {
        let chunk = length.min(ZEROS.len());
        file.write_all(&ZEROS[..chunk])?;
        length -= chunk;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Disc, replace_file};
    use std::fs::{self, OpenOptions};

    const FST_OFFSET: usize = 0x500;
    const FILE_OFFSET: usize = 0x800;
    const FILE_NAME: &str = "test.dat";

    fn test_iso(file_data: &[u8]) -> Vec<u8> {
        let mut data = vec![0; FILE_OFFSET + file_data.len()];
        data[0..6].copy_from_slice(b"GALE01");
        data[0x424..0x428].copy_from_slice(&(FST_OFFSET as u32).to_be_bytes());
        let fst_size = 24 + FILE_NAME.len() + 1;
        data[0x428..0x42c].copy_from_slice(&(fst_size as u32).to_be_bytes());

        // Root directory entry: parent index 0, two total entries.
        data[FST_OFFSET] = 1;
        data[FST_OFFSET + 8..FST_OFFSET + 12].copy_from_slice(&2_u32.to_be_bytes());

        // File entry: first string-table name, fixed data offset and size.
        let file_entry = FST_OFFSET + 12;
        data[file_entry + 4..file_entry + 8].copy_from_slice(&(FILE_OFFSET as u32).to_be_bytes());
        data[file_entry + 8..file_entry + 12]
            .copy_from_slice(&(file_data.len() as u32).to_be_bytes());

        let string_table = FST_OFFSET + 24;
        data[string_table..string_table + FILE_NAME.len()].copy_from_slice(FILE_NAME.as_bytes());
        data[FILE_OFFSET..].copy_from_slice(file_data);

        data
    }

    fn read(path: &std::path::Path) -> crate::Result<Vec<u8>> {
        Disc::open(path)?.read(FILE_NAME)
    }

    #[test]
    fn a_disc_on_disk_reads_its_header_and_files_without_loading_the_image() {
        let mut bytes = test_iso(b"costume");
        bytes[7] = 2;
        bytes[0x20..0x2a].copy_from_slice(b"Test title");
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("disc.iso");
        fs::write(&path, bytes).expect("write ISO");

        let mut disc = Disc::open(&path).expect("open disc");
        assert_eq!(disc.header().game_id, "GALE01");
        assert_eq!(disc.header().revision, 2);
        assert_eq!(disc.header().title, "Test title");
        assert_eq!(disc.read(FILE_NAME).expect("read file"), b"costume");
        assert!(disc.read("missing.dat").is_err());
    }

    #[test]
    fn malformed_fst_ranges_and_name_offsets_return_errors() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("malformed.iso");
        let mut bytes = test_iso(b"old");
        bytes[0x428..0x42c].copy_from_slice(&u32::MAX.to_be_bytes());
        fs::write(&path, bytes).expect("write malformed ISO");
        assert!(read(&path).is_err());

        let mut bytes = test_iso(b"old");
        let file_entry = FST_OFFSET + 12;
        bytes[file_entry + 1..file_entry + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
        fs::write(&path, bytes).expect("write malformed ISO");
        assert!(read(&path).is_err());
    }

    #[test]
    fn a_refused_replacement_leaves_the_image_its_length() {
        let directory = tempfile::tempdir().expect("temp directory");
        let malformed_path = directory.path().join("malformed-file.iso");
        let mut malformed = test_iso(b"old");
        let file_entry = FST_OFFSET + 12;
        malformed[file_entry + 4..file_entry + 8].copy_from_slice(&u32::MAX.to_be_bytes());
        fs::write(&malformed_path, &malformed).expect("write malformed ISO");
        let original_len = malformed.len() as u64;
        assert!(replace_file(&malformed_path, FILE_NAME, b"x").is_err());
        assert_eq!(fs::metadata(&malformed_path).unwrap().len(), original_len);

        let sparse_path = directory.path().join("oversized-sparse.iso");
        fs::write(&sparse_path, test_iso(b"old")).expect("write sparse ISO base");
        let oversized_len = u64::from(u32::MAX) + 1;
        OpenOptions::new()
            .write(true)
            .open(&sparse_path)
            .unwrap()
            .set_len(oversized_len)
            .expect("extend sparse ISO");
        assert!(replace_file(&sparse_path, FILE_NAME, b"larger replacement").is_err());
        assert_eq!(fs::metadata(&sparse_path).unwrap().len(), oversized_len);
    }

    #[test]
    fn a_larger_replacement_is_appended_and_read_back() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("test.iso");
        fs::write(&path, test_iso(b"old")).expect("write test ISO");
        let replacement = b"a replacement larger than the original";

        replace_file(&path, FILE_NAME, replacement).expect("replace file on disk");

        assert_eq!(read(&path).expect("read the replacement"), replacement);
        assert_eq!(fs::metadata(path).expect("ISO metadata").len() % 32, 0);
    }

    #[test]
    fn a_smaller_replacement_stays_in_place_and_zero_pads_the_slot() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("test.iso");
        fs::write(&path, test_iso(b"abcdefgh")).expect("write test ISO");

        replace_file(&path, FILE_NAME, b"xyz").expect("replace file on disk");
        assert_eq!(read(&path).expect("read the replacement"), b"xyz");
        let updated = fs::read(&path).expect("read the image");
        assert_eq!(&updated[FILE_OFFSET + 3..FILE_OFFSET + 8], &[0; 5]);
    }
}
