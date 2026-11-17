//! Library: the player's skins, and textures.gg's. A skin's sheet says where
//! it goes and what it takes to put it in tgg-melee or into their Melee
//! disc; Browse lists the site's skins with a preview each. Adding the
//! player's own files looks at them first: Review shows where each seems to
//! go, which the player can change, and the files the app can't use with
//! why, before any joins the library. `actions` changes it; `view` draws a
//! skin, `site` lists textures.gg's, and `review` the dialog.

pub mod actions;
pub mod port;
pub mod review;
pub mod site;
pub mod view;

use melee_dat::MeleeSlot;
use tgg_app::library::{Candidate, Rejected};

/// The Skins page's own state; the skins are the app's.
#[derive(Default)]
pub struct Page {
    /// Files being looked at before they join the library.
    pub review: Option<Review>,
    /// textures.gg's skins, for Browse.
    pub browse: site::Browse,
    /// A skin from the site on its way, and what to do with it after.
    pub pending: Option<(site::Pack, site::Then)>,
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
