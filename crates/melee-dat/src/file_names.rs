//! How the game names its files: `PlFcRe.dat` is Falco's Red costume,
//! `GrNBa.dat` is Battlefield. A [`MeleeSlot`] is one such file: what a skin
//! replaces.

use std::fmt;
use std::str::FromStr;

const CHARACTERS: &[(&str, &str)] = &[
    ("Kp", "Bowser"),
    ("Ca", "Captain Falcon"),
    ("Dk", "Donkey Kong"),
    ("Dr", "Dr. Mario"),
    ("Fc", "Falco"),
    ("Fx", "Fox"),
    ("Gn", "Ganondorf"),
    ("Pp", "Ice Climbers"),
    ("Pr", "Jigglypuff"),
    ("Kb", "Kirby"),
    ("Lk", "Link"),
    ("Lg", "Luigi"),
    ("Mr", "Mario"),
    ("Ms", "Marth"),
    ("Mt", "Mewtwo"),
    ("Gw", "Mr. Game & Watch"),
    ("Nn", "Nana"),
    ("Ns", "Ness"),
    ("Pe", "Peach"),
    ("Pc", "Pichu"),
    ("Pk", "Pikachu"),
    ("Fe", "Roy"),
    ("Ss", "Samus"),
    ("Sk", "Sheik"),
    ("Cl", "Young Link"),
    ("Ys", "Yoshi"),
    ("Zd", "Zelda"),
];

const COSTUMES: &[(&str, &str)] = &[
    ("Nr", "Neutral"),
    ("Re", "Red"),
    ("Bu", "Blue"),
    ("Gr", "Green"),
    ("Wh", "White"),
    ("Aq", "Aqua"),
    ("La", "Lavender"),
    ("Ye", "Yellow"),
    ("Bk", "Black"),
    ("Or", "Orange"),
    ("Pi", "Pink"),
    ("Gy", "Gray"),
];

/// The versus stages' files, as the decomp's stage modules load them
/// (`grlast.c` loads `GrNLa.dat` for Final Destination, for example).
const STAGES: &[(&str, &str)] = &[
    ("GrNBa.dat", "Battlefield"),
    ("GrBb.dat", "Big Blue"),
    ("GrZe.dat", "Brinstar"),
    ("GrKr.dat", "Brinstar Depths"),
    ("GrCn.dat", "Corneria"),
    ("GrOp.dat", "Dream Land N64"),
    ("GrNLa.dat", "Final Destination"),
    ("GrFz.dat", "Flat Zone"),
    ("GrIz.dat", "Fountain of Dreams"),
    ("GrFs.dat", "Fourside"),
    ("GrGb.dat", "Great Bay"),
    ("GrGr.dat", "Green Greens"),
    ("GrIm.dat", "Icicle Mountain"),
    ("GrGd.dat", "Jungle Japes"),
    ("GrKg.dat", "Kongo Jungle"),
    ("GrOk.dat", "Kongo Jungle N64"),
    ("GrI1.dat", "Mushroom Kingdom"),
    ("GrI2.dat", "Mushroom Kingdom II"),
    ("GrMc.dat", "Mute City"),
    ("GrOt.dat", "Onett"),
    ("GrPu.dat", "Poke Floats"),
    ("GrPs.dat", "Pokemon Stadium"),
    ("GrCs.dat", "Princess Peach's Castle"),
    ("GrRc.dat", "Rainbow Cruise"),
    ("GrSh.dat", "Temple"),
    ("GrVe.dat", "Venom"),
    ("GrYt.dat", "Yoshi's Island"),
    ("GrOy.dat", "Yoshi's Island N64"),
    ("GrSt.dat", "Yoshi's Story"),
];

/// A fighter as the game's files name it: the `Fc` of `PlFcRe.dat`. The Ice
/// Climbers are two, Popo (`Pp`) and Nana (`Nn`), as their files are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Character(u8);

impl Character {
    /// Every fighter with costume files, in roster order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..CHARACTERS.len() as u8).map(Self)
    }

    /// The fighter a two-letter file code names, whatever its case.
    pub fn from_code(code: &str) -> Option<Self> {
        position(CHARACTERS, code).map(Self)
    }

    /// The two-letter code in file names (`Fc`).
    pub fn code(self) -> &'static str {
        CHARACTERS[usize::from(self.0)].0
    }

    /// The name players use ("Falco").
    pub fn name(self) -> &'static str {
        CHARACTERS[usize::from(self.0)].1
    }
}

/// A costume color as the game's files name it: the `Re` of `PlFcRe.dat`.
/// Colors order as the character select screen lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CostumeColor(u8);

impl CostumeColor {
    /// The color every fighter has (`Nr`).
    pub const NEUTRAL: Self = Self(0);

    pub fn all() -> impl Iterator<Item = Self> {
        (0..COSTUMES.len() as u8).map(Self)
    }

    /// The color a two-letter file code names, whatever its case.
    pub fn from_code(code: &str) -> Option<Self> {
        position(COSTUMES, code).map(Self)
    }

    /// The two-letter code in file names (`Re`).
    pub fn code(self) -> &'static str {
        COSTUMES[usize::from(self.0)].0
    }

    /// The name players use ("Red").
    pub fn name(self) -> &'static str {
        COSTUMES[usize::from(self.0)].1
    }
}

/// A versus stage, by the file the game loads for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Stage(u8);

impl Stage {
    /// Every versus stage, by name.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..STAGES.len() as u8).map(Self)
    }

    /// The stage whose file is exactly `file_name` (`GrNLa.dat`).
    pub fn from_file_name(file_name: &str) -> Option<Self> {
        STAGES
            .iter()
            .position(|(file, _)| *file == file_name)
            .map(|index| Self(index as u8))
    }

    /// The file the game loads (`GrNLa.dat`).
    pub fn file_name(self) -> &'static str {
        STAGES[usize::from(self.0)].0
    }

    /// The name players use ("Final Destination").
    pub fn name(self) -> &'static str {
        STAGES[usize::from(self.0)].1
    }
}

fn position(values: &[(&str, &str)], code: &str) -> Option<u8> {
    values
        .iter()
        .position(|(value, _)| value.eq_ignore_ascii_case(code))
        .map(|index| index as u8)
}

/// A file of the game that a skin replaces: one fighter's costume of one
/// color, or a versus stage. Whether a given game has the slot (not every
/// fighter has every color) is for [`crate::vanilla`] or the disc to say.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeleeSlot {
    Costume {
        character: Character,
        color: CostumeColor,
    },
    Stage(Stage),
}

impl MeleeSlot {
    /// The slot whose file is exactly `file_name`: `PlFcRe.dat`, `GrNLa.dat`.
    pub fn from_file_name(file_name: &str) -> Option<Self> {
        let slot = parse_filename(file_name)?;
        (slot.file_name() == file_name).then_some(slot)
    }

    /// The file the game loads for the slot.
    pub fn file_name(self) -> String {
        match self {
            Self::Costume { character, color } => {
                format!("Pl{}{}.dat", character.code(), color.code())
            }
            Self::Stage(stage) => stage.file_name().to_owned(),
        }
    }

    /// The fighter, for a costume slot.
    pub fn character(self) -> Option<Character> {
        match self {
            Self::Costume { character, .. } => Some(character),
            Self::Stage(_) => None,
        }
    }

    /// The color, for a costume slot.
    pub fn color(self) -> Option<CostumeColor> {
        match self {
            Self::Costume { color, .. } => Some(color),
            Self::Stage(_) => None,
        }
    }

    /// Whether both are costumes of one fighter, or the same stage.
    pub fn same_owner(self, other: Self) -> bool {
        match (self, other) {
            (
                Self::Costume { character, .. },
                Self::Costume {
                    character: other, ..
                },
            ) => character == other,
            (Self::Stage(stage), Self::Stage(other)) => stage == other,
            _ => false,
        }
    }
}

/// The slot's file name.
impl fmt::Display for MeleeSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.file_name())
    }
}

/// A name that is not exactly a slot's file name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not the file name of a costume or versus stage")]
pub struct NotASlot;

impl FromStr for MeleeSlot {
    type Err = NotASlot;

    fn from_str(file_name: &str) -> Result<Self, NotASlot> {
        Self::from_file_name(file_name).ok_or(NotASlot)
    }
}

/// The slot a file seems made for, from a name that may say more than the
/// slot's own: `PlFxOr-Asymm-Jacket.dat` is Fox's Orange costume, and
/// `Tournament-GrNBa-remix.dat` is Battlefield. A leading path is ignored.
pub fn parse_filename(filename: &str) -> Option<MeleeSlot> {
    let base = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    let bytes = base.as_bytes();

    for start in 0..bytes.len().saturating_sub(5) {
        if !bytes[start..].starts_with(b"Pl") {
            continue;
        }
        let code = |range: std::ops::Range<usize>| std::str::from_utf8(&bytes[range]).ok();
        if let (Some(character), Some(color)) = (
            code(start + 2..start + 4).and_then(Character::from_code),
            code(start + 4..start + 6).and_then(CostumeColor::from_code),
        ) {
            return Some(MeleeSlot::Costume { character, color });
        }
    }

    Stage::all()
        .find(|stage| {
            let file = stage.file_name();
            base.contains(file.strip_suffix(".dat").unwrap_or(file))
        })
        .map(MeleeSlot::Stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn costume(character: &str, color: &str) -> MeleeSlot {
        MeleeSlot::Costume {
            character: Character::from_code(character).unwrap(),
            color: CostumeColor::from_code(color).unwrap(),
        }
    }

    fn stage(file_name: &str) -> MeleeSlot {
        MeleeSlot::Stage(Stage::from_file_name(file_name).unwrap())
    }

    #[test]
    fn parses_character_filename_with_suffix() {
        assert_eq!(
            parse_filename("PlFxOr-Asymm-Jacket.dat"),
            Some(costume("Fx", "Or"))
        );
    }

    #[test]
    fn parses_known_stage_inside_descriptive_filename() {
        assert_eq!(
            parse_filename("Tournament-GrNBa-remix.dat"),
            Some(stage("GrNBa.dat"))
        );
    }

    #[test]
    fn final_destination_is_grnla_as_the_game_loads_it() {
        // A reported skin, GrNLaWaffle.dat, was sent to a GrNFd.dat no ISO has.
        let slot = parse_filename("GrNLaWaffle.dat");
        assert_eq!(slot, Some(stage("GrNLa.dat")));
        let Some(MeleeSlot::Stage(found)) = slot else {
            panic!("a stage");
        };
        assert_eq!(found.name(), "Final Destination");
    }

    #[test]
    fn every_versus_stage_has_one_distinct_file() {
        let files: std::collections::HashSet<_> = STAGES.iter().map(|(file, _)| file).collect();
        let names: std::collections::HashSet<_> = STAGES.iter().map(|(_, name)| name).collect();
        assert_eq!((STAGES.len(), files.len(), names.len()), (29, 29, 29));
    }

    #[test]
    fn malformed_unicode_character_code_does_not_panic() {
        assert_eq!(parse_filename("éPléNr.dat"), None);
    }

    #[test]
    fn rejects_unknown_targets() {
        assert_eq!(parse_filename("PlXxNr.dat"), None);
        assert_eq!(parse_filename("notes.txt"), None);
    }

    /// A slot is its own file name and nothing looser: a descriptive name
    /// finds a slot but is not one.
    #[test]
    fn a_slot_round_trips_through_exactly_its_file_name() {
        for name in ["PlFcRe.dat", "PlPpNr.dat", "GrNLa.dat"] {
            let slot: MeleeSlot = name.parse().unwrap();
            assert_eq!(slot.to_string(), name);
        }
        assert_eq!("PlFcRe-custom.dat".parse::<MeleeSlot>(), Err(NotASlot));
        assert_eq!("plfcre.dat".parse::<MeleeSlot>(), Err(NotASlot));
        assert!(costume("Fx", "Or").same_owner(costume("Fx", "Nr")));
        assert!(!costume("Fx", "Or").same_owner(costume("Fc", "Or")));
        assert!(!stage("GrPs.dat").same_owner(costume("Fx", "Nr")));
    }
}
