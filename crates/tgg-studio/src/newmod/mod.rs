//! Start a mod: the five things `tgg mod new` asks about (a rule,
//! something on screen, a menu or screen, an online mode, native C), a
//! name, and Create. The mod is made from the installed tgg-melee's SDK in
//! a folder of its own, and the sheet then says where it is, the command
//! that runs it, and how to open it and read the guide. The app never
//! builds or publishes a mod: that's the `tgg` command and textures.gg.
//! `actions` changes it; `view` draws the sheet; `library` is what it asks
//! tgg-mod.

pub mod actions;
pub mod library;
pub mod view;

use library::{CreatedMod, Progress, Sdk, Tools};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

/// What a mod is for, and so the SDK example it starts from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A script that changes how the game plays.
    #[default]
    Rule,
    /// A script that shows something on screen: a counter, a trainer.
    OnScreen,
    /// A script that adds a menu entry and its screen.
    Menu,
    /// A script mode that plays online.
    OnlineMode,
    /// A C mod, for what scripts can't reach.
    Native,
}

impl Kind {
    /// Every kind, in the order the sheet shows them.
    pub const ALL: [Kind; 5] = [
        Kind::Rule,
        Kind::OnScreen,
        Kind::Menu,
        Kind::OnlineMode,
        Kind::Native,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Rule => "A rule",
            Kind::OnScreen => "Something on screen",
            Kind::Menu => "A menu or screen",
            Kind::OnlineMode => "An online mode",
            Kind::Native => "Native C",
        }
    }

    /// The line under its name.
    pub fn line(self) -> &'static str {
        match self {
            Kind::Rule => "Change how the game plays",
            Kind::OnScreen => "Counters, trainers, overlays",
            Kind::Menu => "Add your own place in the menus",
            Kind::OnlineMode => "Your own way to play with friends",
            Kind::Native => "For what scripts can't do",
        }
    }

    /// The SDK example it starts from.
    pub fn template(self) -> &'static str {
        match self {
            Kind::Rule => tgg_mod::SCRIPT_TEMPLATES[0],
            Kind::OnScreen => tgg_mod::SCRIPT_TEMPLATES[1],
            Kind::Menu => tgg_mod::SCRIPT_TEMPLATES[2],
            Kind::OnlineMode => tgg_mod::SCRIPT_TEMPLATES[3],
            Kind::Native => tgg_mod::TEMPLATE,
        }
    }
}

/// The Start a mod sheet's state.
#[derive(Default)]
pub struct NewMod {
    /// The sheet shows.
    pub open: bool,
    /// What the player's making.
    pub kind: Kind,
    /// Its name, as the player types it.
    pub name: rune::TextEdit,
    /// The folder the mod's own folder (named by its id) goes in.
    pub parent: PathBuf,
    /// The installed tgg-melee's SDK, once it's here, or why it couldn't be
    /// had.
    pub sdk: Option<Result<Sdk, String>>,
    /// The SDK on its way (the first time): how far, and stopping it.
    pub fetching: Option<Fetching>,
    /// Which of the tools that build mods are installed, once looked for.
    pub tools: Option<Tools>,
    pub stage: Stage,
}

#[derive(Default)]
pub enum Stage {
    /// The player is filling it in.
    #[default]
    Editing,
    Creating,
    Created(CreatedMod),
    Failed(String),
}

/// The SDK being downloaded.
#[derive(Clone, Default)]
pub struct Fetching {
    pub progress: Arc<Mutex<Option<Progress>>>,
    pub cancel: Arc<AtomicBool>,
}

impl Fetching {
    /// How far along, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        let progress = self.progress.lock().ok().and_then(|latest| latest.clone());
        progress
            .and_then(|progress| {
                let total = progress.total.filter(|total| *total > 0)?;
                Some((progress.bytes as f32 / total as f32).clamp(0.0, 1.0))
            })
            .unwrap_or(0.0)
    }
}

impl NewMod {
    /// The id the mod gets, from its name.
    pub fn id_text(&self) -> String {
        id_from(&self.name.text)
    }

    /// The id, when the name makes a valid one.
    pub fn mod_id(&self) -> Option<tgg_mod::ModId> {
        self.id_text().parse().ok()
    }

    /// What players see it called: the name typed, else its id.
    pub fn title(&self) -> String {
        match self.name.text.trim() {
            "" => self.id_text(),
            name => name.to_owned(),
        }
    }

    /// The folder the mod is made in.
    pub fn folder(&self) -> PathBuf {
        self.parent.join(self.id_text())
    }

    /// Whether Create can be pressed: the SDK here, a name that makes a
    /// valid id, and nothing being made.
    pub fn ready(&self) -> bool {
        matches!(self.stage, Stage::Editing | Stage::Failed(_))
            && matches!(self.sdk, Some(Ok(_)))
            && self.mod_id().is_some()
    }
}

/// An id from a name: lowercase, spaces and other marks as `-`, only the
/// characters ids allow, at most 64 of them.
pub fn id_from(name: &str) -> String {
    let mut id = String::new();
    for character in name.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_') {
            id.push(character);
        } else if !id.ends_with('-') && !id.is_empty() {
            id.push('-');
        }
    }
    id.trim_end_matches('-').chars().take(64).collect()
}

/// Where new mods go unless the player picked a folder: `mods` in their
/// home folder.
pub fn default_parent() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mods")
}

#[cfg(test)]
mod tests {
    use super::id_from;

    #[test]
    fn an_id_comes_from_the_name() {
        assert_eq!(id_from("Neutral Spawns"), "neutral-spawns");
        assert_eq!(id_from("  Fox's  Shine!! "), "fox-s-shine");
        assert_eq!(id_from("ucf 0.84"), "ucf-0.84");
        assert_eq!(id_from("!!!"), "");
        assert_eq!(id_from(&"a".repeat(80)).len(), 64);
    }
}
