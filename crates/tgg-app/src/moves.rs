//! A fighter's moves as players browse them: grouped by kind of move, named
//! as players name them, each saying whether it plays and, when it doesn't,
//! why.

use melee_dat::fighter::moves::MoveGroup;
use melee_dat::{MeleeError, MeleeFighterPlayback};

/// One line of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum MoveRow {
    /// A kind of move ("Specials"), heading those under it.
    Heading(String),
    Move {
        /// The animation it plays.
        animation: usize,
        /// The name players use, else the action.
        title: String,
        /// The action beside a player name, as modders know it; empty
        /// otherwise.
        detail: String,
        /// Why it can't play, when it can't.
        refused: Option<String>,
    },
}

/// Every animation `playback` has an action for, grouped as players browse
/// them; empty slots and unnamed records are left out. Binds each once to
/// learn whether it plays (~0.1 ms each).
pub fn moves(playback: &MeleeFighterPlayback) -> Vec<MoveRow> {
    let mut listed: Vec<_> = playback
        .animations()
        .iter()
        .filter(|animation| animation.action.is_some())
        .collect();
    listed.sort_by_key(|animation| (animation.group, animation.index));
    let mut rows = Vec::new();
    let mut group = None::<MoveGroup>;
    for animation in listed {
        if group != Some(animation.group) {
            group = Some(animation.group);
            rows.push(MoveRow::Heading(animation.group.label().into()));
        }
        let refused = match playback.check(animation.index) {
            Ok(()) => None,
            Err(MeleeError::Unplayable { reason, .. }) => Some(capitalized(&reason)),
            Err(error) => Some(error.to_string()),
        };
        rows.push(MoveRow::Move {
            animation: animation.index,
            title: animation.label(),
            detail: animation
                .name
                .is_some()
                .then(|| animation.action.clone())
                .flatten()
                .unwrap_or_default(),
            refused,
        });
    }
    rows
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}
