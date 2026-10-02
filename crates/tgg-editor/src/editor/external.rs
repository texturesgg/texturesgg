//! Edit a texture in another app: export it to a temporary PNG, open that in
//! the user's image editor, and re-import it every time it's saved there.
//!
//! The editor is the system's default app for PNGs, or the command in the
//! settings' `external_editor` (split into words like a shell would, with the
//! PNG's path appended). Saves are noticed by polling each watched file's
//! modification time; a save that doesn't decode yet (a partial write) is
//! retried on the next polls, and given up on if it stays unchanged and
//! undecodable. A re-import is an ordinary edit, with its fidelity notice and
//! an undo step. Undo and redo rewrite the watched PNG, so it keeps matching
//! the document.
//!
//! The PNGs live in a per-process temporary directory. They're deleted when
//! the document they belong to goes away, and the directory when the app
//! quits.

use crate::Error;
use crate::editor::Editor;
use gpui::{Context, Task};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often watched files are checked for saves.
const POLL: Duration = Duration::from_millis(500);

/// Polls a save may stay unchanged and undecodable before it's given up on.
const UNDECODABLE_POLLS: u32 = 6;

/// Textures being edited elsewhere, by document texture index.
#[derive(Default)]
pub(crate) struct ExternalEdits {
    watched: BTreeMap<usize, Watched>,
    poll: Option<Task<()>>,
}

struct Watched {
    path: PathBuf,
    /// The modification time last imported (or written by the export).
    seen: Option<SystemTime>,
    /// A newer save that didn't decode, and how many polls in a row it has
    /// stayed that way.
    undecodable: Option<(SystemTime, u32)>,
}

impl Watched {
    fn new(path: PathBuf) -> Self {
        Self {
            seen: modified(&path),
            path,
            undecodable: None,
        }
    }

    /// Note that the save at `now` didn't decode. Returns whether to give up
    /// on it: it stayed unchanged for long enough that it isn't a write in
    /// progress. A given-up save counts as seen, so only a newer one is read.
    fn failed_to_decode(&mut self, now: SystemTime) -> bool {
        let polls = match self.undecodable {
            Some((at, polls)) if at == now => polls + 1,
            _ => 1,
        };
        if polls >= UNDECODABLE_POLLS {
            self.seen = Some(now);
            self.undecodable = None;
            true
        } else {
            self.undecodable = Some((now, polls));
            false
        }
    }
}

impl ExternalEdits {
    /// Stop watching and delete the temporary PNGs, for a document that's
    /// going away.
    pub(crate) fn clear(&mut self) {
        for watched in std::mem::take(&mut self.watched).into_values() {
            std::fs::remove_file(&watched.path).ok();
        }
        // Dropping the task cancels the poll.
        self.poll = None;
    }
}

/// Where this process puts PNGs for editing.
fn temporary_directory() -> PathBuf {
    std::env::temp_dir()
        .join("textures.gg")
        .join(std::process::id().to_string())
}

/// Delete this process's temporary PNGs, when the app quits.
pub(crate) fn remove_temporary_directory() {
    std::fs::remove_dir_all(temporary_directory()).ok();
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Split a command line into words: whitespace separates them, single quotes
/// keep text literal, double quotes keep whitespace, and a backslash escapes
/// the next character outside single quotes. `flatpak run org.gimp.GIMP`
/// is three words.
fn split_command(command: &str) -> Result<Vec<String>, Error> {
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
                        None => return Err(Error::UnclosedQuote('\'')),
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
                            None => return Err(Error::UnclosedQuote('"')),
                        },
                        Some(character) => word.push(character),
                        None => return Err(Error::UnclosedQuote('"')),
                    }
                }
            }
            '\\' => match characters.next() {
                Some(escaped) => word.get_or_insert_default().push(escaped),
                None => return Err(Error::LoneBackslash),
            },
            character => word.get_or_insert_default().push(character),
        }
    }
    words.extend(word);
    if words.is_empty() {
        return Err(Error::EmptyCommand);
    }
    Ok(words)
}

/// Start `command` on `path`.
fn spawn_editor(command: &str, path: &Path) -> Result<(), Error> {
    let words = split_command(command)?;
    std::process::Command::new(&words[0])
        .args(&words[1..])
        .arg(path)
        .spawn()?;
    Ok(())
}

impl Editor {
    pub(crate) fn edit_externally(&mut self, cx: &mut Context<Self>) {
        let Some(texture) = self.selected() else {
            self.set_notice("Select a texture to edit in your image editor.", true, cx);
            return;
        };
        let path = temporary_directory().join(self.export_name(texture));
        let written = self.texture_png(texture).and_then(|png| {
            std::fs::create_dir_all(path.parent().expect("a temporary directory"))?;
            Ok(std::fs::write(&path, png)?)
        });
        if let Err(error) = written {
            self.set_notice(format!("Couldn't export for editing: {error}"), true, cx);
            return;
        }
        let name = self.texture_name(texture);
        match self.settings.external_editor.as_deref() {
            Some(command) => {
                if let Err(error) = spawn_editor(command, &path) {
                    self.set_notice(format!("Couldn't start {command}: {error}"), true, cx);
                    return;
                }
            }
            None => cx.open_with_system(&path),
        }
        self.external.watched.insert(texture, Watched::new(path));
        self.set_notice(
            format!("Editing {name} in your image editor; each save there updates it here."),
            false,
            cx,
        );
        if self.external.poll.is_none() {
            self.external.poll = Some(cx.spawn(async move |editor, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    if editor
                        .update(cx, |editor, cx| editor.poll_external(cx))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
    }

    /// Rewrite a watched texture's PNG after undo or redo changed it, so the
    /// file matches the document again, and remember the write so it isn't
    /// re-imported.
    pub(crate) fn resync_external(&mut self, texture: usize, cx: &mut Context<Self>) {
        let Some(path) = self
            .external
            .watched
            .get(&texture)
            .map(|watched| watched.path.clone())
        else {
            return;
        };
        let written = self
            .texture_png(texture)
            .and_then(|png| Ok(std::fs::write(&path, png)?));
        match written {
            Ok(()) => {
                if let Some(watched) = self.external.watched.get_mut(&texture) {
                    *watched = Watched::new(path);
                }
            }
            Err(error) => self.set_notice(
                format!("Couldn't update the PNG in your image editor: {error}"),
                true,
                cx,
            ),
        }
    }

    /// Re-import every watched file saved since it was last seen.
    fn poll_external(&mut self, cx: &mut Context<Self>) {
        let saved: Vec<(usize, PathBuf, SystemTime)> = self
            .external
            .watched
            .iter()
            .filter_map(|(&texture, watched)| {
                let now = modified(&watched.path)?;
                (Some(now) != watched.seen).then(|| (texture, watched.path.clone(), now))
            })
            .collect();
        for (texture, path, now) in saved {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let png = std::fs::read(&path).ok();
            // A save still being written doesn't decode; try again next
            // poll, unless it stays that way.
            let decodes = png
                .as_deref()
                .is_some_and(|png| image::load_from_memory(png).is_ok());
            let Some(watched) = self.external.watched.get_mut(&texture) else {
                continue;
            };
            let Some(png) = png.filter(|_| decodes) else {
                if watched.failed_to_decode(now) {
                    self.set_notice(
                        format!("Couldn't read {name} as a PNG; save it again to retry."),
                        true,
                        cx,
                    );
                }
                continue;
            };
            watched.seen = Some(now);
            watched.undecodable = None;
            self.import_bytes(texture, &name, &png, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for _ in 1..UNDECODABLE_POLLS {
            assert!(!watched.failed_to_decode(first));
        }
        // A newer save starts the count again.
        assert!(!watched.failed_to_decode(second));
        assert_eq!(watched.seen, None);
        for _ in 2..UNDECODABLE_POLLS {
            assert!(!watched.failed_to_decode(second));
        }
        assert!(watched.failed_to_decode(second));
        assert_eq!(watched.seen, Some(second));
        assert_eq!(watched.undecodable, None);
    }
}
