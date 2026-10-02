//! The player's costumes: every fighter's costume slots in their game, read
//! from its file table; how a slot reads to a player; and what a page of
//! them can ask the app to do.

use gpui::SharedString;
use melee_dat::{Character, CostumeColor, MeleeSlot, Stage, parse_filename};
use std::path::PathBuf;

/// A fighter and the costume slots its game has for it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Fighter {
    /// Its two-letter code in file names (`Fc`).
    pub code: &'static str,
    pub name: &'static str,
    pub costumes: Vec<Costume>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Costume {
    /// The color players pick it by, such as "Red".
    pub name: &'static str,
    /// Its file in the game, such as `PlFcRe.dat`.
    pub file: String,
}

/// The fighters with costume files among `files`, in the roster's order,
/// each with its slots in color order.
pub(crate) fn roster<'a>(files: impl IntoIterator<Item = &'a str>) -> Vec<Fighter> {
    // Only the costume files themselves, not descriptive names.
    let mut found: Vec<(Character, CostumeColor)> = files
        .into_iter()
        .filter_map(|file| match MeleeSlot::from_file_name(file)? {
            MeleeSlot::Costume { character, color } => Some((character, color)),
            MeleeSlot::Stage(_) => None,
        })
        .collect();
    found.sort();
    Character::all()
        .filter_map(|character| {
            let costumes: Vec<Costume> = found
                .iter()
                .filter(|(owner, _)| *owner == character)
                .map(|&(character, color)| Costume {
                    name: color.name(),
                    file: MeleeSlot::Costume { character, color }.file_name(),
                })
                .collect();
            (!costumes.is_empty()).then_some(Fighter {
                code: character.code(),
                name: character.name(),
                costumes,
            })
        })
        .collect()
}

/// Whether two slots are costumes of the same fighter, or the same stage.
pub(crate) fn same_owner(slot: &str, other: &str) -> bool {
    match (parse_filename(slot), parse_filename(other)) {
        (Some(slot), Some(other)) => slot.same_owner(other),
        _ => false,
    }
}

/// A costume slot's color ("Orange"); a stage slot has none.
pub(crate) fn slot_color(slot: &str) -> Option<&'static str> {
    parse_filename(slot)?.color().map(CostumeColor::name)
}

/// A stage's name from its file, or the file when it is no stage's.
pub(crate) fn stage_name(file: &str) -> &str {
    match Stage::from_file_name(file) {
        Some(stage) => stage.name(),
        None => file,
    }
}

/// Where a skin goes, in the player's words: "Falco · Red", "Final
/// Destination", or "no slot" when its file doesn't say.
pub(crate) fn slot_label(slot: Option<&str>) -> String {
    match slot.and_then(parse_filename) {
        Some(MeleeSlot::Costume { character, color }) => {
            format!("{} · {}", character.name(), color.name())
        }
        Some(MeleeSlot::Stage(stage)) => stage.name().to_owned(),
        None => "no slot".into(),
    }
}

#[derive(Clone)]
pub(crate) enum CostumesEvent {
    /// Add these files (DATs, or zips of them) to the library.
    Add(Vec<PathBuf>),
    /// Install the library's skin `skin` into slot `slot`.
    Install { skin: String, slot: String },
    /// Put back what the slot held before its last install.
    Undo(String),
    /// Put the slot back to its vanilla file.
    Restore(String),
    /// Ask for skin files, then add them as `Add` does.
    Choose,
    /// Go to Settings, where the game changes.
    Settings,
}

/// A one-line report on the costume list, such as what adding files did.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Notice {
    pub text: SharedString,
    pub error: bool,
}

#[cfg(test)]
mod tests {
    use super::{roster, slot_label};

    #[test]
    fn slots_share_an_owner_by_fighter_or_by_stage() {
        use super::{same_owner, slot_color};
        assert!(same_owner("PlFxOr.dat", "PlFxNr.dat"));
        assert!(!same_owner("PlFxOr.dat", "PlFcOr.dat"));
        assert!(same_owner("GrPs.dat", "GrPs.dat"));
        assert!(!same_owner("GrPs.dat", "GrSt.dat"));
        assert!(!same_owner("GrPs.dat", "PlFxNr.dat"));
        // A name that is no slot shares nothing, and cannot be sliced.
        assert!(!same_owner("Pl", "Pl"));
        assert_eq!(slot_color("PlFxOr.dat"), Some("Orange"));
        assert_eq!(slot_color("GrPs.dat"), None);
        assert_eq!(slot_color("Pl"), None);
    }

    #[test]
    fn costumes_group_by_fighter_in_roster_and_color_order() {
        let fighters = roster([
            "PlFcRe.dat",
            "PlFcNr.dat",
            "PlFcAJ.dat",
            "PlCaNr.dat",
            "GrPs.dat",
            "PlFcNr-custom.dat",
        ]);
        let names: Vec<_> = fighters.iter().map(|fighter| fighter.name).collect();
        assert_eq!(names, ["Captain Falcon", "Falco"]);
        let falco: Vec<_> = fighters[1]
            .costumes
            .iter()
            .map(|costume| (costume.name, costume.file.as_str()))
            .collect();
        assert_eq!(falco, [("Neutral", "PlFcNr.dat"), ("Red", "PlFcRe.dat")]);
    }

    #[test]
    fn slots_read_as_fighter_and_color_or_stage() {
        assert_eq!(slot_label(Some("PlFcRe.dat")), "Falco · Red");
        assert_eq!(slot_label(Some("GrNLa.dat")), "Final Destination");
        assert_eq!(slot_label(None), "no slot");
    }
}
