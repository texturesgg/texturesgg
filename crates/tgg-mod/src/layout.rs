//! The functions a game layout lets mods name, read once from a port
//! executable's symbol table.
//!
//! A registry reads a layout when it registers it, then checks each mod
//! built for that layout against it: a hook on a function the game doesn't
//! have, or on a static whose name several files share, is refused at publish
//! instead of in a player's log. It also gives each hook one canonical name:
//! an exported function by its plain name, a static as `file.c:name`. The
//! runtime treats `name` and `file.c:name` as one function when they resolve
//! to the same code, so conflict checks have to compare canonical names.
//!
//! A static the compiler also copied (`name.constprop.N`, `name.isra.N`,
//! `name.part.N`, in the same file) is refused as a hook: calls that reach the
//! copy skip the hook, and tgg-mod-runtime refuses it at load for the same
//! reason. `name.cold` is not a copy: it is the function's cold tail, and
//! every call still enters through `name`.

use crate::decls::Hooks;
use object::{Object, ObjectSymbol, SymbolKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Suffixes GCC gives the copies it makes of a function.
const CLONE_SUFFIXES: &[&str] = &["constprop", "isra", "part"];

/// A registered layout: which port build it came from, and its symbols.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layout {
    pub api: String,
    pub game_abi: String,
    pub target: String,
    pub port: String,
    #[serde(flatten)]
    pub symbols: Symbols,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbols {
    /// Functions the executable exports, by name.
    pub exported: BTreeSet<String>,
    /// Static functions, by the file the symbol table names for them.
    pub statics: BTreeMap<String, BTreeSet<String>>,
    /// Statics the compiler also copied, by file.
    pub cloned: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SymbolError {
    #[error("{0}: the game has no function by that name")]
    Unknown(String),
    #[error("{0}: static in several files ({1}); name it as file.c:{0}")]
    Ambiguous(String, String),
    #[error("{0}: the compiler also copied this static, so calls to the copy would skip the hook")]
    Cloned(String),
}

/// GCC's name for a copy of `base`, such as `base.constprop.0`, gives `base`.
fn clone_base(name: &str) -> Option<&str> {
    let (base, suffix) = name.split_once('.')?;
    let kind = suffix.split('.').next()?;
    CLONE_SUFFIXES.contains(&kind).then_some(base)
}

impl Symbols {
    /// Read the function symbols of an executable. Statics belong to the file
    /// named by the last file symbol before them, as GCC and the ELF symbol
    /// table lay them out.
    pub fn read(executable: &[u8]) -> Result<Self, object::Error> {
        let file = object::File::parse(executable)?;
        let mut symbols = Self::default();
        let mut current_file: Option<String> = None;
        for symbol in file.symbols() {
            if symbol.kind() == SymbolKind::File {
                current_file = symbol.name().ok().map(str::to_owned);
                continue;
            }
            if symbol.kind() != SymbolKind::Text || !symbol.is_definition() {
                continue;
            }
            let Ok(name) = symbol.name() else { continue };
            // A cold tail is part of its function, not one a mod can name.
            if name.is_empty() || name.contains(".cold") {
                continue;
            }
            if symbol.is_local() {
                let Some(file) = &current_file else { continue };
                match clone_base(name) {
                    Some(base) => {
                        symbols
                            .cloned
                            .entry(file.clone())
                            .or_default()
                            .insert(base.to_owned());
                    }
                    None => {
                        symbols
                            .statics
                            .entry(file.clone())
                            .or_default()
                            .insert(name.to_owned());
                    }
                }
            } else if clone_base(name).is_none() {
                symbols.exported.insert(name.to_owned());
            }
        }
        Ok(symbols)
    }

    /// The canonical name of the function `hook` names, as a hook record
    /// writes it: `name` or `file.c:name`.
    pub fn resolve(&self, hook: &str) -> Result<String, SymbolError> {
        let check_static = |file: &str, name: &str| {
            if self
                .cloned
                .get(file)
                .is_some_and(|names| names.contains(name))
            {
                Err(SymbolError::Cloned(hook.to_owned()))
            } else {
                Ok(format!("{file}:{name}"))
            }
        };
        if let Some((file, name)) = hook.split_once(':') {
            return match self.statics.get(file) {
                Some(names) if names.contains(name) => check_static(file, name),
                _ => Err(SymbolError::Unknown(hook.to_owned())),
            };
        }
        if self.exported.contains(hook) {
            return Ok(hook.to_owned());
        }
        let files: Vec<&str> = self
            .statics
            .iter()
            .filter(|(_, names)| names.contains(hook))
            .map(|(file, _)| file.as_str())
            .collect();
        match files.as_slice() {
            [] => Err(SymbolError::Unknown(hook.to_owned())),
            [file] => check_static(file, hook),
            files => Err(SymbolError::Ambiguous(hook.to_owned(), files.join(", "))),
        }
    }

    /// `hooks` with every name canonical, or every name that doesn't resolve.
    pub fn canonical_hooks(&self, hooks: &Hooks) -> Result<Hooks, Vec<SymbolError>> {
        let mut errors = Vec::new();
        let mut list = |names: &[String]| {
            let mut out: Vec<String> = names
                .iter()
                .filter_map(|name| self.resolve(name).map_err(|e| errors.push(e)).ok())
                .collect();
            out.sort();
            out.dedup();
            out
        };
        let canonical = Hooks {
            before: list(&hooks.before),
            after: list(&hooks.after),
            replaces: list(&hooks.replaces),
        };
        if errors.is_empty() {
            Ok(canonical)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_resolve_to_one_name_and_unsafe_ones_are_refused() {
        let mut symbols = Symbols::default();
        symbols.exported.insert("ftCo_Landing_IASA".into());
        for (file, name) in [
            ("a.c", "shared"),
            ("b.c", "shared"),
            ("a.c", "only_a"),
            ("c.c", "copied"),
        ] {
            symbols
                .statics
                .entry(file.into())
                .or_default()
                .insert(name.into());
        }
        symbols
            .cloned
            .entry("c.c".into())
            .or_default()
            .insert("copied".into());

        assert_eq!(
            symbols.resolve("ftCo_Landing_IASA").as_deref(),
            Ok("ftCo_Landing_IASA")
        );
        // A plain name and its file.c: form are one function.
        assert_eq!(symbols.resolve("only_a").as_deref(), Ok("a.c:only_a"));
        assert_eq!(symbols.resolve("a.c:only_a").as_deref(), Ok("a.c:only_a"));
        assert_eq!(symbols.resolve("b.c:shared").as_deref(), Ok("b.c:shared"));
        assert!(matches!(
            symbols.resolve("shared"),
            Err(SymbolError::Ambiguous(..))
        ));
        assert!(matches!(
            symbols.resolve("c.c:copied"),
            Err(SymbolError::Cloned(_))
        ));
        assert!(matches!(
            symbols.resolve("nope"),
            Err(SymbolError::Unknown(_))
        ));
    }
}
