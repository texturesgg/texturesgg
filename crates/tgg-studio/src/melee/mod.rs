//! Melee: what's in the player's game. The roster of its fighters and
//! stages as plates, each saying whether the player has changed it, from
//! what every slot holds (read in the background). Opening one shows it on
//! the stage, with its costumes, the files they share and its moves in the
//! pane column beside it. `actions` changes it; `view` draws the roster and
//! `subject` the one opened.

pub mod actions;
pub mod subject;
pub mod view;

use melee_dat::{Character, MeleeSlot, Stage};
use std::collections::HashMap;
use tgg_app::Game;
use tgg_app::costumes::{Fighter, roster};
use tgg_app::install::SlotState;

/// The roster's two halves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Fighters,
    Stages,
}

#[derive(Default)]
pub struct Melee {
    pub tab: Tab,
    /// The game's fighters, in roster order.
    pub fighters: Vec<Fighter>,
    /// The game's stages, in file order.
    pub stages: Vec<Stage>,
    /// What each slot holds; `None` while it's being read.
    pub states: Option<HashMap<MeleeSlot, SlotState>>,
    /// The fighter or stage opened onto the stage, if one is.
    pub subject: Option<Subject>,
    /// The costume last chosen for each fighter, to open it on again.
    pub chosen: HashMap<Character, MeleeSlot>,
    pub panes: Panes,
    /// Why the last fighter or stage couldn't open, in the player's words.
    pub problem: Option<String>,
}

/// What's opened onto the stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Fighter(Character),
    Stage(Stage),
}

/// Which of the subject's panes are folded to their headers. Moves starts
/// folded: the list is long, and the stage matters more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panes {
    pub costumes: bool,
    pub shared: bool,
    pub moves: bool,
}

impl Default for Panes {
    fn default() -> Self {
        Self { costumes: false, shared: false, moves: true }
    }
}

/// A pane of the subject's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Costumes,
    Shared,
    Moves,
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
        Self { fighters: roster(&slots), stages, ..Self::default() }
    }

    /// Whether `slot` holds something other than the game's own file, once
    /// that's known.
    pub fn changed(&self, slot: MeleeSlot) -> bool {
        self.states.as_ref().is_some_and(|states| !matches!(states.get(&slot), None | Some(SlotState::Vanilla)))
    }

    /// How many of `fighter`'s files the player has changed.
    pub fn changed_count(&self, fighter: &Fighter) -> usize {
        fighter.slots().filter(|slot| self.changed(*slot)).count()
    }

    /// The costume a fighter's plate shows: its first.
    pub fn shown_slot(fighter: &Fighter) -> Option<MeleeSlot> {
        fighter.costumes.first().map(|costume| costume.slot(fighter))
    }
}

impl Melee {
    /// The fighter opened, when it's a fighter.
    pub fn fighter(&self) -> Option<&Fighter> {
        let Some(Subject::Fighter(character)) = self.subject else { return None };
        self.fighters.iter().find(|fighter| fighter.character == character)
    }

    /// The slot a fighter opens on: its costume last chosen, else its first.
    pub fn opening_slot(&self, fighter: &Fighter) -> Option<MeleeSlot> {
        self.chosen.get(&fighter.character).copied().or_else(|| Self::shown_slot(fighter))
    }
}

impl Panes {
    pub fn folded(&self, pane: Pane) -> bool {
        match pane {
            Pane::Costumes => self.costumes,
            Pane::Shared => self.shared,
            Pane::Moves => self.moves,
        }
    }
}

impl Subject {
    /// The name the title row gives it.
    pub fn name(self) -> &'static str {
        match self {
            Subject::Fighter(character) => character.name(),
            Subject::Stage(stage) => stage.name(),
        }
    }

    /// The name a roster plate and the stage share, so one flies into the other.
    pub fn plate_id(self) -> String {
        match self {
            Subject::Fighter(character) => format!("melee/plate/{}", character.name()),
            Subject::Stage(stage) => format!("melee/plate/{}", stage.file_name()),
        }
    }
}
