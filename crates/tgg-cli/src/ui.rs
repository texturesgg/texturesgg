//! How tgg looks in a terminal: pickers, confirmations and progress bars
//! (cliclack, indicatif), and color for versions, paths and commands
//! (console). Only in an interactive terminal; piped, in CI, or with
//! `NO_COLOR` or `TERM=dumb`, output stays plain text and every choice
//! comes from a flag or an argument.

use console::{Style, style};
use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

/// Whether tgg talks to a person at a terminal.
pub fn interactive() -> bool {
    static INTERACTIVE: OnceLock<bool> = OnceLock::new();
    *INTERACTIVE.get_or_init(|| {
        let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
        std::io::stdin().is_terminal()
            && std::io::stdout().is_terminal()
            && !set("CI")
            && !set("NO_COLOR")
            && std::env::var("TERM").map_or(true, |term| term != "dumb")
    })
}

/// Color on only when interactive.
pub fn init() {
    let on = interactive();
    console::set_colors_enabled(on);
    console::set_colors_enabled_stderr(on);
}

/// A command to run, such as `tgg port install`.
pub fn cmd(text: impl Display) -> String {
    style(text).cyan().bold().to_string()
}

/// A version, such as `0.5.0`.
pub fn ver(text: impl Display) -> String {
    style(text).bold().to_string()
}

/// A path.
pub fn path(text: impl Display) -> String {
    style(text).underlined().to_string()
}

/// Explanation, less important than what's beside it.
pub fn dim(text: impl Display) -> String {
    style(text).dim().to_string()
}

pub fn green(text: impl Display) -> String {
    style(text).green().to_string()
}

pub fn yellow(text: impl Display) -> String {
    style(text).yellow().to_string()
}

pub fn red(text: impl Display) -> String {
    style(text).red().to_string()
}

/// A section heading.
pub fn heading(text: impl Display) -> String {
    Style::new().bold().apply_to(text).to_string()
}

/// Ask yes or no, yes by default. Escape or Ctrl-C answers no.
pub fn confirm(question: impl Display) -> anyhow::Result<bool> {
    match cliclack::confirm(question).initial_value(true).interact() {
        Ok(answer) => Ok(answer),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Pick one of `items` (value, label, hint); `None` when the person backs out.
pub fn pick<T: Clone + Eq>(
    prompt: impl Display,
    items: Vec<(T, String, String)>,
    initial: Option<T>,
) -> anyhow::Result<Option<T>> {
    let mut select = cliclack::select(prompt);
    for (value, label, hint) in items {
        select = select.item(value, label, hint);
    }
    if let Some(initial) = initial {
        select = select.initial_value(initial);
    }
    match select.interact() {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Pick any of `items` (value, label, hint); none when the person backs out.
pub fn pick_many<T: Clone + Eq>(
    prompt: impl Display,
    items: Vec<(T, String, String)>,
) -> anyhow::Result<Vec<T>> {
    let mut select = cliclack::multiselect(prompt).required(false);
    for (value, label, hint) in items {
        select = select.item(value, label, hint);
    }
    match select.interact() {
        Ok(values) => Ok(values),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

/// A download's progress bar, in an interactive terminal.
pub fn bar(label: impl Display, total: u64) -> Option<indicatif::ProgressBar> {
    if !interactive() {
        return None;
    }
    let bar = indicatif::ProgressBar::new(total);
    bar.set_style(
        indicatif::ProgressStyle::with_template(
            "{msg} {wide_bar:.cyan/blue} {bytes}/{total_bytes} {eta}",
        )
        .expect("a valid template")
        .progress_chars("━━─"),
    );
    bar.set_message(label.to_string());
    Some(bar)
}

/// `count` of `one`, or of `many`.
pub fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The commands a fix starts with, to tell a fix from an explanation.
const FIX_STARTS: &[&str] = &[
    "tgg ",
    "winget ",
    "brew ",
    "sudo ",
    "apt ",
    "dnf ",
    "pacman ",
    "zypper ",
    "nix ",
    "nix-shell ",
    "xcode-select ",
    "git config ",
    "add eval ",
    "install ",
    "put ",
];

/// A doctor finding as its explanation and the fix in it, if it ends with
/// one: the text after the first `: ` or `; ` that starts a command.
pub fn split_fix(detail: &str) -> (&str, Option<&str>) {
    let mut at = 0;
    while let Some(found) = detail[at..].find([':', ';']) {
        let split = at + found;
        let rest = &detail[split + 1..];
        if let Some(fix) = rest.strip_prefix(' ')
            && FIX_STARTS.iter().any(|start| fix.starts_with(start))
        {
            return (&detail[..split], Some(fix));
        }
        at = split + 1;
    }
    (detail, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fix_is_split_from_its_explanation() {
        assert_eq!(
            split_fix("tgg-melee isn't installed: tgg port install"),
            ("tgg-melee isn't installed", Some("tgg port install"))
        );
        assert_eq!(
            split_fix("3.20; mods need 3.25 or later: sudo apt install cmake"),
            (
                "3.20; mods need 3.25 or later",
                Some("sudo apt install cmake")
            )
        );
        assert_eq!(
            split_fix("not found; only tgg mod dev --gdb needs it"),
            ("not found; only tgg mod dev --gdb needs it", None)
        );
        assert_eq!(split_fix("Linux on x86-64"), ("Linux on x86-64", None));
    }

    #[test]
    fn styles_are_plain_text_without_color() {
        console::set_colors_enabled(false);
        assert_eq!(cmd("tgg port run"), "tgg port run");
        assert_eq!(plural(1, "mod", "mods"), "1 mod");
        assert_eq!(plural(3, "mod", "mods"), "3 mods");
    }
}
