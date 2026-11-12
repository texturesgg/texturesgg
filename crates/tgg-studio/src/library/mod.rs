//! Library: the player's skins, kept by content hash, as plates grouped by
//! the fighter each was made for, each saying where it goes and whether
//! it's in their game. Adding skins looks at the files first: Review shows
//! where each seems to go, which the player can change, and the files the
//! app can't use with why, before any joins the library. `actions` changes
//! it; `view` draws the page and `review` the dialog.

pub mod actions;
pub mod port;
pub mod review;
pub mod view;

use melee_dat::MeleeSlot;
use tgg_app::library::{Candidate, Rejected};

/// The Library page's own state; the skins are the app's.
#[derive(Default)]
pub struct Page {
    pub filter: Filter,
    /// Files being looked at before they join the library.
    pub review: Option<Review>,
}

/// Which skins the page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    InGame,
    NotInstalled,
    /// Saved from the editor.
    Made,
}

/// Files the player chose, before they join the library.
pub struct Review {
    pub items: Vec<ReviewItem>,
    /// Files the app can't use, with why.
    pub rejected: Vec<Rejected>,
}

/// A file to add, and the slot the player wants it for.
pub struct ReviewItem {
    pub candidate: Candidate,
    /// Where it goes: the slot it seems made for, until the player says.
    pub slot: Option<MeleeSlot>,
}
