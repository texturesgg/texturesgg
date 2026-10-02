//! A disc image left on disk and read a file at a time, for callers that
//! need a few files from a 1.4 GB image without loading all of it.

use crate::fst::find_file;
use crate::{Error, FstEntry, Result, io, read_disk_fst};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The boot header fields that identify a disc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscHeader {
    /// The six-character game ID, such as "GALE01" for Melee NTSC.
    pub game_id: String,
    /// The disc revision, such as 2 for Melee NTSC 1.02.
    pub revision: u8,
    pub title: String,
}

/// An open disc image: its header and file table, with file contents read on
/// demand.
pub struct Disc {
    file: File,
    header: DiscHeader,
    entries: Vec<FstEntry>,
}

impl Disc {
    /// Open the image at `path`, reading only its header and file table.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path)?;
        // The title field runs from 0x20 to the end of the 0x400 byte header.
        let mut boot = [0_u8; 0x400];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut boot)?;
        let header = DiscHeader {
            game_id: String::from_utf8_lossy(&boot[0..6]).into_owned(),
            revision: boot[7],
            title: io::read_cstring(&boot, 0x20),
        };
        let entries = read_disk_fst(&mut file)?.entries;
        Ok(Self {
            file,
            header,
            entries,
        })
    }

    pub fn header(&self) -> &DiscHeader {
        &self.header
    }

    /// Every entry in the file table, directories included.
    pub fn files(&self) -> &[FstEntry] {
        &self.entries
    }

    /// Read the file called `name`: a path from the root
    /// (`audio/us/main.ssm`), or a bare name when only one file has it.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let entry = find_file(&self.entries, name)?;
        let (offset, size) = (u64::from(entry.offset), entry.size);
        let end = offset
            .checked_add(u64::from(size))
            .ok_or_else(|| Error::InvalidIso(format!("File {name} range overflow")))?;
        if end > self.file.metadata()?.len() {
            return Err(Error::InvalidIso(format!(
                "File {name} extends past end of ISO"
            )));
        }
        self.file.seek(SeekFrom::Start(offset))?;
        let mut data = vec![0_u8; size as usize];
        self.file.read_exact(&mut data)?;
        Ok(data)
    }
}
