//! Editing a texture in another app: where its PNG goes, starting the
//! player's image editor on it, and noticing each save there.
//!
//! The editor is the system's default app for PNGs, or the command in the
//! settings' `external_editor` (split into words as a shell would, with the
//! PNG's path appended). A save is noticed by its file's modification time;
//! one that doesn't decode yet (a write in progress) is retried, and given up
//! on if it stays unchanged and undecodable.
//!
//! The PNGs live in a folder of this process's under the temporary directory,
//! deleted with the document they belong to and, whole, when the app quits.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Checks a save may stay unchanged and undecodable before it's given up on.
pub const UNDECODABLE_CHECKS: u32 = 6;

/// Why the image editor's command couldn't run.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("the image editor command is empty")]
    Empty,
    #[error("the image editor command has an unclosed {0} quote")]
    UnclosedQuote(char),
    #[error("the image editor command ends in a lone backslash")]
    LoneBackslash,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A texture's PNG being edited in another app.
#[derive(Debug)]
pub struct Watched {
    pub path: PathBuf,
    /// The modification time last read (or written by the app).
    seen: Option<SystemTime>,
    /// A newer save that didn't decode, and how many checks in a row it has
    /// stayed that way.
    undecodable: Option<(SystemTime, u32)>,
}

impl Watched {
    /// Watch `path`, as it is now: only a later save is read.
    pub fn new(path: PathBuf) -> Self {
        Self {
            seen: modified(&path),
            path,
            undecodable: None,
        }
    }

    /// When it was saved, if that's since it was last read.
    pub fn saved(&self) -> Option<SystemTime> {
        modified(&self.path).filter(|now| Some(*now) != self.seen)
    }

    /// The save at `now` was read.
    pub fn read(&mut self, now: SystemTime) {
        self.seen = Some(now);
        self.undecodable = None;
    }

    /// The save at `now` didn't decode. Returns whether to give up on it: it
    /// stayed unchanged long enough that it isn't a write in progress. A
    /// save given up on counts as read, so only a newer one is tried.
    pub fn failed_to_decode(&mut self, now: SystemTime) -> bool {
        let checks = match self.undecodable {
            Some((at, checks)) if at == now => checks + 1,
            _ => 1,
        };
        if checks >= UNDECODABLE_CHECKS {
            self.read(now);
            true
        } else {
            self.undecodable = Some((now, checks));
            false
        }
    }
}

/// A new place for a PNG named `file_name` to be edited: a folder of its
/// own under this process's, so two textures of the same name never share
/// a file, and the name stays plain in the image editor's title.
pub fn new_png_path(file_name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    temporary_directory()
        .join(NEXT.fetch_add(1, Ordering::Relaxed).to_string())
        .join(file_name)
}

/// Stop editing the PNG at `path`: delete it and its folder.
pub fn remove_png(path: &Path) {
    std::fs::remove_file(path).ok();
    if let Some(folder) = path.parent() {
        std::fs::remove_dir(folder).ok();
    }
}

/// Where this process puts PNGs for editing.
pub fn temporary_directory() -> PathBuf {
    std::env::temp_dir()
        .join("textures.gg")
        .join(std::process::id().to_string())
}

/// Delete this process's PNGs, when the app quits.
pub fn remove_temporary_directory() {
    std::fs::remove_dir_all(temporary_directory()).ok();
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Open `path` in the player's image editor: `command` when they've set
/// one, else the system's default app for it.
pub fn open_in_editor(command: Option<&str>, path: &Path) -> Result<(), CommandError> {
    let mut process = match command {
        Some(command) => {
            let words = split_command(command)?;
            let mut process = std::process::Command::new(&words[0]);
            process.args(&words[1..]);
            process
        }
        None => system_opener(),
    };
    let mut child = process.arg(path).spawn()?;
    // Reap it when it exits, so it doesn't linger as a zombie.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The system's command that opens a file in its default app.
fn system_opener() -> std::process::Command {
    if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(target_os = "windows") {
        let mut start = std::process::Command::new("cmd");
        // `start`'s first quoted argument is a window title.
        start.args(["/C", "start", ""]);
        start
    } else {
        std::process::Command::new("xdg-open")
    }
}

/// Split a command line into words: whitespace separates them, single quotes
/// keep text literal, double quotes keep whitespace, and a backslash escapes
/// the next character outside single quotes. `flatpak run org.gimp.GIMP`
/// is three words.
pub fn split_command(command: &str) -> Result<Vec<String>, CommandError> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut characters = command.chars();
    while let Some(character) = characters.next() {
        match character {
            ' ' | '\t' | '\n' => words.extend(word.take()),
            '\'' => {
                let word = word.get_or_insert_default();
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(character) => word.push(character),
                        None => return Err(CommandError::UnclosedQuote('\'')),
                    }
                }
            }
            '"' => {
                let word = word.get_or_insert_default();
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some('\\') => match characters.next() {
                            Some(escaped @ ('"' | '\\')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(CommandError::UnclosedQuote('"')),
                        },
                        Some(character) => word.push(character),
                        None => return Err(CommandError::UnclosedQuote('"')),
                    }
                }
            }
            '\\' => match characters.next() {
                Some(escaped) => word.get_or_insert_default().push(escaped),
                None => return Err(CommandError::LoneBackslash),
            },
            character => word.get_or_insert_default().push(character),
        }
    }
    words.extend(word);
    if words.is_empty() {
        return Err(CommandError::Empty);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn words(command: &str) -> Vec<String> {
        split_command(command).unwrap()
    }

    #[test]
    fn a_command_splits_on_whitespace_except_inside_quotes_and_escapes() {
        assert_eq!(words("gimp"), ["gimp"]);
        assert_eq!(
            words("  flatpak run\torg.gimp.GIMP "),
            ["flatpak", "run", "org.gimp.GIMP"]
        );
        assert_eq!(
            words(r#""/opt/My Apps/krita" --nosplash"#),
            ["/opt/My Apps/krita", "--nosplash"]
        );
        assert_eq!(words("'/opt/it'\\''s/app' -x"), ["/opt/it's/app", "-x"]);
        assert_eq!(words(r"/opt/My\ Apps/app"), ["/opt/My Apps/app"]);
        assert_eq!(words(r#""say \"hi\" \n""#), [r#"say "hi" \n"#]);
        assert_eq!(words("app ''"), ["app", ""]);
    }

    #[test]
    fn broken_commands_are_errors() {
        assert!(split_command("").is_err());
        assert!(split_command("   ").is_err());
        assert!(split_command("'gimp").is_err());
        assert!(split_command("\"gimp").is_err());
        assert!(split_command("gimp\\").is_err());
    }

    #[test]
    fn an_unchanged_undecodable_save_is_given_up_on() {
        let mut watched = Watched::new(PathBuf::from("missing.png"));
        let first = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let second = first + Duration::from_secs(1);
        for _ in 1..UNDECODABLE_CHECKS {
            assert!(!watched.failed_to_decode(first));
        }
        // A newer save starts the count again.
        assert!(!watched.failed_to_decode(second));
        assert_eq!(watched.seen, None);
        for _ in 2..UNDECODABLE_CHECKS {
            assert!(!watched.failed_to_decode(second));
        }
        assert!(watched.failed_to_decode(second));
        assert_eq!(watched.seen, Some(second));
        assert_eq!(watched.undecodable, None);
    }

    #[test]
    fn only_a_save_after_watching_begins_is_noticed() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("texture.png");
        std::fs::write(&path, b"before").unwrap();
        let mut watched = Watched::new(path.clone());
        assert_eq!(watched.saved(), None);
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert_eq!(watched.saved(), Some(later));
        watched.read(later);
        assert_eq!(watched.saved(), None, "read, it's not noticed again");
    }
}
