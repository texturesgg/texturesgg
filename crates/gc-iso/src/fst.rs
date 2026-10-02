use crate::{Error, Result, io};

/// The most bytes the names and paths of one file table may add up to.
const MAX_NAME_BYTES: usize = 16 * 1024 * 1024;

/// An entry in the GCM file system table.
#[derive(Debug, Clone)]
pub struct FstEntry {
    /// File or directory name. The root directory's is empty.
    pub name: String,
    /// The name with the directories above it, `/`-separated and without a
    /// leading slash: `audio/us/main.ssm`.
    pub path: String,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// File offset within the ISO (for files).
    pub offset: u32,
    /// File size in bytes (for files), or next entry index (for directories).
    pub size: u32,
    /// Index of this entry in the FST (needed to write back changes).
    pub fst_index: usize,
}

/// Parse the FST from raw ISO data.
pub(crate) fn parse_fst(
    data: &[u8],
    fst_offset: usize,
    total_entries: usize,
    string_table_offset: usize,
) -> Result<Vec<FstEntry>> {
    let entries_size = total_entries
        .checked_mul(12)
        .ok_or_else(|| Error::InvalidIso("FST entry count overflow".into()))?;
    let entries_end = fst_offset
        .checked_add(entries_size)
        .ok_or_else(|| Error::InvalidIso("FST entry range overflow".into()))?;
    if entries_end > data.len()
        || string_table_offset < entries_end
        || string_table_offset > data.len()
    {
        return Err(Error::InvalidIso(
            "FST entries or string table are out of bounds".into(),
        ));
    }

    let mut entries = Vec::with_capacity(total_entries);
    let mut name_bytes = 0_usize;
    // The directories the walk is inside, as (index their entries end at,
    // length of `prefix` before they were entered).
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut prefix = String::new();
    for i in 0..total_entries {
        let entry_offset = fst_offset + i * 12;
        // Any set flag marks a directory, as Dolphin reads it.
        let is_dir = data[entry_offset] != 0;
        let file_offset = io::read_u32_be(data, entry_offset + 4);
        let file_size = io::read_u32_be(data, entry_offset + 8);

        // Entry 0 is the root directory, which has no name of its own.
        let (name, path) = if i == 0 {
            (String::new(), String::new())
        } else {
            while let Some(&(end, length)) = open.last()
                && i >= end
            {
                open.pop();
                prefix.truncate(length);
            }
            let name_offset = io::read_u24_be(data, entry_offset + 1) as usize;
            let name = string_table_offset
                .checked_add(name_offset)
                .and_then(|start| data.get(start..))
                .filter(|name| !name.is_empty())
                .ok_or_else(|| Error::InvalidIso("FST name offset is out of bounds".into()))?;
            let name = &name[..name
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(name.len())];
            // Every entry may point at the same long run of bytes, so what the
            // names add up to is bounded before any of them is copied.
            name_bytes = name_bytes
                .saturating_add(name.len().saturating_mul(2))
                .saturating_add(prefix.len());
            if name_bytes > MAX_NAME_BYTES {
                return Err(Error::InvalidIso(format!(
                    "FST names exceed {MAX_NAME_BYTES} byte limit"
                )));
            }
            let name = String::from_utf8_lossy(name).into_owned();
            let path = format!("{prefix}{name}");
            if is_dir {
                open.push((file_size as usize, prefix.len()));
                prefix.push_str(&name);
                prefix.push('/');
            }
            (name, path)
        };

        entries.push(FstEntry {
            is_dir,
            name,
            path,
            offset: file_offset,
            size: file_size,
            fst_index: i,
        });
    }

    Ok(entries)
}

/// The file `name` names: by its path when `name` has a `/` in it, else by
/// its bare name, which must then be the only file called that.
pub(crate) fn find_file<'a>(entries: &'a [FstEntry], name: &str) -> Result<&'a FstEntry> {
    let by_path = name.contains('/');
    let mut matches = entries
        .iter()
        .filter(|entry| !entry.is_dir && name == if by_path { &entry.path } else { &entry.name });
    let entry = matches
        .next()
        .ok_or_else(|| Error::FileNotFound(name.into()))?;
    if matches.next().is_some() {
        return Err(Error::AmbiguousName(name.into()));
    }
    Ok(entry)
}
