//! `tgg mod fix`: bring a mod's source up to the SDK you have.
//!
//! The decomp renames things. Each sync's rename map (in the SDK's `renames/`)
//! says what became what, joined on what didn't change: a function's console
//! address, a field's record and console offset, an enum constant's value.
//! This builds the mod, reads what the compiler says, and rewrites exactly the
//! spots it points at:
//!
//! - a retired function or variable name, which still builds through the SDK's
//!   `tgg/game/aliases.h` with a warning ("X is now Y: tgg mod fix renames
//!   it"), renamed where the warning's "in expansion of macro" note points;
//! - a field the record no longer has ("has no member named"), renamed when a
//!   map names its record and old name;
//! - an undeclared name a map renamed (an enum constant, a function, a macro).
//!
//! Then it builds again, until a pass changes nothing. It only edits a spot
//! whose text is the old name, inside the mod's own folder, so it can't rename
//! an unrelated `x1990` in another struct or in the mod's own code. What's
//! left is reported with what the maps say about it (`changed`, `gone`).

use crate::build::{self, Options};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// How many build-and-rewrite passes before giving up
const MAX_PASSES: usize = 10;

#[derive(Deserialize, Default)]
struct MapFile {
    #[serde(default)]
    functions: Vec<Rename>,
    #[serde(default)]
    variables: Vec<Rename>,
    #[serde(default)]
    fields: Vec<FieldRename>,
    #[serde(default)]
    enum_constants: Vec<Rename>,
    #[serde(default)]
    macros: Vec<Rename>,
    #[serde(default)]
    changed: Vec<String>,
    #[serde(default)]
    gone: Vec<String>,
}

#[derive(Deserialize)]
struct Rename {
    old: String,
    new: String,
}

#[derive(Deserialize)]
struct FieldRename {
    record: String,
    old: String,
    new: String,
}

/// Every map the SDK carries, folded together and chained, so a name three
/// syncs old goes to the newest one
#[derive(Default)]
struct Renames {
    /// Functions, variables, enum constants and macros: old name to new
    names: BTreeMap<String, String>,
    /// Fields: (record, old) to new
    fields: BTreeMap<(String, String), String>,
    /// What a rename can't describe, for the report
    notes: Vec<String>,
}

impl Renames {
    fn load(sdk: &Path) -> Result<Self> {
        let mut renames = Renames::default();
        let dir = sdk.join("renames");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(renames);
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        // Named by the order of the syncs, oldest first
        paths.sort();
        for path in paths {
            let text =
                std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
            let map: MapFile =
                serde_json::from_str(&text).with_context(|| path.display().to_string())?;
            for r in map
                .functions
                .iter()
                .chain(&map.variables)
                .chain(&map.enum_constants)
                .chain(&map.macros)
            {
                for now in renames.names.values_mut() {
                    if *now == r.old {
                        now.clone_from(&r.new);
                    }
                }
                renames.names.insert(r.old.clone(), r.new.clone());
            }
            for f in &map.fields {
                for ((record, _), now) in renames.fields.iter_mut() {
                    if *record == f.record && *now == f.old {
                        now.clone_from(&f.new);
                    }
                }
                renames
                    .fields
                    .insert((f.record.clone(), f.old.clone()), f.new.clone());
            }
            renames.notes.extend(map.changed);
            renames.notes.extend(map.gone);
        }
        Ok(renames)
    }
}

/// One place to rewrite: `old` at a line and column becomes `new`
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Edit {
    path: PathBuf,
    line: usize,
    column: usize,
    old: String,
    new: String,
}

/// A diagnostic line: "path:line:column: kind: message"
struct Diagnostic<'a> {
    path: &'a str,
    line: usize,
    column: usize,
    kind: &'a str,
    message: &'a str,
}

fn parse(line: &str) -> Option<Diagnostic<'_>> {
    // The path can hold a colon (C:\ on Windows): find ":N:N: " from the left
    let bytes = line.as_bytes();
    let mut i = 0;
    while let Some(off) = line[i..].find(':') {
        let start = i + off;
        let rest = &line[start + 1..];
        let digits1 = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits1 > 0 && rest.as_bytes().get(digits1) == Some(&b':') {
            let rest2 = &rest[digits1 + 1..];
            let digits2 = rest2.bytes().take_while(u8::is_ascii_digit).count();
            if digits2 > 0 && rest2[digits2..].starts_with(": ") {
                let after = &rest2[digits2 + 2..];
                let (kind, message) = after.split_once(": ")?;
                if !matches!(kind, "warning" | "error" | "note" | "fatal error") {
                    return None;
                }
                return Some(Diagnostic {
                    path: &line[..start],
                    line: rest[..digits1].parse().ok()?,
                    column: rest2[..digits2].parse().ok()?,
                    kind,
                    message,
                });
            }
        }
        i = start + 1;
        if i >= bytes.len() {
            break;
        }
    }
    None
}

/// The text between each pair of single quotes in a message
fn quoted(message: &str) -> Vec<&str> {
    message.split('\'').skip(1).step_by(2).collect()
}

/// The edits a build's output asks for, and its errors that none answers
fn find_edits(output: &str, renames: &Renames, root: &Path) -> (BTreeSet<Edit>, Vec<String>) {
    let mut edits = BTreeSet::new();
    let mut left = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let ours = |path: &str| -> Option<PathBuf> {
        let p = std::path::absolute(Path::new(path)).ok()?;
        p.starts_with(root).then_some(p)
    };
    for (i, line) in lines.iter().enumerate() {
        let Some(d) = parse(line) else { continue };
        // A retired name: the warning sits in aliases.h, and the note after it
        // says where the mod spelled the name
        if d.kind == "warning" {
            if let Some((old, rest)) = d.message.split_once(" is now ")
                && let Some((new, _)) = rest.split_once(": tgg mod fix renames it")
            {
                for note in lines[i + 1..].iter().filter_map(|l| parse(l)) {
                    if note.kind != "note" {
                        break;
                    }
                    if note.message == format!("in expansion of macro '{old}'")
                        && let Some(path) = ours(note.path)
                    {
                        edits.insert(Edit {
                            path,
                            line: note.line,
                            column: note.column,
                            old: old.to_owned(),
                            new: new.to_owned(),
                        });
                    }
                }
            }
            continue;
        }
        if d.kind != "error" {
            continue;
        }
        let Some(path) = ours(d.path) else { continue };
        let q = quoted(d.message);
        let edit = if d.message.contains("has no member named") {
            // "'Fighter' {aka 'struct Fighter'} has no member named 'x1990'; did
            // you mean 'x1991'?": the member is the quote after "named", the
            // types the quotes before it
            let (before, after) = d
                .message
                .split_once("has no member named")
                .unwrap_or_default();
            let member = quoted(after).first().copied().unwrap_or_default();
            let types = quoted(before);
            let found = types.iter().find_map(|ty| {
                let ty = ty
                    .trim_start_matches("struct ")
                    .trim_start_matches("union ");
                renames.fields.get(&(ty.to_owned(), member.to_owned()))
            });
            found.map(|new| (member, new))
        } else if d.message.contains("undeclared")
            || d.message.contains("implicit declaration of function")
        {
            let name = q.first().copied().unwrap_or_default();
            renames.names.get(name).map(|new| (name, new))
        } else {
            None
        };
        match edit {
            Some((old, new)) => {
                edits.insert(Edit {
                    path,
                    line: d.line,
                    column: d.column,
                    old: old.to_owned(),
                    new: new.clone(),
                });
            }
            None => left.push(line.to_string()),
        }
    }
    (edits, left)
}

/// Rewrite each edit's spot: the old name at its column, else the one
/// occurrence of it on its line. Returns the edits made.
fn apply(edits: &BTreeSet<Edit>) -> Result<Vec<Edit>> {
    let mut done = Vec::new();
    let mut by_file: BTreeMap<&Path, Vec<&Edit>> = BTreeMap::new();
    for e in edits {
        by_file.entry(&e.path).or_default().push(e);
    }
    for (path, mut file_edits) in by_file {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        let mut lines: Vec<String> = text.split_inclusive('\n').map(str::to_owned).collect();
        // Right to left on a line, so earlier columns stay put
        file_edits.sort_by_key(|e| std::cmp::Reverse((e.line, e.column)));
        for e in file_edits {
            let Some(line) = lines.get_mut(e.line.wrapping_sub(1)) else {
                continue;
            };
            let word = |at: usize| {
                line.get(at..at + e.old.len()) == Some(e.old.as_str())
                    && !line[..at].chars().next_back().is_some_and(is_ident)
                    && !line[at + e.old.len()..]
                        .chars()
                        .next()
                        .is_some_and(is_ident)
            };
            let at = if word(e.column.saturating_sub(1)) {
                Some(e.column - 1)
            } else {
                let spots: Vec<usize> = line
                    .match_indices(e.old.as_str())
                    .map(|(i, _)| i)
                    .filter(|&i| word(i))
                    .collect();
                (spots.len() == 1).then(|| spots[0])
            };
            if let Some(at) = at {
                line.replace_range(at..at + e.old.len(), &e.new);
                done.push(e.clone());
            }
        }
        std::fs::write(path, lines.concat()).with_context(|| path.display().to_string())?;
    }
    Ok(done)
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

pub fn fix(dir: &Path, options: &Options, check: bool) -> Result<()> {
    let root = std::path::absolute(dir)?;
    let mut renames: Option<Renames> = None;
    let mut made = Vec::new();
    for pass in 0..MAX_PASSES {
        let Some(compiled) = build::compile(dir, options)? else {
            println!("{}: no C sources; nothing to fix", dir.display());
            return Ok(());
        };
        let renames = match &renames {
            Some(r) => r,
            None => renames.insert(Renames::load(&compiled.sdk.root)?),
        };
        let (edits, left) = find_edits(&compiled.output, renames, &root);
        if edits.is_empty() || check {
            for e in &edits {
                println!(
                    "would rename {} to {} at {}:{}",
                    e.old,
                    e.new,
                    rel(&e.path, &root),
                    e.line
                );
            }
            report(&made, &left, &compiled, renames, &root);
            return Ok(());
        }
        let done = apply(&edits)?;
        if done.is_empty() {
            report(&made, &left, &compiled, renames, &root);
            return Ok(());
        }
        made.extend(done);
        if pass + 1 == MAX_PASSES {
            eprintln!("tgg mod fix: still changing after {MAX_PASSES} passes; stopping");
        }
    }
    Ok(())
}

fn rel(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn report(
    made: &[Edit],
    left: &[String],
    compiled: &build::Compiled,
    renames: &Renames,
    root: &Path,
) {
    for e in made {
        println!(
            "renamed {} to {} at {}:{}",
            e.old,
            e.new,
            rel(&e.path, root),
            e.line
        );
    }
    let warnings = compiled
        .output
        .lines()
        .filter(|l| l.contains("tgg mod fix renames it"))
        .count();
    if compiled.ok && left.is_empty() && warnings == 0 {
        println!("it builds, with no retired names");
        return;
    }
    for line in left {
        println!("left: {line}");
        // What the maps say about the names the error quotes
        for name in quoted(line) {
            for note in renames
                .notes
                .iter()
                .filter(|n| n.split_whitespace().any(|w| w.contains(name)))
            {
                println!("  the rename map says: {note}");
            }
        }
    }
    if !compiled.ok && left.is_empty() {
        println!("the build still fails; see its output with tgg mod build");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GCC's own words from a build against a sync that renamed a function,
    /// a field and an enum constant (C locale)
    const OUTPUT: &str = "\
/sdk/include/tgg/game/aliases.h:4:33: warning: gm_GetCurrentGameMode is now gm_GetGameMode: tgg mod fix renames it
    4 | #define gm_GetCurrentGameMode _Pragma(\"GCC warning ...\") gm_GetGameMode
/mod/src/mod.c:171:9: note: in expansion of macro 'gm_GetCurrentGameMode'
/mod/src/mod.c:159:28: error: 'Fighter' has no member named 'x1968_jumpsUsed'; did you mean 'x1969_walljumpUsed'?
/mod/src/mod.c:84:40: error: 'Ft_Kind_Popo' undeclared (first use in this function)
/mod/src/mod.c:90:5: error: 'Fighter' has no member named 'unknown_field'
";

    #[test]
    fn reads_what_the_compiler_points_at() {
        let mut renames = Renames::default();
        renames
            .names
            .insert("Ft_Kind_Popo".into(), "Ft_Kind_IceClimber".into());
        renames.fields.insert(
            ("Fighter".into(), "x1968_jumpsUsed".into()),
            "jumps_used".into(),
        );
        let (edits, left) = find_edits(OUTPUT, &renames, Path::new("/mod"));
        let got: Vec<_> = edits
            .iter()
            .map(|e| (e.line, e.old.as_str(), e.new.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (84, "Ft_Kind_Popo", "Ft_Kind_IceClimber"),
                (159, "x1968_jumpsUsed", "jumps_used"),
                (171, "gm_GetCurrentGameMode", "gm_GetGameMode"),
            ]
        );
        // A field no map knows is left for a person, not guessed at
        assert_eq!(left.len(), 1);
        assert!(left[0].contains("unknown_field"));
    }
}
