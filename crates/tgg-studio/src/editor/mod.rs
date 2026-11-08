//! Edit: one costume open as a document, on the same stage Melee showed it
//! on. Its textures list in a pane beside the stage, the one selected tints
//! the surfaces that draw it, and a click on the model selects what it
//! landed on. Every edit shows on the model as it's made: a texture can be
//! replaced with a PNG, exported as one, or edited in the player's image
//! editor, and the colors outside its textures changed. Undo and redo step
//! through them; Save keeps the costume in the library as an edited skin,
//! and Save and install also writes it into the slot it came from. Leaving
//! with edits unsaved asks first. `actions` changes it, `files` takes
//! textures in and out, `external` round-trips them through the image
//! editor, `colors` changes its colors, and `view` and `colors_view` draw it.

pub mod actions;
pub mod colors;
pub mod colors_view;
pub mod external;
pub mod files;
pub mod view;

use crate::nav::Place;
use melee_dat::MeleeSlot;
use rune::Pixels;
use tgg_app::textures::Listing;

pub struct Document {
    /// The slot it's from, which Save and install writes.
    pub slot: MeleeSlot,
    pub listing: Listing,
    /// Each texture's picture, for the panes; filled as edits land.
    pub thumbnails: Vec<Pixels>,
    pub selected: Option<usize>,
    /// What the player asked for while edits were unsaved: the prompt
    /// shows until they answer.
    pub pending: Option<Pending>,
    /// Its textures open in the player's image editor.
    pub external: external::External,
    /// The colors it draws with that aren't in its textures, by group.
    pub colors: Vec<tgg_app::colors::ColorGroup>,
    /// The color change being made, before it's applied.
    pub color_pick: Option<colors::ColorPick>,
}

/// What waits on the unsaved-changes prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Back to the fighter on the stage.
    Leave,
    /// Close the window.
    Close,
    /// Go to another place.
    Go(Place),
    /// Open another of the game's slots for editing: a recent edit.
    Edit(melee_dat::MeleeSlot),
}

impl Document {
    /// Whether there are edits not yet saved.
    pub fn modified(&self) -> bool {
        self.listing
            .document
            .as_ref()
            .is_ok_and(|document| document.is_modified())
    }

    /// How many textures it has.
    pub fn texture_count(&self) -> usize {
        self.listing
            .document
            .as_ref()
            .map_or(0, |document| document.textures().len())
    }
}
