//! Melee: what's in the player's game. The roster of its fighters and
//! stages as plates, each saying whether the player has changed it, from
//! what every slot holds (read in the background). Opening one onto the
//! stage comes next. `actions` changes it; `view` draws it.

pub mod actions;
pub mod view;

use melee_dat::{MeleeSlot, Stage};
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
        Self { tab: Tab::Fighters, fighters: roster(&slots), stages, states: None }
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
