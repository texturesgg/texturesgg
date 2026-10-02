//! Bounded, loss-aware parsing for the raw HSD DAT archive container.
//!
//! This crate owns byte access, declared archive extents, ordinary relocation
//! sites and pointer resolution, public roots, extern table records, parser
//! errors, and resource limits. It deliberately contains no HAL evaluation,
//! renderer contracts, exporters, ISO/FST handling, or application policy.

pub mod header;
pub mod reader;
mod reloc;
pub mod root;

use header::{DATA_SECTION_OFFSET, DatHeader};
use reader::{Reader, read_u32_at};
use root::RootNode;

const MAX_RELOCATIONS: u32 = 1_000_000;
const MAX_NAMED_ROOTS: u32 = 65_536;
const MAX_EXTERNS: u32 = 65_536;
const MAX_EXTERN_FIXUP_SITES: usize = 1_000_000;
const MAX_ROOT_SYMBOL_BYTES: usize = 1024 * 1024;
const MAX_EXTERN_SYMBOL_BYTES: usize = 1024 * 1024;

/// Parsed HSD .dat file.
///
/// The data section contains all structs — joints, meshes, materials, textures, etc.
/// Pointers within the data section are u32 offsets relative to the start of the data section.
/// The relocation table tells us which locations contain pointers (for resolution).
///
/// The fields are public so that a caller can build an archive in code or
/// patch its data in place. [`Self::parse`] establishes these invariants, and
/// a value built or changed by hand has to keep them itself:
///
/// - `relocation_sites` is sorted ascending with no duplicates
///   ([`Self::resolve_pointer`] binary-searches it, so an unsorted list makes
///   relocated fields read as unrelocated);
/// - every relocation site is a four-byte field inside `data` whose value is
///   at most `data.len()`;
/// - `header` agrees with the other fields: `data_size` is `data.len()` and
///   the three counts are the lengths of `relocation_sites`, `roots` and
///   `externs`;
/// - the counts and the symbol bytes are within this crate's limits.
///
/// [`Self::from_parts`] sorts the sites and derives the header, and checks
/// nothing else. Breaking an invariant never panics or reads out of bounds:
/// every accessor is bounds-checked and reports a [`DatPointerError`] or
/// `None`, but the answers describe an archive no file could hold.
#[derive(Debug, Clone)]
pub struct DatFile {
    /// Raw data section bytes (starts at file offset 0x20).
    pub data: Vec<u8>,
    /// Parsed header metadata.
    pub header: DatHeader,
    /// Sorted data-relative pointer-field sites from the ordinary relocation table.
    pub relocation_sites: Vec<u32>,
    /// Named root nodes (entry points into the scene graph).
    pub roots: Vec<RootNode>,
    /// Named extern references. Their `data_offset` values are fixup-chain head
    /// pointer-field sites, not object starts.
    pub externs: Vec<RootNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DatPointerError {
    FieldOutOfBounds,
    MissingRelocation,
    TargetOutOfBounds,
}

impl std::fmt::Display for DatPointerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::FieldOutOfBounds => "pointer field is out of bounds",
            Self::MissingRelocation => "pointer field is not relocated",
            Self::TargetOutOfBounds => "pointer target is out of bounds",
        })
    }
}

impl std::error::Error for DatPointerError {}

/// Invalid external fixup-chain structure, distinct from ordinary relocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DatExternError {
    FieldOutOfBounds { site: u32 },
    RepeatedSite { site: u32 },
    RelocationOverlap { site: u32 },
    LimitExceeded { limit: usize },
}

impl std::fmt::Display for DatExternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FieldOutOfBounds { site } => {
                write!(f, "external fixup field {site:#010x} is out of bounds")
            }
            Self::RepeatedSite { site } => {
                write!(f, "repeated external fixup field {site:#010x}")
            }
            Self::RelocationOverlap { site } => {
                write!(
                    f,
                    "external fixup field {site:#010x} is also ordinarily relocated"
                )
            }
            Self::LimitExceeded { limit } => {
                write!(f, "DAT exceeds the external fixup site budget of {limit}")
            }
        }
    }
}

impl std::error::Error for DatExternError {}

#[derive(Debug)]
#[non_exhaustive]
pub enum DatParseError {
    TooSmall,
    InvalidHeader,
    FileSizeMismatch { declared: u32, actual: usize },
    InvalidTableLayout,
    DuplicateRelocationSite,
    InvalidRelocation(DatPointerError),
    ResourceLimit { resource: DatResource, limit: usize },
}

/// What a [`DatParseError::ResourceLimit`] counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DatResource {
    Relocations,
    NamedRoots,
    Externs,
    /// Bytes of root names copied out of the symbol table.
    RootSymbolBytes,
    /// Bytes of extern names copied out of the symbol table.
    ExternSymbolBytes,
}

impl std::fmt::Display for DatResource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Relocations => "relocation",
            Self::NamedRoots => "named root",
            Self::Externs => "extern",
            Self::RootSymbolBytes => "root symbol byte",
            Self::ExternSymbolBytes => "extern symbol byte",
        })
    }
}

impl std::fmt::Display for DatParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall => formatter.write_str("file too small to contain HSD header"),
            Self::InvalidHeader => formatter.write_str("invalid or corrupt HSD header"),
            Self::FileSizeMismatch { declared, actual } => write!(
                formatter,
                "declared DAT file size {declared} does not match input length {actual}"
            ),
            Self::InvalidTableLayout => {
                formatter.write_str("header table counts exceed the file bounds")
            }
            Self::DuplicateRelocationSite => {
                formatter.write_str("duplicate ordinary relocation site")
            }
            Self::InvalidRelocation(error) => {
                write!(formatter, "invalid ordinary relocation: {error}")
            }
            Self::ResourceLimit { resource, limit } => {
                write!(formatter, "DAT exceeds the {resource} budget of {limit}")
            }
        }
    }
}

impl std::error::Error for DatParseError {}

impl DatFile {
    /// A DAT from its data section, roots, and relocation sites, with the
    /// header those imply and no externs. For building one in code: the
    /// sites are sorted, and nothing else is validated as [`Self::parse`]
    /// validates a file (see the invariants on [`DatFile`]).
    pub fn from_parts(data: Vec<u8>, roots: Vec<RootNode>, mut relocation_sites: Vec<u32>) -> Self {
        relocation_sites.sort_unstable();
        // A data section too large for the header's fields saturates them.
        let data_size = u32::try_from(data.len()).unwrap_or(u32::MAX);
        Self {
            header: DatHeader {
                file_size: data_size.saturating_add(DATA_SECTION_OFFSET as u32),
                data_size,
                reloc_count: relocation_sites.len() as u32,
                root_count: roots.len() as u32,
                extern_count: 0,
                version: [0; 4],
            },
            data,
            relocation_sites,
            roots,
            externs: Vec::new(),
        }
    }

    /// Parse a .dat file from raw bytes.
    ///
    /// The declared file size must match the complete supplied archive, as required by
    /// HSD_ArchiveParse. Callers reading an embedded archive must supply its exact extent.
    pub fn parse(raw: &[u8]) -> Result<Self, DatParseError> {
        if raw.len() < DATA_SECTION_OFFSET {
            return Err(DatParseError::TooSmall);
        }

        let mut reader = Reader::new(raw);
        let header = DatHeader::parse(&mut reader).ok_or(DatParseError::InvalidHeader)?;
        if header.file_size as usize != raw.len() {
            return Err(DatParseError::FileSizeMismatch {
                declared: header.file_size,
                actual: raw.len(),
            });
        }
        for (resource, count, limit) in [
            (
                DatResource::Relocations,
                header.reloc_count,
                MAX_RELOCATIONS,
            ),
            (DatResource::NamedRoots, header.root_count, MAX_NAMED_ROOTS),
            (DatResource::Externs, header.extern_count, MAX_EXTERNS),
        ] {
            if count > limit {
                return Err(DatParseError::ResourceLimit {
                    resource,
                    limit: limit as usize,
                });
            }
        }

        let data_end = (DATA_SECTION_OFFSET as u64)
            .checked_add(header.data_size as u64)
            .ok_or(DatParseError::InvalidTableLayout)?;
        let reloc_end = data_end
            .checked_add((header.reloc_count as u64).saturating_mul(4))
            .ok_or(DatParseError::InvalidTableLayout)?;
        let root_end = reloc_end
            .checked_add((header.root_count as u64).saturating_mul(8))
            .ok_or(DatParseError::InvalidTableLayout)?;
        let tables_end = root_end
            .checked_add((header.extern_count as u64).saturating_mul(8))
            .ok_or(DatParseError::InvalidTableLayout)?;
        if tables_end > raw.len() as u64 {
            return Err(DatParseError::InvalidTableLayout);
        }

        // Extract data section
        let data =
            raw[DATA_SECTION_OFFSET..DATA_SECTION_OFFSET + header.data_size as usize].to_vec();

        // Parse and validate the ordinary relocation table before exposing it to
        // descriptor parsers. Matching HAL trusts these values; untrusted DATs cannot.
        let relocation_sites =
            reloc::parse_relocation_sites(raw, &header).ok_or(DatParseError::InvalidTableLayout)?;
        if relocation_sites
            .windows(2)
            .any(|sites| sites[0] == sites[1])
        {
            return Err(DatParseError::DuplicateRelocationSite);
        }
        for &site in &relocation_sites {
            let target = read_u32_at(&data, site as usize)
                .ok_or(DatPointerError::FieldOutOfBounds)
                .map_err(DatParseError::InvalidRelocation)?;
            if target as usize > data.len() {
                return Err(DatParseError::InvalidRelocation(
                    DatPointerError::TargetOutOfBounds,
                ));
            }
        }

        // Parse root table
        let roots = root::parse_root_table(
            raw,
            header.root_table_offset(),
            header.symbol_table_offset(),
            header.root_count,
            MAX_ROOT_SYMBOL_BYTES,
        )
        .map_err(|error| match error {
            root::RootTableError::InvalidLayout => DatParseError::InvalidTableLayout,
            root::RootTableError::SymbolBudget => DatParseError::ResourceLimit {
                resource: DatResource::RootSymbolBytes,
                limit: MAX_ROOT_SYMBOL_BYTES,
            },
        })?;

        // Parse extern table
        let externs = root::parse_extern_table(
            raw,
            header.extern_table_offset(),
            header.symbol_table_offset(),
            header.extern_count,
            MAX_EXTERN_SYMBOL_BYTES,
        )
        .map_err(|error| match error {
            root::RootTableError::InvalidLayout => DatParseError::InvalidTableLayout,
            root::RootTableError::SymbolBudget => DatParseError::ResourceLimit {
                resource: DatResource::ExternSymbolBytes,
                limit: MAX_EXTERN_SYMBOL_BYTES,
            },
        })?;

        Ok(Self {
            data,
            header,
            relocation_sites,
            roots,
            externs,
        })
    }

    /// Read a u32 from the data section at the given offset (big-endian).
    pub fn read_u32(&self, offset: u32) -> Option<u32> {
        read_u32_at(&self.data, offset as usize)
    }

    /// Resolve an ordinary nullable pointer field from the data section.
    ///
    /// Relocation membership is authoritative: a listed field contains a non-null
    /// data-relative target, including target zero. A target equal to the data length is
    /// retained as a one-past-end address; dereferencing it still requires a bounded range.
    /// An unlisted zero is null. A nonzero unlisted word is not accepted as a pointer.
    pub fn resolve_pointer(&self, offset: u32) -> Result<Option<u32>, DatPointerError> {
        let value = self
            .read_u32(offset)
            .ok_or(DatPointerError::FieldOutOfBounds)?;
        if self.relocation_sites.binary_search(&offset).is_err() {
            return if value == 0 {
                Ok(None)
            } else {
                Err(DatPointerError::MissingRelocation)
            };
        }
        if value as usize > self.data.len() {
            return Err(DatPointerError::TargetOutOfBounds);
        }
        Ok(Some(value))
    }

    /// Classify every field in the declared external fixup chains without linking
    /// symbols, rewriting bytes, or treating chain words as ordinary pointers.
    ///
    /// HAL's `HSD_ArchiveLocateExtern` follows raw links until `u32::MAX`; zero
    /// is a valid field site. Callers may use this set to model Melee's
    /// `lbArchive_InitializeDAT`, which initially links these fields to null.
    pub fn external_fixup_sites(&self) -> Result<std::collections::HashSet<u32>, DatExternError> {
        let mut sites = std::collections::HashSet::new();
        for external in &self.externs {
            let mut site = external.data_offset;
            while site != u32::MAX {
                let next = self
                    .read_u32(site)
                    .ok_or(DatExternError::FieldOutOfBounds { site })?;
                if self.relocation_sites.binary_search(&site).is_ok() {
                    return Err(DatExternError::RelocationOverlap { site });
                }
                if sites.len() >= MAX_EXTERN_FIXUP_SITES && !sites.contains(&site) {
                    return Err(DatExternError::LimitExceeded {
                        limit: MAX_EXTERN_FIXUP_SITES,
                    });
                }
                if !sites.insert(site) {
                    return Err(DatExternError::RepeatedSite { site });
                }
                site = next;
            }
        }
        Ok(sites)
    }

    /// Resolve a required ordinary relocated pointer field.
    pub fn resolve_required_pointer(&self, offset: u32) -> Result<u32, DatPointerError> {
        self.resolve_pointer(offset)?
            .ok_or(DatPointerError::MissingRelocation)
    }

    /// Read a f32 from the data section at the given offset.
    pub fn read_f32(&self, offset: u32) -> Option<f32> {
        reader::read_f32_at(&self.data, offset as usize)
    }

    /// Read a u16 from the data section at the given offset.
    pub fn read_u16(&self, offset: u32) -> Option<u16> {
        reader::read_u16_at(&self.data, offset as usize)
    }

    /// Read a u8 from the data section at the given offset.
    pub fn read_u8(&self, offset: u32) -> Option<u8> {
        self.data.get(offset as usize).copied()
    }

    /// Get a slice of the data section.
    pub fn data_slice(&self, offset: u32, len: usize) -> Option<&[u8]> {
        let start = offset as usize;
        let end = start.checked_add(len)?;
        self.data.get(start..end)
    }
}

/// A summary of the archive: its header counts, roots and externs.
impl std::fmt::Display for DatFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== HSD .dat File ===")?;
        writeln!(f, "  File size:    {} bytes", self.header.file_size)?;
        writeln!(f, "  Data size:    {} bytes", self.header.data_size)?;
        writeln!(f, "  Reloc count:  {}", self.header.reloc_count)?;
        writeln!(f, "  Root count:   {}", self.header.root_count)?;
        writeln!(f, "  Extern count: {}", self.header.extern_count)?;
        writeln!(f, "  Roots:")?;
        for root in &self.roots {
            writeln!(f, "    {:?} @ 0x{:08X}", root.name, root.data_offset)?;
        }
        if !self.externs.is_empty() {
            writeln!(f, "  Externs:")?;
            for ext in &self.externs {
                writeln!(f, "    {:?} @ 0x{:08X}", ext.name, ext.data_offset)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_resolution_classifies_relocation_and_bounds() {
        let mut data = vec![0; 24];
        data[0x08..0x0C].copy_from_slice(&4u32.to_be_bytes());
        data[0x0C..0x10].copy_from_slice(&4u32.to_be_bytes());
        data[0x10..0x14].copy_from_slice(&24u32.to_be_bytes());
        data[0x14..0x18].copy_from_slice(&u32::MAX.to_be_bytes());
        let dat = DatFile {
            header: DatHeader {
                file_size: 0x20 + data.len() as u32,
                data_size: data.len() as u32,
                reloc_count: 4,
                root_count: 0,
                extern_count: 0,
                version: [0; 4],
            },
            data,
            relocation_sites: vec![0x00, 0x0C, 0x10, 0x14],
            roots: Vec::new(),
            externs: Vec::new(),
        };

        assert_eq!(dat.resolve_pointer(0x00), Ok(Some(0)));
        assert_eq!(dat.resolve_pointer(0x04), Ok(None));
        assert_eq!(
            dat.resolve_pointer(0x08),
            Err(DatPointerError::MissingRelocation)
        );
        assert_eq!(dat.resolve_pointer(0x0C), Ok(Some(4)));
        assert_eq!(dat.resolve_pointer(0x10), Ok(Some(24)));
        assert_eq!(
            dat.resolve_pointer(0x14),
            Err(DatPointerError::TargetOutOfBounds)
        );
        assert_eq!(
            dat.resolve_pointer(0x18),
            Err(DatPointerError::FieldOutOfBounds)
        );
    }

    fn raw_dat(data: &[u8], relocation_sites: &[u32]) -> Vec<u8> {
        let file_size = DATA_SECTION_OFFSET + data.len() + relocation_sites.len() * 4;
        let mut raw = vec![0; file_size];
        raw[0x00..0x04].copy_from_slice(&(file_size as u32).to_be_bytes());
        raw[0x04..0x08].copy_from_slice(&(data.len() as u32).to_be_bytes());
        raw[0x08..0x0C].copy_from_slice(&(relocation_sites.len() as u32).to_be_bytes());
        raw[DATA_SECTION_OFFSET..DATA_SECTION_OFFSET + data.len()].copy_from_slice(data);
        for (index, site) in relocation_sites.iter().enumerate() {
            let start = DATA_SECTION_OFFSET + data.len() + index * 4;
            raw[start..start + 4].copy_from_slice(&site.to_be_bytes());
        }
        raw
    }

    #[test]
    fn external_fixup_chains_preserve_zero_links_and_raw_words() {
        let mut data = vec![0; 12];
        data[..4].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut dat = DatFile::parse(&raw_dat(&data, &[])).unwrap();
        dat.externs = vec![
            RootNode {
                name: "linked".into(),
                data_offset: 8,
            },
            RootNode {
                name: "empty".into(),
                data_offset: u32::MAX,
            },
        ];
        assert_eq!(
            dat.external_fixup_sites().unwrap(),
            std::collections::HashSet::from([0, 8])
        );
        assert_eq!(dat.data, data);
        // External classification does not change the ordinary pointer domain.
        assert_eq!(
            dat.resolve_pointer(0),
            Err(DatPointerError::MissingRelocation)
        );
        assert_eq!(dat.resolve_pointer(8), Ok(None));
    }

    #[test]
    fn external_fixup_chains_reject_invalid_fields_and_repeated_ownership() {
        let mut dat = DatFile::parse(&raw_dat(&[0; 8], &[])).unwrap();
        dat.externs = vec![RootNode {
            name: "linked".into(),
            data_offset: 0,
        }];
        assert_eq!(
            dat.external_fixup_sites(),
            Err(DatExternError::RepeatedSite { site: 0 })
        );

        dat.data[..4].copy_from_slice(&4u32.to_be_bytes());
        assert_eq!(
            dat.external_fixup_sites(),
            Err(DatExternError::RepeatedSite { site: 0 })
        );

        dat.data[..4].copy_from_slice(&u32::MAX.to_be_bytes());
        dat.externs.push(RootNode {
            name: "other".into(),
            data_offset: 0,
        });
        assert_eq!(
            dat.external_fixup_sites(),
            Err(DatExternError::RepeatedSite { site: 0 })
        );
        dat.externs.pop();

        for site in [5, 8, u32::MAX - 1] {
            dat.data[..4].copy_from_slice(&site.to_be_bytes());
            assert_eq!(
                dat.external_fixup_sites(),
                Err(DatExternError::FieldOutOfBounds { site })
            );
        }
        dat.data[..4].copy_from_slice(&u32::MAX.to_be_bytes());
        dat.relocation_sites.push(0);
        assert_eq!(
            dat.external_fixup_sites(),
            Err(DatExternError::RelocationOverlap { site: 0 })
        );
    }

    #[test]
    fn external_fixup_site_budget_is_aggregate_across_symbols() {
        let mut data = vec![0; (MAX_EXTERN_FIXUP_SITES + 1) * 4];
        for (index, word) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *word = (((index + 1) * 4) as u32).to_be_bytes();
        }
        data[(MAX_EXTERN_FIXUP_SITES - 1) * 4..MAX_EXTERN_FIXUP_SITES * 4]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        data[MAX_EXTERN_FIXUP_SITES * 4..].copy_from_slice(&u32::MAX.to_be_bytes());
        let mut dat = DatFile::parse(&raw_dat(&data, &[])).unwrap();
        dat.externs = vec![RootNode {
            name: "first".into(),
            data_offset: 0,
        }];
        assert_eq!(
            dat.external_fixup_sites().unwrap().len(),
            MAX_EXTERN_FIXUP_SITES
        );
        dat.externs.push(RootNode {
            name: "second".into(),
            data_offset: (MAX_EXTERN_FIXUP_SITES * 4) as u32,
        });
        assert_eq!(
            dat.external_fixup_sites(),
            Err(DatExternError::LimitExceeded {
                limit: MAX_EXTERN_FIXUP_SITES
            })
        );
    }

    #[test]
    fn parse_requires_the_declared_archive_extent() {
        let mut raw = raw_dat(&[0; 8], &[0]);
        let actual = raw.len();
        assert!(DatFile::parse(&raw).is_ok());

        for declared in [0, actual as u32 - 1, actual as u32 + 1, u32::MAX] {
            raw[..4].copy_from_slice(&declared.to_be_bytes());
            assert!(matches!(
                DatFile::parse(&raw),
                Err(DatParseError::FileSizeMismatch {
                    declared: reported_declared,
                    actual: reported_actual,
                }) if reported_declared == declared && reported_actual == actual
            ));
        }

        // Even an otherwise empty archive must declare its 0x20-byte header.
        assert!(matches!(
            DatFile::parse(&[0; DATA_SECTION_OFFSET]),
            Err(DatParseError::FileSizeMismatch {
                declared: 0,
                actual: DATA_SECTION_OFFSET,
            })
        ));
        assert!(DatFile::parse(&raw_dat(&[], &[])).is_ok());
    }

    #[test]
    fn parse_accepts_one_past_and_rejects_malformed_relocations() {
        let one_past_end = DatFile::parse(&raw_dat(&4u32.to_be_bytes(), &[0])).unwrap();
        assert_eq!(one_past_end.resolve_pointer(0), Ok(Some(4)));

        assert!(matches!(
            DatFile::parse(&raw_dat(&[0; 4], &[4])),
            Err(DatParseError::InvalidRelocation(
                DatPointerError::FieldOutOfBounds
            ))
        ));

        assert!(matches!(
            DatFile::parse(&raw_dat(&5u32.to_be_bytes(), &[0])),
            Err(DatParseError::InvalidRelocation(
                DatPointerError::TargetOutOfBounds
            ))
        ));

        assert!(matches!(
            DatFile::parse(&raw_dat(&[0; 4], &[0, 0])),
            Err(DatParseError::DuplicateRelocationSite)
        ));
    }

    /// A root table of one entry after `data`, naming a symbol at
    /// `symbol_offset` in a string table holding `symbols`.
    fn raw_dat_with_root(data: &[u8], symbol_offset: u32, symbols: &[u8]) -> Vec<u8> {
        let mut raw = raw_dat(data, &[]);
        raw.extend(0u32.to_be_bytes());
        raw.extend(symbol_offset.to_be_bytes());
        raw.extend(symbols);
        let file_size = raw.len() as u32;
        raw[0x00..0x04].copy_from_slice(&file_size.to_be_bytes());
        raw[0x0C..0x10].copy_from_slice(&1u32.to_be_bytes());
        raw
    }

    #[test]
    fn a_root_names_its_symbol_or_the_table_is_refused() {
        let dat = DatFile::parse(&raw_dat_with_root(&[0; 4], 1, b"\0top\0")).unwrap();
        assert_eq!(dat.roots[0].name, "top");

        for (symbol_offset, symbols) in [
            // Past the string table.
            (64, &b"top\0"[..]),
            // No terminator before the end of the file.
            (0, &b"top"[..]),
            // Not UTF-8.
            (0, &b"\xFF\0"[..]),
        ] {
            assert!(matches!(
                DatFile::parse(&raw_dat_with_root(&[0; 4], symbol_offset, symbols)),
                Err(DatParseError::InvalidTableLayout)
            ));
        }
    }

    #[test]
    fn header_counts_are_checked_before_anything_is_allocated() {
        // A table the file is too short to hold.
        let mut raw = raw_dat(&[0; 4], &[]);
        raw[0x0C..0x10].copy_from_slice(&1u32.to_be_bytes());
        assert!(matches!(
            DatFile::parse(&raw),
            Err(DatParseError::InvalidTableLayout)
        ));

        // A count over the crate's limit, however long the file claims to be.
        // The error names which count it was.
        for (field, counted) in [
            (0x08, DatResource::Relocations),
            (0x0C, DatResource::NamedRoots),
            (0x10, DatResource::Externs),
        ] {
            let mut raw = raw_dat(&[0; 4], &[]);
            raw[field..field + 4].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(matches!(
                DatFile::parse(&raw),
                Err(DatParseError::ResourceLimit { resource, .. }) if resource == counted
            ));
        }
    }
}
