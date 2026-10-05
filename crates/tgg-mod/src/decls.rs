//! The hooks, events, exports, imports and game layout a mod's library
//! declares.
//!
//! Every `TGG_BEFORE`, `TGG_AFTER`, `TGG_REPLACE`, `TGG_ON`, `TGG_INIT`,
//! `TGG_EXPORT`, `TGG_IMPORT`, `TGG_API`, `TGG_STATE` and game symbol
//! reference in a mod's source becomes a fixed-size record in its library's
//! `tggdecls` section, as do the game layout id, target and mod API version it
//! was compiled against. A hooked function is a game symbol: a plain name, or
//! `file.c:name` for a static. The game reads the same records to install the
//! hooks, so what this module reports is what the mod does. The record layout
//! is tgg-melee's `tgg_decl` (`tgg/mod.h`, and `package-format.md` in its
//! docs).

use object::{Object, ObjectSection};
use serde::{Deserialize, Serialize};

/// "TGG1", little-endian: each record's first field.
pub const MAGIC: u32 = 0x3147_4754;
/// Bytes per record: magic, kind, a 120-byte symbol, a function pointer.
pub const RECORD_SIZE: usize = 136;
const SYMBOL_OFFSET: usize = 8;
const SYMBOL_SIZE: usize = 120;
/// The section the records live in.
pub const SECTION: &str = "tggdecls";

const KIND_BEFORE: u32 = 1;
const KIND_AFTER: u32 = 2;
const KIND_REPLACE: u32 = 3;
const KIND_GAME_ABI: u32 = 4;
const KIND_EXPORT: u32 = 5;
const KIND_IMPORT: u32 = 6;
/// A game symbol whose address the game fills into the mod.
const KIND_SYMBOL: u32 = 7;
const KIND_TARGET: u32 = 8;
/// A mod's own state that rolls back with the game: the name is the first
/// 112 bytes of the string field, and the size a u64 after it.
const KIND_STATE: u32 = 9;
const STATE_NAME_SIZE: usize = 112;
/// An event subscription: the event's name, from `tgg/events.h`.
const KIND_EVENT: u32 = 10;
/// The mod's `TGG_INIT` function; a library has at most one.
const KIND_INIT: u32 = 13;
/// The mod API `major.minor` the library was built against.
const KIND_API_VERSION: u32 = 14;

/// The game functions a mod hooks, each list sorted and without repeats.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hooks {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub before: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
    /// Functions the mod replaces. Two mods replacing one function can't
    /// both load.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
}

impl Hooks {
    pub fn is_empty(&self) -> bool {
        self.before.is_empty() && self.after.is_empty() && self.replaces.is_empty()
    }
}

/// What a library declares.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    /// The game layout id it was built against; `None` when it carries none.
    pub game_abi: Option<String>,
    /// The target triple it was built for, such as `x86_64-linux-gnu`;
    /// `None` when it carries none.
    pub target: Option<String>,
    /// The mod API `major.minor` it was built against; `None` when it carries
    /// none.
    pub api_version: Option<String>,
    /// Bytes of the mod's own state that roll back with the game.
    pub state: u64,
    /// Whether it has a `TGG_INIT` function.
    pub init: bool,
    pub hooks: Hooks,
    /// The events it subscribes to, sorted.
    pub events: Vec<String>,
    /// The game symbols whose addresses it takes, sorted. They change nothing
    /// about the game, so they never reach the manifest or a conflict; a
    /// build checks them against the game's symbols.
    pub symbols: Vec<String>,
    /// Names of the functions it offers other mods, sorted.
    pub exports: Vec<String>,
    /// What it takes from other mods, as `provider-id/export-name`, sorted.
    /// The game loads it only after every provider.
    pub imports: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeclError {
    #[error("not a library the game loads: {0}")]
    Object(#[from] object::Error),
    #[error("the library is not 64-bit little-endian x86-64 ELF")]
    Architecture,
    #[error("the library's {SECTION} section is not whole records")]
    Truncated,
    #[error("record {0} of the library's {SECTION} section is malformed")]
    Malformed(usize),
    #[error("record {0} of the library has kind {1}, which the game doesn't have")]
    UnknownKind(usize, u32),
    #[error("the library declares two game layouts ({0} and {1})")]
    TwoLayouts(String, String),
    #[error("the library declares two targets ({0} and {1})")]
    TwoTargets(String, String),
    #[error("the library declares two mod API versions ({0} and {1})")]
    TwoApiVersions(String, String),
    #[error("the library declares TGG_INIT twice")]
    TwoInits,
}

/// Read the declarations of `library`, an ELF shared object.
pub fn read(library: &[u8]) -> Result<Declarations, DeclError> {
    let file = object::File::parse(library)?;
    if file.format() != object::BinaryFormat::Elf
        || file.architecture() != object::Architecture::X86_64
        || !file.is_little_endian()
        || !file.is_64()
    {
        return Err(DeclError::Architecture);
    }
    let mut declarations = Declarations::default();
    let Some(section) = file.section_by_name(SECTION) else {
        return Ok(declarations);
    };
    let data = section.data()?;
    if data.len() % RECORD_SIZE != 0 {
        return Err(DeclError::Truncated);
    }
    for (index, record) in data.as_chunks::<RECORD_SIZE>().0.iter().enumerate() {
        let word = |at: usize| u32::from_le_bytes(record[at..at + 4].try_into().expect("4 bytes"));
        // A linker may pad between records; padding is all zero.
        if word(0) == 0 {
            continue;
        }
        if word(4) == KIND_STATE {
            let name = &record[SYMBOL_OFFSET..SYMBOL_OFFSET + STATE_NAME_SIZE];
            let at = SYMBOL_OFFSET + STATE_NAME_SIZE;
            let size = u64::from_le_bytes(record[at..at + 8].try_into().expect("8 bytes"));
            if word(0) != MAGIC || !name.contains(&0) || size == 0 {
                return Err(DeclError::Malformed(index));
            }
            declarations.state = declarations
                .state
                .checked_add(size)
                .ok_or(DeclError::Malformed(index))?;
            continue;
        }
        let symbol = &record[SYMBOL_OFFSET..SYMBOL_OFFSET + SYMBOL_SIZE];
        let end = symbol
            .iter()
            .position(|&b| b == 0)
            .ok_or(DeclError::Malformed(index))?;
        let symbol = std::str::from_utf8(&symbol[..end])
            .map_err(|_| DeclError::Malformed(index))?
            .to_owned();
        if word(0) != MAGIC {
            return Err(DeclError::Malformed(index));
        }
        let hooks = &mut declarations.hooks;
        match word(4) {
            KIND_BEFORE => hooks.before.push(symbol),
            KIND_AFTER => hooks.after.push(symbol),
            KIND_REPLACE => hooks.replaces.push(symbol),
            KIND_EXPORT if !symbol.is_empty() && !symbol.contains('/') => {
                declarations.exports.push(symbol)
            }
            KIND_IMPORT if import_is_valid(&symbol) => declarations.imports.push(symbol),
            KIND_SYMBOL if !symbol.is_empty() => declarations.symbols.push(symbol),
            KIND_EVENT if !symbol.is_empty() => declarations.events.push(symbol),
            KIND_INIT if declarations.init => return Err(DeclError::TwoInits),
            KIND_INIT => declarations.init = true,
            KIND_TARGET => match &declarations.target {
                Some(known) if *known != symbol => {
                    return Err(DeclError::TwoTargets(known.clone(), symbol));
                }
                _ => declarations.target = Some(symbol),
            },
            KIND_GAME_ABI => match &declarations.game_abi {
                Some(known) if *known != symbol => {
                    return Err(DeclError::TwoLayouts(known.clone(), symbol));
                }
                _ => declarations.game_abi = Some(symbol),
            },
            KIND_API_VERSION if api_version_is_valid(&symbol) => match &declarations.api_version {
                Some(known) if *known != symbol => {
                    return Err(DeclError::TwoApiVersions(known.clone(), symbol));
                }
                _ => declarations.api_version = Some(symbol),
            },
            KIND_EXPORT | KIND_IMPORT | KIND_SYMBOL | KIND_EVENT | KIND_API_VERSION => {
                return Err(DeclError::Malformed(index));
            }
            kind => return Err(DeclError::UnknownKind(index, kind)),
        }
    }
    for list in [
        &mut declarations.hooks.before,
        &mut declarations.hooks.after,
        &mut declarations.hooks.replaces,
        &mut declarations.events,
        &mut declarations.symbols,
        &mut declarations.exports,
        &mut declarations.imports,
    ] {
        list.sort();
        list.dedup();
    }
    Ok(declarations)
}

/// `major.minor`, both numbers.
fn api_version_is_valid(symbol: &str) -> bool {
    let number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    matches!(symbol.split_once('.'), Some((major, minor)) if number(major) && number(minor))
}

/// `provider-id/export-name`, both parts present.
fn import_is_valid(symbol: &str) -> bool {
    matches!(symbol.split_once('/'), Some((provider, name)) if !provider.is_empty() && !name.is_empty() && !name.contains('/'))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use object::write::{Object as WriteObject, StandardSection};
    use object::{Architecture, BinaryFormat, Endianness, SectionKind};

    /// One record as the game's macros lay it out.
    pub(crate) fn record(kind: u32, symbol: &str) -> Vec<u8> {
        let mut bytes = vec![0; RECORD_SIZE];
        bytes[..4].copy_from_slice(&MAGIC.to_le_bytes());
        bytes[4..8].copy_from_slice(&kind.to_le_bytes());
        bytes[SYMBOL_OFFSET..SYMBOL_OFFSET + symbol.len()].copy_from_slice(symbol.as_bytes());
        bytes
    }

    /// An x86-64 ELF object whose tggdecls section holds `records`, in
    /// place of a compiled mod.
    pub(crate) fn library(records: &[Vec<u8>]) -> Vec<u8> {
        let mut object =
            WriteObject::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
        let text = object.section_id(StandardSection::Text);
        object.append_section_data(text, &[0xc3], 1);
        let decls = object.add_section(Vec::new(), SECTION.as_bytes().to_vec(), SectionKind::Data);
        object.append_section_data(decls, &records.concat(), 8);
        object.write().expect("write the object")
    }

    pub(crate) const BEFORE: u32 = KIND_BEFORE;
    pub(crate) const REPLACE: u32 = KIND_REPLACE;
    pub(crate) const GAME_ABI: u32 = KIND_GAME_ABI;
    pub(crate) const EXPORT: u32 = KIND_EXPORT;
    pub(crate) const IMPORT: u32 = KIND_IMPORT;
    pub(crate) const SYMBOL: u32 = KIND_SYMBOL;
    pub(crate) const TARGET: u32 = KIND_TARGET;
    pub(crate) const EVENT: u32 = KIND_EVENT;
    pub(crate) const INIT: u32 = KIND_INIT;
    pub(crate) const API_VERSION: u32 = KIND_API_VERSION;

    /// The records every source file of a mod built with the SDK carries.
    pub(crate) fn built_with_sdk() -> Vec<Vec<u8>> {
        vec![
            record(GAME_ABI, "6a0e926ca3e90452"),
            record(TARGET, "x86_64-linux-gnu"),
            record(API_VERSION, "0.1"),
        ]
    }

    /// A state record for `size` bytes named `name`.
    pub(crate) fn state_record(name: &str, size: u64) -> Vec<u8> {
        let mut bytes = record(KIND_STATE, name);
        let at = SYMBOL_OFFSET + STATE_NAME_SIZE;
        bytes[at..at + 8].copy_from_slice(&size.to_le_bytes());
        bytes
    }

    #[test]
    fn a_record_kind_the_game_doesnt_have_is_refused_as_the_loader_refuses_it() {
        // Kinds 11 and 12 (content, settings) are reserved; this game has
        // neither.
        let mut records = built_with_sdk();
        records.push(record(11, "menu"));
        assert!(matches!(
            read(&library(&records)),
            Err(DeclError::UnknownKind(3, 11))
        ));
        let mut records = built_with_sdk();
        records.extend([record(INIT, ""), record(INIT, "")]);
        assert!(matches!(read(&library(&records)), Err(DeclError::TwoInits)));
    }
}
