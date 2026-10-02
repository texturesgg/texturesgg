//! Preferences kept between sessions: the palette, the sidebar, the
//! editor's panes, and the costumes edited lately, in
//! `textures.gg/editor.json` under the platform's config directory.
//!
//! Settings are a convenience: a missing or unreadable file falls back to the
//! defaults, and a failed write is logged, never shown as an error.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tgg_ui::Appearance;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    /// `gallery` or `paper`; unknown names fall back to Gallery.
    palette: Option<String>,
    /// The editor's pane column width in web pixels; it clamps to its range.
    pub(crate) inspector_width: Option<f32>,
    /// Which of the editor's panes are open, and which folded.
    pub(crate) panes: Option<PaneLayout>,
    /// Tint the selected texture's surfaces in the viewport (default on).
    pub(crate) highlight_selection: Option<bool>,
    /// The program Edit in Image Editor opens PNGs with (for example
    /// `aseprite`); the system's default app for PNGs when unset.
    pub(crate) external_editor: Option<String>,
    /// The player's Melee NTSC 1.02 ISO, chosen or found before.
    pub(crate) iso_path: Option<PathBuf>,
    /// Folders the player keeps Melee ISOs in, beyond the ones Slippi uses.
    pub(crate) game_folders: Vec<PathBuf>,
    /// The sidebar shows beside the player's places (default: shown).
    pub(crate) sidebar: Option<bool>,
    /// The game's costume files edited lately, the latest first.
    pub(crate) recent: Vec<String>,
    /// Look for a newer version at launch (default on).
    pub(crate) check_for_updates: Option<bool>,
}

/// One of the editor's panes: open, and folded to its header.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct PaneState {
    pub(crate) open: bool,
    pub(crate) folded: bool,
}

/// The editor's panes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PaneLayout {
    pub(crate) textures: PaneState,
    pub(crate) texture: PaneState,
    pub(crate) moves: PaneState,
    pub(crate) colors: PaneState,
}

impl Default for PaneLayout {
    /// The textures and the selected one open; moves and colors a click
    /// away.
    fn default() -> Self {
        let open = PaneState {
            open: true,
            folded: false,
        };
        Self {
            textures: open,
            texture: open,
            moves: PaneState::default(),
            colors: PaneState::default(),
        }
    }
}

/// How many recent edits the sidebar lists.
const RECENT: usize = 5;

impl Settings {
    /// Put `file` first among the recent edits.
    pub(crate) fn remember_edit(&mut self, file: &str) {
        self.recent.retain(|recent| recent != file);
        self.recent.insert(0, file.to_owned());
        self.recent.truncate(RECENT);
    }
}

impl Settings {
    pub(crate) fn appearance(&self) -> Appearance {
        match self.palette.as_deref() {
            Some("paper") => Appearance::Paper,
            _ => Appearance::Gallery,
        }
    }

    pub(crate) fn set_appearance(&mut self, appearance: Appearance) {
        self.palette = Some(
            match appearance {
                Appearance::Gallery => "gallery",
                Appearance::Paper => "paper",
            }
            .into(),
        );
    }

    /// Take the settings that differ between `before` and `after`, leaving
    /// the rest as they are. A holder of an older copy saves its own
    /// changes this way without undoing anyone else's.
    pub(crate) fn apply_changes(&mut self, before: &Self, after: &Self) {
        // Destructured so a new setting cannot be forgotten here.
        let Self {
            palette,
            inspector_width,
            panes,
            highlight_selection,
            external_editor,
            iso_path,
            game_folders,
            sidebar,
            recent,
            check_for_updates,
        } = after;
        fn take<T: Clone + PartialEq>(target: &mut T, before: &T, after: &T) {
            if before != after {
                *target = after.clone();
            }
        }
        take(&mut self.palette, &before.palette, palette);
        take(
            &mut self.inspector_width,
            &before.inspector_width,
            inspector_width,
        );
        take(&mut self.panes, &before.panes, panes);
        take(
            &mut self.highlight_selection,
            &before.highlight_selection,
            highlight_selection,
        );
        take(
            &mut self.external_editor,
            &before.external_editor,
            external_editor,
        );
        take(&mut self.iso_path, &before.iso_path, iso_path);
        take(&mut self.game_folders, &before.game_folders, game_folders);
        take(&mut self.sidebar, &before.sidebar, sidebar);
        take(&mut self.recent, &before.recent, recent);
        take(
            &mut self.check_for_updates,
            &before.check_for_updates,
            check_for_updates,
        );
    }

    fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_else(|error| {
            crate::log(&format!("ignoring unreadable settings: {error}"));
            Self::default()
        })
    }

    fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }
}

fn path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("textures.gg").join("editor.json"))
}

pub(crate) fn load() -> Settings {
    path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map_or_else(Settings::default, |text| Settings::parse(&text))
}

/// Change the saved settings: read what is on disk now, change it, write
/// it back. Every writer goes through here, so one never writes back an
/// older copy of a setting another has changed since.
pub(crate) fn update(change: impl FnOnce(&mut Settings)) -> Settings {
    let mut settings = load();
    change(&mut settings);
    save(&settings);
    settings
}

pub(crate) fn save(settings: &Settings) {
    let Some(path) = path() else {
        return;
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| crate::disk::write_atomically(&path, settings.to_json().as_bytes()));
    if let Err(error) = written {
        crate::log(&format!(
            "couldn't save settings to {}: {error}",
            path.display()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;
    use tgg_ui::Appearance;

    #[test]
    fn settings_round_trip_and_tolerate_bad_input() {
        let mut settings = Settings {
            inspector_width: Some(320.0),
            ..Settings::default()
        };
        settings.set_appearance(Appearance::Paper);
        let reread = Settings::parse(&settings.to_json());
        assert_eq!(reread, settings);
        assert_eq!(reread.appearance(), Appearance::Paper);

        // Missing fields default; unknown fields and palettes are ignored.
        let partial = Settings::parse(r#"{"palette":"neon","future":1}"#);
        assert_eq!(partial.appearance(), Appearance::Gallery);
        assert_eq!(partial.inspector_width, None);
        assert_eq!(Settings::parse("not json"), Settings::default());
    }

    #[test]
    fn an_older_copy_saves_its_changes_without_undoing_newer_ones() {
        // The editor opens holding the settings as they were...
        let opened = Settings {
            sidebar: Some(true),
            ..Settings::default()
        };
        // ...then the shell records the edit and hides the sidebar...
        let mut on_disk = opened.clone();
        on_disk.remember_edit("PlFxOr.dat");
        on_disk.sidebar = Some(false);
        // ...and the editor, later, changes only its highlight.
        let mut editors = opened.clone();
        editors.highlight_selection = Some(false);

        on_disk.apply_changes(&opened, &editors);
        assert_eq!(on_disk.highlight_selection, Some(false));
        assert_eq!(on_disk.recent, ["PlFxOr.dat"]);
        assert_eq!(on_disk.sidebar, Some(false));
    }

    #[test]
    fn recent_edits_put_the_latest_first_once_and_keep_five() {
        let mut settings = Settings::default();
        for file in ["PlFcRe.dat", "PlFxOr.dat", "PlFcRe.dat"] {
            settings.remember_edit(file);
        }
        assert_eq!(settings.recent, ["PlFcRe.dat", "PlFxOr.dat"]);
        for index in 0..6 {
            settings.remember_edit(&format!("PlMr{index:02}.dat"));
        }
        assert_eq!(settings.recent.len(), 5);
        assert_eq!(settings.recent[0], "PlMr05.dat");
    }
}
