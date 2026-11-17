//! `tgg mod fix`: bring a mod's source up to the SDK you have.
//!
//! The decomp renames things, and so does the mod API at a major version. Each
//! rename map (in the SDK's `renames/`) says what became what: a sync's is
//! joined on what didn't change (a function's console address, a field's
//! record and console offset, an enum constant's value), the API's is written
//! by hand. For C this builds the mod, reads what the compiler says, and
//! rewrites exactly the spots it points at:
//!
//! - a retired function or variable name, which still builds through the SDK's
//!   `tgg/game/aliases.h` with a warning ("X is now Y: tgg mod fix renames
//!   it"), renamed where the warning's "in expansion of macro" note points;
//! - a field the record no longer has ("has no member named"), renamed when a
//!   map names its record and old name;
//! - an undeclared name a map renamed (an enum constant, a function, a macro,
//!   a type);
//! - an event a map renamed, where the mod subscribes to it (`TGG_ON`).
//!
//! A script mod's `.luau` files get the map's script renames wherever they
//! spell `tgg.<old>`, then the SDK's tgg-script checks what's left.
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
    types: Vec<Rename>,
    #[serde(default)]
    events: Vec<Rename>,
    /// Scripts' names under `tgg.`: "input.pad" to "input.controller"
    #[serde(default)]
    script: Vec<Rename>,
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
    /// Functions, variables, enum constants, macros and types: old name to new
    names: BTreeMap<String, String>,
    /// Events: old name to new
    events: BTreeMap<String, String>,
    /// Scripts' names under `tgg.`: old to new
    script: BTreeMap<String, String>,
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
            let names = map
                .functions
                .iter()
                .chain(&map.variables)
                .chain(&map.enum_constants)
                .chain(&map.macros)
                .chain(&map.types);
            chain(&mut renames.names, names);
            chain(&mut renames.events, &map.events);
            chain(&mut renames.script, &map.script);
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

/// Add a map's renames to what came before, so an older name goes to the newest
fn chain<'a>(to: &mut BTreeMap<String, String>, renames: impl IntoIterator<Item = &'a Rename>) {
    for r in renames {
        for now in to.values_mut() {
            if *now == r.old {
                now.clone_from(&r.new);
            }
        }
        to.insert(r.old.clone(), r.new.clone());
    }
}

/// The event a quoted `tgg_event_<event>_fn` names, when a map renamed it
fn renamed_event<'a>(name: &str, renames: &'a Renames) -> Option<(&'a str, &'a str)> {
    let event = name
        .strip_prefix("tgg_event_")
        .and_then(|n| n.strip_suffix("_fn"))?;
    renames
        .events
        .get_key_value(event)
        .map(|(old, new)| (old.as_str(), new.as_str()))
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
        let q = quoted(d.message);
        // A renamed event: the error can sit in tgg/events.h, with notes after it saying
        // where the mod's TGG_ON is
        if let Some((old, new)) = q.iter().find_map(|name| renamed_event(name, renames)) {
            let notes = lines[i + 1..]
                .iter()
                .map_while(|l| parse(l).filter(|n| n.kind == "note"));
            let mut found = false;
            for (p, line, column) in std::iter::once((d.path, d.line, d.column))
                .chain(notes.map(|n| (n.path, n.line, n.column)))
            {
                if let Some(path) = ours(p) {
                    found = true;
                    edits.insert(Edit {
                        path,
                        line,
                        column,
                        old: old.to_owned(),
                        new: new.to_owned(),
                    });
                }
            }
            if !found {
                left.push(line.to_string());
            }
            continue;
        }
        let Some(path) = ours(d.path) else { continue };
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
            || d.message.contains("unknown type name")
        {
            let name = q.first().copied().unwrap_or_default();
            renames.names.get(name).map(|new| (name, new))
        } else {
            None
        };
        match edit {
            Some((old, new)) => {
                // GCC says a function is undeclared once per function body, and the error
                // means the name is declared nowhere in the file: every use of it there is
                // the same call (line 0: the whole file)
                let whole = d.message.contains("implicit declaration of function");
                edits.insert(Edit {
                    path,
                    line: if whole { 0 } else { d.line },
                    column: if whole { 0 } else { d.column },
                    old: old.to_owned(),
                    new: new.clone(),
                });
            }
            // A header the mod includes in several sources fails in each
            None if left.contains(&line.to_string()) => {}
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
            if e.line == 0 {
                for (n, line) in lines.iter_mut().enumerate() {
                    let spots: Vec<usize> = line
                        .match_indices(e.old.as_str())
                        .map(|(i, _)| i)
                        .filter(|&i| {
                            !line[..i].chars().next_back().is_some_and(is_ident)
                                && !line[i + e.old.len()..].chars().next().is_some_and(is_ident)
                        })
                        .collect();
                    for &at in spots.iter().rev() {
                        line.replace_range(at..at + e.old.len(), &e.new);
                        done.push(Edit {
                            line: n + 1,
                            column: at + 1,
                            ..e.clone()
                        });
                    }
                }
                continue;
            }
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

/// The `.luau` files under `dir`, but not in its build folder
fn luau_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| dir.display().to_string())? {
        let path = entry?.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n != "build" && n != ".git")
            {
                luau_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|x| x == "luau") {
            out.push(path);
        }
    }
    Ok(())
}

/// A script's text with the map's script renames made where it spells `tgg.<old>`, and the
/// edits, by line
fn rename_script(text: &str, renames: &Renames, path: &Path) -> (String, Vec<Edit>) {
    let mut edits = Vec::new();
    let mut out = String::with_capacity(text.len());
    for (n, line) in text.split_inclusive('\n').enumerate() {
        let mut line = line.to_owned();
        let mut from = 0;
        while let Some(off) = line[from..].find("tgg.") {
            let at = from + off;
            let name_at = at + 4;
            let starts_word = !line[..at].chars().next_back().is_some_and(is_ident);
            // The longest dotted name here that a map renamed: "draw.image_stage_name" before
            // "draw.image_stage"
            let found = starts_word
                .then(|| {
                    renames
                        .script
                        .iter()
                        .filter(|(old, _)| {
                            line[name_at..].starts_with(old.as_str())
                                && !line[name_at + old.len()..]
                                    .chars()
                                    .next()
                                    .is_some_and(is_ident)
                        })
                        .max_by_key(|(old, _)| old.len())
                })
                .flatten();
            match found {
                Some((old, new)) => {
                    line.replace_range(name_at..name_at + old.len(), new);
                    edits.push(Edit {
                        path: path.to_owned(),
                        line: n + 1,
                        column: name_at + 1,
                        old: format!("tgg.{old}"),
                        new: format!("tgg.{new}"),
                    });
                    from = name_at + new.len();
                }
                None => from = name_at,
            }
        }
        out.push_str(&line);
    }
    (out, edits)
}

/// A script mod: the map's script renames in its `.luau` files, then tgg-script's check
fn fix_script(dir: &Path, options: &Options, check: bool) -> Result<()> {
    let root = std::path::absolute(dir)?;
    let sdk = build::resolve_sdk(options.sdk)?;
    let renames = Renames::load(&sdk.root)?;
    let mut files = Vec::new();
    luau_files(&root, &mut files)?;
    files.sort();
    for path in &files {
        let text = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
        let (new_text, edits) = rename_script(&text, &renames, path);
        let verb = if check { "would rename" } else { "renamed" };
        for e in &edits {
            println!(
                "{verb} {} to {} at {}:{}",
                e.old,
                e.new,
                rel(&e.path, &root),
                e.line
            );
        }
        if !check && !edits.is_empty() {
            std::fs::write(path, new_text).with_context(|| path.display().to_string())?;
        }
    }
    if check {
        return Ok(());
    }
    match build::build_script(dir, options.sdk) {
        Ok(_) => println!("it checks, with no old names"),
        Err(error) => {
            let text = format!("{error:#}");
            println!("left: {text}");
            for note in notes_about(&text, &renames) {
                println!("  the rename map says: {note}");
            }
        }
    }
    Ok(())
}

/// What the maps say about the names a message mentions. Each note starts with the name it's
/// about, after its kind for a sync's ("field Fighter.x1990 ...", "tgg_screen_at: use ...",
/// "tgg.content.stage_name takes ...")
fn notes_about<'a>(message: &str, renames: &'a Renames) -> Vec<&'a str> {
    let mentions = |name: &str| {
        message.match_indices(name).any(|(at, _)| {
            !message[..at].chars().next_back().is_some_and(is_ident)
                && !message[at + name.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_ident)
        })
    };
    renames
        .notes
        .iter()
        .filter(|note| {
            let mut text = note.as_str();
            for kind in [
                "a clone's ",
                "variable ",
                "function ",
                "field ",
                "record ",
                "macro ",
            ] {
                text = text.strip_prefix(kind).unwrap_or(text);
            }
            let name: String = text
                .chars()
                .take_while(|&c| is_ident(c) || c == '.')
                .collect();
            // A field's own name; a script's without tgg.
            let name = name.trim_start_matches("tgg.");
            let name = if note.starts_with("field ") {
                name.rsplit('.').next().unwrap_or(name)
            } else {
                name
            };
            name.len() > 3 && mentions(name)
        })
        .map(String::as_str)
        .collect()
}

pub fn fix(dir: &Path, options: &Options, check: bool) -> Result<()> {
    if tgg_mod::is_script(dir) {
        return fix_script(dir, options, check);
    }
    let root = std::path::absolute(dir)?;
    let mut renames: Option<Renames> = None;
    let mut made = Vec::new();
    for pass in 0..MAX_PASSES {
        let Some(compiled) = build::compile(dir, options)? else {
            println!("{}: no C sources, so nothing to fix", dir.display());
            return Ok(());
        };
        let renames = match &renames {
            Some(r) => r,
            None => renames.insert(Renames::load(&compiled.sdk.root)?),
        };
        let (edits, left) = find_edits(&compiled.output, renames, &root);
        if edits.is_empty() || check {
            for e in &edits {
                let at = rel(&e.path, &root);
                if e.line == 0 {
                    println!("would rename {} to {} everywhere in {at}", e.old, e.new);
                } else {
                    println!("would rename {} to {} at {at}:{}", e.old, e.new, e.line);
                }
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
            eprintln!("tgg mod fix: still changing after {MAX_PASSES} passes, so it stopped");
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
        for note in notes_about(line, renames) {
            println!("  the rename map says: {note}");
        }
    }
    if !compiled.ok && left.is_empty() {
        println!("the build still fails. tgg mod build shows its output");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mod's folder as `run` passes it, absolute: on Windows `/mod`
    /// alone has no drive, and the compiler's paths gain one.
    fn root() -> PathBuf {
        std::path::absolute("/mod").expect("an absolute path")
    }

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
        let (edits, left) = find_edits(OUTPUT, &renames, &root());
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

    #[test]
    fn renames_an_event_where_the_mod_subscribes() {
        let mut renames = Renames::default();
        renames
            .events
            .insert("state_loaded".into(), "snapshot_loaded".into());
        // GCC's words for TGG_ON(state_loaded, loaded) on line 6 (C locale)
        let output = "/mod/src/mod.c:6:1: error: unknown type name 'tgg_event_state_loaded_fn'; \
                      did you mean 'tgg_event_save_loaded_fn'?\n\
                      /mod/src/mod.c:6:1: error: expression in static assertion is not an integer\n";
        let (edits, left) = find_edits(output, &renames, &root());
        let got: Vec<_> = edits
            .iter()
            .map(|e| (e.line, e.old.as_str(), e.new.as_str()))
            .collect();
        assert_eq!(got, [(6, "state_loaded", "snapshot_loaded")]);
        assert_eq!(left.len(), 1, "the assertion that follows from it");
    }

    #[test]
    fn renames_what_a_script_spells_under_tgg() {
        let mut renames = Renames::default();
        let map = [
            ("draw.image_stage", "image.stage"),
            ("draw.image_stage_name", "image.stage_name"),
            ("input.pad", "input.controller"),
        ];
        for (old, new) in map {
            renames.script.insert(old.into(), new.into());
        }
        let text = "local a = tgg.draw.image_stage_name(s)\n\
                    local p = tgg.input.pad(1) or mytgg.input.pad(1)\n\
                    tgg.input.pads(2)\n";
        let (out, edits) = rename_script(text, &renames, Path::new("view.luau"));
        assert_eq!(
            out,
            "local a = tgg.image.stage_name(s)\n\
             local p = tgg.input.controller(1) or mytgg.input.pad(1)\n\
             tgg.input.pads(2)\n"
        );
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[1].line, 2);
    }
}
