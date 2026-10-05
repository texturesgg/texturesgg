//! The game's functions and variables a mod can name, from the SDK's
//! `symbols.txt`, and each hook's canonical name.
//!
//! A build checks every hook and game symbol a mod names against them, so a
//! hook on a function the game doesn't have, or on a static whose name several
//! files share, fails the build instead of being refused in a player's log.
//! They also give each hook one canonical name: an exported function by its
//! plain name, a static as `file.c:name`. The game treats `name` and
//! `file.c:name` as one function when they resolve to the same code, so
//! conflict checks compare canonical names.
//!
//! Each line of `symbols.txt` is `func` or `data`, the name a record uses
//! (`file.c:name` for a static), then flags: `copied` for a static the
//! compiler also copied for some of its callers (a hook would miss those
//! calls), `local` for a global the game doesn't export (a name libc
//! defines), which mods can't link against, and `runtime` for a function of
//! the mod runtime, which mods call and can't hook. `#` starts a comment line.

use crate::decls::Hooks;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Func,
    Data,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub kind: Kind,
    pub copied: bool,
    pub local: bool,
    pub runtime: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Symbols {
    /// Every symbol, by the name a record uses.
    entries: BTreeMap<String, Symbol>,
    /// The files whose statics have each plain name.
    statics: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("line {0} of the symbol list isn't \"func|data <name> [flags]\"")]
    Line(usize),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SymbolError {
    #[error("{0}: the game has nothing by that name")]
    Unknown(String),
    #[error("{0}: static in several files ({1}); name it as file.c:{0}")]
    Ambiguous(String, String),
    #[error("{0}: the compiler also copied this static, so calls to the copy would skip the hook")]
    Copied(String),
    #[error("{0} is a variable; hooks go on functions")]
    Variable(String),
    #[error("{0} belongs to the mod runtime; call it, don't hook it")]
    Runtime(String),
}

impl Symbols {
    /// Read the symbol list at `path`.
    pub fn open(path: &Path) -> Result<Self, ParseError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| ParseError::Io(path.display().to_string(), e))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut symbols = Self::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_ascii_whitespace();
            let kind = match words.next() {
                Some("func") => Kind::Func,
                Some("data") => Kind::Data,
                _ => return Err(ParseError::Line(index + 1)),
            };
            let name = words.next().ok_or(ParseError::Line(index + 1))?;
            let mut symbol = Symbol {
                kind,
                copied: false,
                local: false,
                runtime: false,
            };
            for flag in words {
                match flag {
                    "copied" => symbol.copied = true,
                    "local" => symbol.local = true,
                    "runtime" => symbol.runtime = true,
                    // A flag a newer game adds tells nothing this reader acts on.
                    _ => {}
                }
            }
            if let Some((file, plain)) = name.split_once(':') {
                symbols
                    .statics
                    .entry(plain.to_owned())
                    .or_default()
                    .push(file.to_owned());
            }
            symbols.entries.insert(name.to_owned(), symbol);
        }
        Ok(symbols)
    }

    /// The canonical name of what `name` names, and the symbol: a plain name
    /// means the symbol the game exports, or else the one static with that
    /// name.
    pub fn resolve(&self, name: &str) -> Result<(String, Symbol), SymbolError> {
        if let Some(symbol) = self.entries.get(name) {
            return Ok((name.to_owned(), *symbol));
        }
        if name.contains(':') {
            return Err(SymbolError::Unknown(name.to_owned()));
        }
        match self.statics.get(name).map(Vec::as_slice) {
            None | Some([]) => Err(SymbolError::Unknown(name.to_owned())),
            Some([file]) => {
                let canonical = format!("{file}:{name}");
                let symbol = self.entries[&canonical];
                Ok((canonical, symbol))
            }
            Some(files) => Err(SymbolError::Ambiguous(name.to_owned(), files.join(", "))),
        }
    }

    /// The canonical name of the function the hook `name` names, or why the
    /// game would refuse the hook.
    pub fn hook(&self, name: &str) -> Result<String, SymbolError> {
        let (canonical, symbol) = self.resolve(name)?;
        if symbol.kind != Kind::Func {
            Err(SymbolError::Variable(name.to_owned()))
        } else if symbol.runtime {
            Err(SymbolError::Runtime(name.to_owned()))
        } else if symbol.copied {
            Err(SymbolError::Copied(name.to_owned()))
        } else {
            Ok(canonical)
        }
    }

    /// Whether a mod's library may link against `name`: a symbol the game
    /// exports, not a static and not one libc defines.
    pub fn links(&self, name: &str) -> bool {
        !name.contains(':') && self.entries.get(name).is_some_and(|symbol| !symbol.local)
    }

    /// `hooks` with every name canonical, or every name the game would refuse.
    pub fn canonical_hooks(&self, hooks: &Hooks) -> Result<Hooks, Vec<SymbolError>> {
        let mut errors = Vec::new();
        let mut list = |names: &[String]| {
            let mut out: Vec<String> = names
                .iter()
                .filter_map(|name| self.hook(name).map_err(|e| errors.push(e)).ok())
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
    fn hooks_resolve_to_one_name_and_ones_the_game_refuses_fail() {
        let symbols = Symbols::parse(
            "# comment\n\
             func ftCo_Landing_IASA\n\
             func a.c:shared\n\
             func b.c:shared\n\
             func a.c:only_a\n\
             func c.c:copied copied\n\
             func both\n\
             func d.c:both\n\
             data Player_Count\n\
             func tgg_log_ runtime\n",
        )
        .expect("parse");
        assert_eq!(
            symbols.hook("ftCo_Landing_IASA").as_deref(),
            Ok("ftCo_Landing_IASA")
        );
        // A plain name and its file.c: form are one function.
        assert_eq!(symbols.hook("only_a").as_deref(), Ok("a.c:only_a"));
        assert_eq!(symbols.hook("a.c:only_a").as_deref(), Ok("a.c:only_a"));
        // A plain name that is both exported and static means the exported one.
        assert_eq!(symbols.hook("both").as_deref(), Ok("both"));
        assert!(matches!(
            symbols.hook("shared"),
            Err(SymbolError::Ambiguous(..))
        ));
        assert!(matches!(
            symbols.hook("copied"),
            Err(SymbolError::Copied(_))
        ));
        assert!(matches!(
            symbols.hook("Player_Count"),
            Err(SymbolError::Variable(_))
        ));
        assert!(matches!(
            symbols.hook("tgg_log_"),
            Err(SymbolError::Runtime(_))
        ));
        assert!(matches!(symbols.hook("nope"), Err(SymbolError::Unknown(_))));
    }
}
