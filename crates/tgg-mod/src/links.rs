//! Whether a mod's library would link when the game loads it.
//!
//! Each undefined dynamic symbol of the library, weak ones included, must
//! resolve where the game runs. A symbol without a version comes from the
//! game: it must be one the game exports (a line of `symbols.txt` without
//! `file.c:` and not `local`); the GameCube SDK functions the game calls are
//! the port's own, and mods can't link against them. A `GLIBC_x.y` version
//! must be no newer than the oldest glibc the game runs on, or the mod would
//! load where it was built and be refused on older systems. This is the check
//! the SDK's `TggCheck.cmake` makes for CMake builds.

use crate::symbols::Symbols;
use object::elf;
use object::read::elf::{ElfFile64, Sym};

/// A symbol the library needs that the game's system can't give it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("{0} isn't something the game exports")]
    NotExported(String),
    #[error("{0} needs glibc {1}; the game runs on {2}")]
    Glibc(String, String, String),
}

/// Every problem with linking `library` into a game whose symbols are
/// `symbols` and whose oldest glibc is `glibc`.
pub fn check(
    library: &[u8],
    symbols: &Symbols,
    glibc: &str,
) -> Result<Vec<LinkError>, object::Error> {
    let file = ElfFile64::<object::Endianness>::parse(library)?;
    let endian = file.endian();
    let sections = file.elf_section_table();
    let table = sections.symbols(endian, library, elf::SHT_DYNSYM)?;
    let versions = sections.versions(endian, library)?;
    let mut problems = Vec::new();
    for (index, symbol) in table.enumerate() {
        if !symbol.is_undefined(endian) || index.0 == 0 {
            continue;
        }
        let name = String::from_utf8_lossy(symbol.name(endian, table.strings())?).into_owned();
        let version = match &versions {
            Some(versions) => versions
                .version(versions.version_index(endian, index))?
                .map(|version| String::from_utf8_lossy(version.name()).into_owned()),
            None => None,
        };
        match version {
            Some(version) => {
                if let Some(needed) = version.strip_prefix("GLIBC_")
                    && newer(needed, glibc)
                {
                    problems.push(LinkError::Glibc(name, needed.to_owned(), glibc.to_owned()));
                }
            }
            None => {
                // Weak symbols the toolchain itself references.
                if name == "__gmon_start__" || name.starts_with("_ITM_") {
                    continue;
                }
                if !symbols.links(&name) {
                    problems.push(LinkError::NotExported(name));
                }
            }
        }
    }
    Ok(problems)
}

/// Whether the dotted version `a` is newer than `b`.
fn newer(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) > parts(b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn glibc_versions_compare_by_number() {
        assert!(super::newer("2.38", "2.34"));
        assert!(super::newer("2.4.1", "2.4"));
        assert!(!super::newer("2.10", "2.34"));
        assert!(!super::newer("2.34", "2.34"));
    }
}
