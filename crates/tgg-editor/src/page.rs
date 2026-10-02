//! What pages share beyond the component library: the game skins install
//! into, as Melee's chip names it.

/// The game skins install into: whether Slippi plays it, and its disc
/// image's file name.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GameChip {
    pub slippi: bool,
    pub file: String,
}
