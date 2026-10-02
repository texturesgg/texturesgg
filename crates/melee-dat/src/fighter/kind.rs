//! The game's own numbering of fighters and of a fighter's costumes.

use std::fmt;

/// The game's internal `FighterKind` (`ft/types.h`): Mario is 0, Fox 1, and
/// Popo and Nana are separate kinds. It is not Slippi's external character ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FighterKind(u8);

impl FighterKind {
    /// How many kinds the source table has, the unplayable ones included.
    pub const COUNT: u8 = 0x21;

    pub const PEACH: Self = Self(0x09);
    pub const POPO: Self = Self(0x0A);
    pub const NANA: Self = Self(0x0B);
    pub const PICHU: Self = Self(0x17);
    pub const GAME_AND_WATCH: Self = Self(0x18);

    /// The kind numbered `value`; `None` past the source table.
    pub const fn new(value: u8) -> Option<Self> {
        if value < Self::COUNT {
            Some(Self(value))
        } else {
            None
        }
    }

    /// Its number in the source table.
    pub const fn index(self) -> u8 {
        self.0
    }
}

impl fmt::Display for FighterKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A costume by its ID: its position in the fighter's costume list, the
/// neutral one being 0. An ID at or past the fighter's costume count plays as
/// costume 0 (`fighter.c:722`); callers clamp before reading tables with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CostumeIndex(pub usize);
