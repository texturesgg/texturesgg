//! The fighter's moves as the browser lists them: grouped by kind of move,
//! named as players name them, with the ones the binder refuses dimmed and
//! the reason on hover.

use melee_dat::fighter::moves::MoveGroup;
use melee_dat::{MeleeError, MeleeFighterPlayback};
use std::rc::Rc;
use tgg_ui::OptionRow;

#[derive(Default)]
pub(crate) struct Moves {
    pub rows: Rc<[OptionRow]>,
    /// The animation each row plays; `None` for a heading.
    animations: Vec<Option<usize>>,
    /// How many moves can play.
    pub playable: usize,
}

impl Moves {
    /// List every animation `playback` has an action for; empty slots and
    /// unnamed records are left out. Binds each once to learn whether it
    /// plays (~0.1 ms each).
    pub fn read(playback: &MeleeFighterPlayback) -> Self {
        let mut listed: Vec<_> = playback
            .animations()
            .iter()
            .filter(|animation| animation.action.is_some())
            .collect();
        listed.sort_by_key(|animation| (animation.group, animation.index));
        let mut moves = Self::default();
        let mut rows = Vec::new();
        let mut group = None::<MoveGroup>;
        for animation in listed {
            if group != Some(animation.group) {
                group = Some(animation.group);
                rows.push(OptionRow::Heading(animation.group.label().into()));
                moves.animations.push(None);
            }
            let disabled = match playback.check(animation.index) {
                Ok(()) => None,
                Err(MeleeError::Unplayable { reason, .. }) => Some(capitalized(&reason).into()),
                Err(error) => Some(error.to_string().into()),
            };
            moves.playable += usize::from(disabled.is_none());
            rows.push(OptionRow::Option {
                title: animation.label().into(),
                // The action beside its player name, as modders know it.
                detail: animation
                    .name
                    .is_some()
                    .then(|| animation.action.clone())
                    .flatten()
                    .unwrap_or_default()
                    .into(),
                disabled,
            });
            moves.animations.push(Some(animation.index));
        }
        moves.rows = rows.into();
        moves
    }

    /// The animation `row` plays.
    pub fn animation(&self, row: usize) -> Option<usize> {
        self.animations.get(row).copied().flatten()
    }

    /// The row that plays `animation`.
    pub fn row(&self, animation: usize) -> Option<usize> {
        self.animations
            .iter()
            .position(|listed| *listed == Some(animation))
    }
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}
