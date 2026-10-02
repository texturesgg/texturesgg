//! The player's costumes: every fighter's costume slots in their game, read
//! from its file table; how a slot reads to a player; and what a page of
//! them can ask the app to do.

use gpui::SharedString;
use melee_dat::{CHARACTERS, COSTUMES, ParsedFilename, costume_name, parse_filename, stage_name};
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
    let mut found: Vec<(&'static str, &'static str, String)> = files
        .into_iter()
        .filter_map(|file| match parse_filename(file)? {
            // Only the costume files themselves, not descriptive names.
            ParsedFilename::Character {
                character_code,
                costume_code,
            } if file == format!("Pl{character_code}{costume_code}.dat") => {
                Some((character_code, costume_code, file.to_owned()))
            }
            _ => None,
        })
        .collect();
    found.sort_by_key(|(_, costume, _)| COSTUMES.iter().position(|(code, _)| code == costume));
    CHARACTERS
        .iter()
        .filter_map(|&(code, name)| {
            let costumes: Vec<Costume> = found
                .iter()
                .filter(|(character, _, _)| *character == code)
                .map(|(_, costume, file)| Costume {
                    name: costume_name(costume),
                    file: file.clone(),
                })
                .collect();
            (!costumes.is_empty()).then_some(Fighter {
                code,
                name,
                costumes,
            })
        })
        .collect()
}

/// Whether two slots are costumes of the same fighter, or the same stage.
pub(crate) fn same_owner(slot: &str, other: &str) -> bool {
    match (parse_filename(slot), parse_filename(other)) {
        (
            Some(ParsedFilename::Character {
                character_code: fighter,
                ..
            }),
            Some(ParsedFilename::Character {
                character_code: other,
                ..
            }),
        ) => fighter == other,
        (
            Some(ParsedFilename::Stage { filename: stage }),
            Some(ParsedFilename::Stage { filename: other }),
        ) => stage == other,
        _ => false,
    }
}

/// A costume slot's color ("Orange"); a stage slot has none.
pub(crate) fn slot_color(slot: &str) -> Option<&'static str> {
    match parse_filename(slot)? {
        ParsedFilename::Character { costume_code, .. } => Some(costume_name(costume_code)),
        ParsedFilename::Stage { .. } => None,
    }
}

/// Where a skin goes, in the player's words: "Falco · Red", "Final
/// Destination", or "no slot" when its file doesn't say.
pub(crate) fn slot_label(slot: Option<&str>) -> String {
    match slot.and_then(parse_filename) {
        Some(ParsedFilename::Character {
            character_code,
            costume_code,
        }) => {
            let fighter = CHARACTERS
                .iter()
                .find(|(code, _)| *code == character_code)
                .map_or(character_code, |(_, name)| name);
            format!("{fighter} · {}", costume_name(costume_code))
        }
        Some(ParsedFilename::Stage { filename }) => stage_name(filename).to_owned(),
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
