//! Play: the player's active set as a card they play from, the queues open
//! right now, and their last matches. Find a match starts the game in the
//! queue the set's gameplay mods make; the card's line says which queue
//! that is and how many are searching in it, worked out by comparing the
//! set's plan with the meeting point's queue list. Play with a friend is a
//! sheet: host a room and read out its code, or join one by typing it.
//!
//! Everything the page shows comes from the sets engine (`crate::sets`),
//! the replays the app already reads, and `GET /v1/queues`. `actions`
//! changes it; `view` draws it. The queues are read once each time Play
//! opens; what a running game says (its room code, a match found) is read
//! from its log about once a second while it runs.

pub mod actions;
pub mod view;

use std::time::Instant;
use tgg_mod::sets::{QueueMod, SetPlan};

/// The queues the meeting point lists, as it sends them.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Listing {
    #[serde(default)]
    pub queues: Vec<Queue>,
}

/// A queue someone is searching in. `kind` is `standing` for one textures.gg
/// keeps open, `player` for one a player's own set made.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Queue {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub searching: u32,
    /// The mods that have to match to meet here.
    #[serde(default)]
    pub mods: Vec<QueueMod>,
    /// The textures.gg set a standing queue comes from.
    #[serde(default)]
    pub set: Option<String>,
}

impl Queue {
    /// One textures.gg keeps open, listed even with nobody in it.
    pub fn standing(&self) -> bool {
        self.kind == "standing"
    }
}

/// Play's own state: what the meeting point last said, the picker's menu,
/// the friend sheet, and what the running game's log says.
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
    /// What the running game's log last said.
    pub log: Log,
    /// When the log was last read.
    pub log_at: Option<Instant>,
    /// The fighter the card shows, in the costume they had on: the player's
    /// from their latest match, else Fox's.
    pub hero: Option<melee_dat::MeleeSlot>,
    /// Play has opened once: its queues are asked for and its replays read.
    pub opened: bool,
}

/// What the running game's log says that Play shows: the room it made, and
/// who it found to play.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub room: Option<String>,
    pub found: Option<String>,
}

/// What Play's jobs found.
pub enum Done {
    /// The meeting point's queues, or why they couldn't be read.
    Queues(Result<Vec<Queue>, String>),
}

/// What the game's log says, from its start: the last room it made, and who
/// it last found to play (until it searches again).
///
/// The game writes its mods' lines as `<mod id>: <what it says>`, so a line
/// reads `tgg.direct-connect: meet: in room FOXD-7K2Q`.
pub fn read_log(text: &str) -> Log {
    let mut log = Log::default();
    for line in text.lines() {
        if let Some((_, said)) = line.split_once("meet: in room ") {
            let code = said.trim();
            if !code.is_empty() {
                log.room = Some(code.to_owned());
            }
        }
        if let Some((_, said)) = line.split_once("find a match: playing ")
            && let Some((name, _)) = said.split_once(" as side ")
        {
            log.found = Some(name.trim().to_owned());
        }
        if line.contains("meet: searching for a match") || line.contains("find a match: again") {
            log.found = None;
        }
    }
    log
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
    use super::{Log, Queue, matching, searching, searching_now};
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
    fn the_game_log_gives_its_room_and_who_it_found() {
        let log = read(
            "tgg.direct-connect: meet: in room FOXD-7K2Q\n\
             tgg.direct-connect: find a match: playing waffle as side 1\n",
        );
        assert_eq!(
            log,
            Log {
                room: Some("FOXD-7K2Q".into()),
                found: Some("waffle".into()),
            }
        );
    }

    #[test]
    fn searching_again_drops_the_player_it_found() {
        let log = read(
            "tgg.direct-connect: find a match: playing waffle as side 0\n\
             tgg.direct-connect: find a match: again\n",
        );
        assert_eq!(log.found, None);
    }

    #[test]
    fn a_log_that_says_nothing_reads_as_nothing() {
        assert_eq!(read("tgg.skins: loaded\n"), Log::default());
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

    fn read(text: &str) -> Log {
        super::read_log(text)
    }
}
