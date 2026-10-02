//! Read GameCube (GCM) disc images a file at a time, and replace one file in
//! place.
//!
//! GCM ISO layout:
//!   0x0000: Boot header (0x440 bytes) — game ID, title, DOL/FST offsets
//!   0x001C: GameCube magic word (0xC2339F3D)
//!   0x0420: DOL offset (u32 BE)
//!   0x0424: FST offset (u32 BE)
//!   0x0428: FST size (u32 BE)
//!
//! FST entry format (12 bytes each):
//!   0x00: flags (u8) — 0=file, otherwise directory
//!   0x01: name_offset (u24 BE) — offset into string table
//!   0x04: file_offset (u32 BE) or parent_dir_index (for directories)
//!   0x08: file_size (u32 BE) or next_entry_index (for directories)

use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;

mod disc;
mod fst;
mod io;

pub use disc::{Disc, DiscHeader};
pub use fst::FstEntry;

const MAX_DISK_FST_BYTES: usize = 64 * 1024 * 1024;
const GAMECUBE_MAGIC: u32 = 0xC233_9F3D;
/// The boot header and the disc information after it, up to the apploader.
const HEADER_BYTES: u64 = 0x440;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid ISO: {0}")]
    InvalidIso(String),

    #[error("File not found in ISO: {0}")]
    FileNotFound(String),

    #[error("More than one file in the ISO is called {0}; name it by its path")]
    AmbiguousName(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Replace the file called `name` (as [`Disc::read`] takes it) in the image
/// at `path`, without loading or rewriting the rest of it.
///
/// A replacement that fits goes into the old slot, zero-padded. A larger one
/// is appended at the end of the image, aligned to 32 bytes, and the file
/// table entry is pointed at it. A [`Disc`] opened before the call holds the
/// old table; open the image again to read the new file.
///
/// A slot that shares bytes with the disc header, the file table or another
/// file is refused before anything is written. The write itself is not
/// atomic: when it fails part-way, a slot written in place may hold part of
/// the replacement, and an appended file is left unreferenced at the end of
/// the image.
pub fn replace_file(path: impl AsRef<Path>, name: &str, new_data: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let disk_fst = read_disk_fst(&mut file)?;
    let entry = fst::find_file(&disk_fst.entries, name)?;
    let length = file.metadata()?.len();
    let slot = extent(entry);
    if slot.end > length {
        return Err(Error::InvalidIso(format!(
            "File {name} extends past end of ISO"
        )));
    }
    let new_size = u32::try_from(new_data.len())
        .map_err(|_| Error::InvalidIso("Replacement exceeds u32 file size".into()))?;
    if new_size <= entry.size {
        // The whole slot is about to be written, so nothing else may live in it.
        let shares = |other: &Range<u64>| slot.start < other.end && other.start < slot.end;
        let fst = disk_fst.offset as u64..disk_fst.end as u64;
        if shares(&(0..HEADER_BYTES))
            || shares(&fst)
            || disk_fst.entries.iter().any(|other| {
                !other.is_dir && other.fst_index != entry.fst_index && shares(&extent(other))
            })
        {
            return Err(Error::InvalidIso(format!(
                "File {name} shares its bytes with the header, the file table or another file"
            )));
        }
    }

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
    Ok(())
}

/// The bytes of the image a file entry names.
fn extent(entry: &FstEntry) -> Range<u64> {
    u64::from(entry.offset)..u64::from(entry.offset) + u64::from(entry.size)
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
    if io::read_u32_be(&header, 0x1C) != GAMECUBE_MAGIC {
        return Err(Error::InvalidIso("not a GameCube disc image".into()));
    }

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
        data[0x1C..0x20].copy_from_slice(&super::GAMECUBE_MAGIC.to_be_bytes());
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

    /// `test_iso` with its one file entry pointed at `offset` for `size` bytes.
    fn iso_with_slot(offset: usize, size: usize) -> Vec<u8> {
        let mut bytes = test_iso(&[0xAA; 0x400]);
        let file_entry = FST_OFFSET + 12;
        bytes[file_entry + 4..file_entry + 8].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[file_entry + 8..file_entry + 12].copy_from_slice(&(size as u32).to_be_bytes());
        bytes
    }

    #[test]
    fn a_slot_over_the_header_or_the_file_table_is_refused_untouched() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("overlap.iso");
        for (offset, size) in [(0, 0x800), (0x43F, 0x10), (FST_OFFSET, 0x20), (0x4F0, 0x11)] {
            let bytes = iso_with_slot(offset, size);
            fs::write(&path, &bytes).expect("write ISO");
            assert!(replace_file(&path, FILE_NAME, b"x").is_err());
            assert_eq!(fs::read(&path).expect("read the image"), bytes);
        }
    }

    /// An image with two directories, `a` and `b`, each holding a
    /// `same.dat`, with `only.dat` beside them at the root.
    fn iso_with_directories() -> Vec<u8> {
        let names = b"a\0same.dat\0b\0only.dat\0";
        // (flags, name offset, offset or parent, size or end index)
        let entries: [(u8, u32, u32, u32); 6] = [
            (1, 0, 0, 6),
            (1, 0, 0, 3),
            (0, 2, 0x800, 4),
            (1, 11, 0, 5),
            (0, 2, 0x820, 4),
            (0, 13, 0x840, 4),
        ];
        let mut data = vec![0; 0x860];
        data[0x1C..0x20].copy_from_slice(&super::GAMECUBE_MAGIC.to_be_bytes());
        data[0x424..0x428].copy_from_slice(&(FST_OFFSET as u32).to_be_bytes());
        let fst_size = entries.len() * 12 + names.len();
        data[0x428..0x42c].copy_from_slice(&(fst_size as u32).to_be_bytes());
        for (index, (flags, name, offset, size)) in entries.into_iter().enumerate() {
            let entry = FST_OFFSET + index * 12;
            data[entry..entry + 4].copy_from_slice(&name.to_be_bytes());
            data[entry] = flags;
            data[entry + 4..entry + 8].copy_from_slice(&offset.to_be_bytes());
            data[entry + 8..entry + 12].copy_from_slice(&size.to_be_bytes());
        }
        let string_table = FST_OFFSET + entries.len() * 12;
        data[string_table..string_table + names.len()].copy_from_slice(names);
        data[0x800..0x804].copy_from_slice(b"in a");
        data[0x820..0x824].copy_from_slice(b"in b");
        data[0x840..0x844].copy_from_slice(b"root");
        data
    }

    #[test]
    fn a_name_two_files_share_is_read_by_path() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("directories.iso");
        fs::write(&path, iso_with_directories()).expect("write ISO");

        let mut disc = Disc::open(&path).expect("open disc");
        assert_eq!(disc.read("only.dat").expect("a unique name"), b"root");
        assert_eq!(disc.read("a/same.dat").expect("a path"), b"in a");
        assert_eq!(disc.read("b/same.dat").expect("a path"), b"in b");
        assert!(matches!(
            disc.read("same.dat"),
            Err(crate::Error::AmbiguousName(_))
        ));

        replace_file(&path, "b/same.dat", b"new").expect("replace by path");
        let mut disc = Disc::open(&path).expect("open disc");
        assert_eq!(disc.read("a/same.dat").expect("the other file"), b"in a");
        assert_eq!(disc.read("b/same.dat").expect("the replaced file"), b"new");
    }

    #[test]
    fn names_that_add_up_past_the_budget_are_refused() {
        // Every entry points at the same megabyte of bytes with no terminator.
        const ENTRIES: usize = 40;
        let name = vec![b'n'; 1024 * 1024];
        let fst_size = ENTRIES * 12 + name.len();
        let mut data = vec![0; FST_OFFSET + fst_size];
        data[0x1C..0x20].copy_from_slice(&super::GAMECUBE_MAGIC.to_be_bytes());
        data[0x424..0x428].copy_from_slice(&(FST_OFFSET as u32).to_be_bytes());
        data[0x428..0x42c].copy_from_slice(&(fst_size as u32).to_be_bytes());
        data[FST_OFFSET] = 1;
        data[FST_OFFSET + 8..FST_OFFSET + 12].copy_from_slice(&(ENTRIES as u32).to_be_bytes());
        data[FST_OFFSET + ENTRIES * 12..].copy_from_slice(&name);
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("names.iso");
        fs::write(&path, data).expect("write ISO");

        assert!(Disc::open(&path).is_err());
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
