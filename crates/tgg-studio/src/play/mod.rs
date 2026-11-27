//! Play: the player's active set as a card they play from, the queues open
//! right now, and their last matches. Find a match starts the game in the
//! queue the set's gameplay mods make; the card's line says which queue
//! that is and how many are searching in it, worked out by comparing the
//! set's plan with the meeting point's queue list. Play with a friend is a
//! sheet: host a room and read out its code and link, or join one by typing
//! its code.
//!
//! Everything the page shows comes from the sets engine (`crate::sets`),
//! the replays the app already reads, and the meeting point's queues
//! ([`tgg_mod::queues`]). `actions` changes it; `view` draws it. The queues
//! are read once each time Play opens; what a running game says is read
//! about once a second while it runs: the room report the game appends to
//! ([`Room`]) for the sheet, and its log for who Find a match found.

pub mod actions;
pub mod view;

use tgg_mod::Queue;
use tgg_mod::sets::{QueueMod, SetPlan};

/// Play's own state: what the meeting point last said, the picker's menu,
/// and the friend sheet.
#[derive(Default)]
pub struct Play {
    /// The queues, once Play has asked: `None` while they're on their way.
    pub queues: Option<Result<Vec<Queue>, String>>,
    /// The set picker's menu shows.
    pub menu: bool,
    /// The friend sheet shows.
    pub friend: bool,
    /// The code typed in the friend sheet's Join.
    pub join: rune::TextEdit,
    /// The fighter the card shows, in the costume they had on: the player's
    /// from their latest match, else Fox's.
    pub hero: Option<melee_dat::MeleeSlot>,
    /// Play has opened once: its queues are asked for and its replays read.
    pub opened: bool,
    /// The hero's fighter in 3D, idling: the model of `hero`'s slot (or
    /// Fox's), drawn live by the viewport. `None` until it's read, or when
    /// it can't be, and the picture shows instead.
    pub fighter: Option<crate::stage::Shown>,
}

/// The room code in what was typed in the join box: the code, or the code
/// of a pasted invite.
pub fn typed_code(text: &str) -> String {
    match crate::links::parse(text) {
        Some(crate::links::Link::Join(code)) => code,
        _ => text.trim().to_owned(),
    }
}

/// What Play's jobs found.
pub enum Done {
    /// The meeting point's queues, or why they couldn't be read.
    Queues(Result<Vec<Queue>, String>),
}

/// A set's gameplay mods, by id, in the shape a queue lists its own: what
/// decides which queue the set searches in.
pub fn gameplay(plan: &SetPlan) -> Vec<QueueMod> {
    let mut mods = plan.gameplay();
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    mods
}

/// The queue in `queues` whose mods are exactly `gameplay` (a set's), by
/// position; `None` when the set makes a queue nobody is in.
pub fn matching(queues: &[Queue], gameplay: &[QueueMod]) -> Option<usize> {
    queues.iter().position(|queue| {
        if queue.mods.is_empty() {
            return false;
        }
        let mut mods = queue.mods.clone();
        mods.sort_by(|a, b| a.id.cmp(&b.id));
        mods == gameplay
    })
}

/// How many are searching, in the words the card and a row use.
pub fn searching(count: u32) -> String {
    match count {
        0 => "nobody searching yet".to_owned(),
        1 => "1 searching".to_owned(),
        n => format!("{n} searching"),
    }
}

/// The top bar's count, once the list is read: the players searching across
/// every queue. (There is no playing count anywhere to show.)
pub fn searching_now(count: u32) -> String {
    match count {
        0 => "nobody searching now".to_owned(),
        1 => "1 searching now".to_owned(),
        n => format!("{n} searching now"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Queue, matching, searching, searching_now, typed_code};
    use tgg_mod::sets::QueueMod;

    fn queue(kind: &str, label: &str, searching: u32, mods: &[(&str, &str)]) -> Queue {
        Queue {
            kind: kind.to_owned(),
            label: label.to_owned(),
            searching,
            set: None,
            mods: mods
                .iter()
                .map(|(id, version)| mod_of(id, version))
                .collect(),
        }
    }

    fn mod_of(id: &str, version: &str) -> QueueMod {
        QueueMod {
            id: id.parse().expect("an id"),
            version: version.parse().expect("a version"),
        }
    }

    /// A set's gameplay mods, by id, as [`super::gameplay`] gives them.
    fn gameplay(mods: &[(&str, &str)]) -> Vec<QueueMod> {
        mods.iter()
            .map(|(id, version)| mod_of(id, version))
            .collect()
    }

    #[test]
    fn the_join_box_takes_a_code_or_a_pasted_invite() {
        assert_eq!(typed_code(" KFXD "), "KFXD");
        assert_eq!(typed_code("https://textures.gg/join/kfxd"), "KFXD");
        assert_eq!(typed_code("textures-gg://join/KFXD/"), "KFXD");
        assert_eq!(typed_code(""), "");
    }

    #[test]
    fn the_set_searches_in_the_queue_whose_mods_it_shares() {
        let queues = vec![
            queue("standing", "Competitive", 14, &[("tgg.ucf", "1.0.0")]),
            queue(
                "player",
                "Turbo",
                3,
                &[("tgg.ucf", "1.0.0"), ("tgg.turbo", "1.0.0")],
            ),
        ];
        assert_eq!(
            matching(&queues, &gameplay(&[("tgg.ucf", "1.0.0")])),
            Some(0)
        );
        // Order doesn't matter: both lists are by id.
        assert_eq!(
            matching(
                &queues,
                &gameplay(&[("tgg.turbo", "1.0.0"), ("tgg.ucf", "1.0.0")])
            ),
            Some(1)
        );
        assert_eq!(
            matching(&queues, &gameplay(&[("tgg.wavedash", "1.0.0")])),
            None
        );
        assert_eq!(
            matching(&queues, &gameplay(&[("tgg.ucf", "0.9.0")])),
            None,
            "the version has to match too"
        );
    }

    #[test]
    fn a_queue_nobody_named_matches_no_set() {
        let queues = vec![queue("player", "", 1, &[])];
        assert_eq!(matching(&queues, &gameplay(&[])), None, "an empty set too");
    }

    #[test]
    fn counts_read_as_a_player_reads_them() {
        assert_eq!(searching(0), "nobody searching yet");
        assert_eq!(searching(1), "1 searching");
        assert_eq!(searching(14), "14 searching");
        assert_eq!(searching_now(0), "nobody searching now");
        assert_eq!(searching_now(1), "1 searching now");
        assert_eq!(searching_now(14), "14 searching now");
    }
}
