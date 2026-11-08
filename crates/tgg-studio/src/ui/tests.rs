//! The design system's own rule: screens use the tokens and components,
//! never raw lengths or colors.

use std::path::{Path, PathBuf};

/// Layout and style calls whose argument must be a token or a named
/// constant, not a number written in place.
const MEASURED: &[&str] = &[
    "gap", "px", "py", "pt", "pb", "pl", "pr", "mx", "my", "mt", "mb", "ml", "mr", "w", "h",
    "size", "radius", "border", "max_w", "max_h", "min_w", "min_h", "shadow", "absolute",
    "opacity",
];

/// Calls where zero means none (no least size, no offset), so a bare zero
/// is fine.
const ZERO_IS_NONE: &[&str] = &["min_w", "min_h", "absolute"];

/// Colors written in place, not taken from the palette.
const RAW_COLORS: &[&str] = &[
    "Color::BLACK",
    "Color::WHITE",
    "rgb(0x",
    "oklch(",
    "srgb_decode(",
];

/// Files that aren't screens: the design system itself, tests, and the
/// stage's GPU copy pass.
fn is_screen(path: &Path, src: &Path) -> bool {
    let relative = path.strip_prefix(src).expect("under src");
    let skipped = relative.starts_with("ui")
        || ["flows.rs", "iso_flows.rs", "stage/copy.rs"]
            .iter()
            .any(|skip| relative == Path::new(skip));
    !skipped && path.extension().is_some_and(|extension| extension == "rs")
}

fn files(folder: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).expect("read src").flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// A call's arguments: what follows its open parenthesis up to the one that
/// closes it, split at its own commas.
fn arguments(after_open: &str) -> Vec<&str> {
    let (mut depth, mut start, mut found) = (0, 0, Vec::new());
    for (at, character) in after_open.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => {
                found.push(&after_open[start..at]);
                return found;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                found.push(&after_open[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    found
}

/// Where `line` writes a measure or color in place, if it does: an argument
/// that's a bare number. Arithmetic on tokens is how a screen relates them,
/// and is fine.
fn offenses(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for method in MEASURED {
        let call = format!(".{method}(");
        for (at, _) in line.match_indices(&call) {
            for argument in arguments(&line[at + call.len()..]) {
                let Ok(value) = argument.trim().parse::<f32>() else {
                    continue;
                };
                if !(value == 0.0 && ZERO_IS_NONE.contains(method)) {
                    found.push(format!("{call}{})", argument.trim()));
                }
            }
        }
    }
    found.extend(
        RAW_COLORS
            .iter()
            .filter(|color| line.contains(**color))
            .map(|color| (*color).to_owned()),
    );
    found
}

#[test]
fn screens_use_the_tokens() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    files(&src, &mut found);
    let mut problems = Vec::new();
    for path in found.iter().filter(|path| is_screen(path, &src)) {
        let source = std::fs::read_to_string(path).expect("read a screen");
        for (number, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for offense in offenses(line) {
                problems.push(format!(
                    "{}:{}: {offense}",
                    path.strip_prefix(&src).expect("under src").display(),
                    number + 1
                ));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "screens wrote measures or colors in place; use ui::tokens or a named constant:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_rule_catches_what_it_should() {
    assert_eq!(offenses("row().gap(6.0).w(space::XS)"), [".gap(6.0)"]);
    assert_eq!(
        offenses("el().min_w(0.0).absolute(0.0, 0.0).grow(1.0)"),
        Vec::<String>::new()
    );
    assert_eq!(offenses("el().ml(-4.0).ml(-INSET)"), [".ml(-4.0)"]);
    assert_eq!(
        offenses(".absolute(0.0, (TRACK - LINE) / 2.0).absolute(0.0, 9.0)"),
        [".absolute(9.0)"]
    );
    assert_eq!(offenses(".size(26.0, size::ROW)"), [".size(26.0)"]);
    assert_eq!(
        offenses(".fill(rune::Color::BLACK.alpha(0.6))"),
        ["Color::BLACK"]
    );
    assert_eq!(
        offenses(".w(PLATE_WIDTH).h(size::ROW)"),
        Vec::<String>::new()
    );
}
