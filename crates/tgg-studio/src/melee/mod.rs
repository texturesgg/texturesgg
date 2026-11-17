//! Skins: textures.gg's skins and the player's own, a plate each with its
//! picture; a skin's sheet says where it goes and turns it on in tgg-melee
//! or writes it into their Melee disc. "Make a skin" opens the game's
//! fighters and stages to pick one, then the editor; saving keeps the skin
//! among the player's own. `actions` changes it; `view` draws the page,
//! `sheet` the skin and the picker, and `subject` the breadcrumb while a
//! slot is open.

pub mod actions;
pub mod sheet;
pub mod subject;
pub mod view;

use melee_dat::{Character, MeleeSlot, Stage};
use rune::TextEdit;
use std::collections::{HashMap, HashSet};
use tgg_app::Game;
use tgg_app::costumes::{Fighter, roster};
use tgg_app::ids::SkinId;
use tgg_app::install::SlotState;

/// Skins' tabs: what the site has, what's on, and what the player made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Browse,
    /// The skins tgg-melee plays now.
    Using,
    /// The skins saved from the editor.
    Made,
}

/// What a skin changes: the kinds the chips filter by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Fighters,
    Stages,
    Effects,
    Hud,
    Menus,
}

impl Kind {
    /// Every kind, in the order the chips show: the game's fighters and
    /// stages first, then what it puts on screen.
    pub const ALL: [Kind; 5] = [
        Kind::Fighters,
        Kind::Stages,
        Kind::Effects,
        Kind::Hud,
        Kind::Menus,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Fighters => "Fighters",
            Kind::Stages => "Stages",
            Kind::Effects => "Effects",
            Kind::Hud => "HUD",
            Kind::Menus => "Menus",
        }
    }
}

/// The two halves of the game's roster, for Make a skin's picker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Roster {
    #[default]
    Fighters,
    Stages,
}

/// What's open over the page: a skin's sheet, or the picker Make a skin
/// opens.
#[derive(Clone, Debug)]
pub enum Sheet {
    /// One of the player's skins.
    Skin(SkinId),
    /// One of textures.gg's.
    Site(crate::library::site::Pack),
}

#[derive(Default)]
pub struct Melee {
    pub tab: Tab,
    /// Only skins of this kind show; all when none.
    pub kind: Option<Kind>,
    /// What the player looks for by name.
    pub search: TextEdit,
    /// The skin's sheet, or Make a skin's picker.
    pub sheet: Option<Sheet>,
    /// The picker shows.
    pub making: bool,
    /// Which half of the roster the picker lists.
    pub picker: Roster,
    /// The game's fighters, in roster order.
    pub fighters: Vec<Fighter>,
    /// The game's stages, in file order.
    pub stages: Vec<Stage>,
    /// What each slot holds; `None` while it's being read.
    pub states: Option<HashMap<MeleeSlot, SlotState>>,
    /// The slots with an install to undo, as last read.
    pub undoable: HashSet<MeleeSlot>,
    /// The fighter or stage whose slot the editor has open, for its plate.
    pub subject: Option<Subject>,
    /// The costume last chosen for each fighter, to open it on again.
    pub chosen: HashMap<Character, MeleeSlot>,
    /// Why the last fighter or stage couldn't open, in the player's words.
    pub problem: Option<String>,
}

/// What's open on the stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Fighter(Character),
    Stage(Stage),
}

impl Melee {
    /// The roster of `game`, its slots' states not yet read.
    pub fn of(game: &Game) -> Self {
        let slots = game.slots();
        let stages = slots
            .iter()
            .filter_map(|slot| match slot {
                MeleeSlot::Stage(stage) => Some(*stage),
                _ => None,
            })
            .collect();
        Self {
            fighters: roster(&slots),
            stages,
            ..Self::default()
        }
    }

    /// Whether `slot` holds something other than the game's own file, once
    /// that's known.
    pub fn changed(&self, slot: MeleeSlot) -> bool {
        self.states
            .as_ref()
            .is_some_and(|states| !matches!(states.get(&slot), None | Some(SlotState::Vanilla)))
    }

    /// How many of `fighter`'s files the player has changed.
    pub fn changed_count(&self, fighter: &Fighter) -> usize {
        fighter.slots().filter(|slot| self.changed(*slot)).count()
    }

    /// The costume a fighter's plate shows: its first.
    pub fn shown_slot(fighter: &Fighter) -> Option<MeleeSlot> {
        fighter
            .costumes
            .first()
            .map(|costume| costume.slot(fighter))
    }

    /// The slot a fighter opens on: its costume last chosen, else its first.
    pub fn opening_slot(&self, fighter: &Fighter) -> Option<MeleeSlot> {
        self.chosen
            .get(&fighter.character)
            .copied()
            .or_else(|| Self::shown_slot(fighter))
    }
}

impl Subject {
    /// The name the breadcrumb gives it.
    pub fn name(self) -> &'static str {
        match self {
            Subject::Fighter(character) => character.name(),
            Subject::Stage(stage) => stage.name(),
        }
    }

    /// The name a plate keeps while the editor is open, so it flies between
    /// the picker and the stage.
    pub fn plate_id(self) -> String {
        match self {
            Subject::Fighter(character) => format!("melee/plate/{}", character.name()),
            Subject::Stage(stage) => format!("melee/plate/{}", stage.file_name()),
        }
    }
}
