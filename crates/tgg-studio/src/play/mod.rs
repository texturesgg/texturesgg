//! Play: the player's active set as a card they play from, the queues open
//! right now, and their last matches. Find a match starts the game in the
//! queue the set's gameplay mods make; the card's line says which queue
//! that is and how many are searching in it, worked out by comparing the
//! set's plan with the meeting point's queue list. Play with a friend is a
//! sheet: host a room and read out its code and link, or join one by typing
//! its code.
//!
//! Everything the page shows comes from the sets engine (`crate::sets`),
//! the replays the app already reads, and `GET /v1/queues`. `actions`
//! changes it; `view` draws it. The queues are read once each time Play
//! opens; what a running game says is read about once a second while it
//! runs: the room report the game appends to ([`Room`]) for the sheet, and
//! its log for who Find a match found.

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
/// the friend sheet, and what the running game's report and log say.
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
    /// What the room report last said: the room, its players and its state.
    pub room: Room,
    /// What the running game's log last said.
    pub log: Log,
    /// When the log and the report were last read.
    pub log_at: Option<Instant>,
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

/// What the running game's log says that Play shows: who it found to play.
/// The room comes from the report ([`Room`]), not the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Log {
    pub found: Option<String>,
}

/// What the room report says: the last line of the file the game appends
/// to (`TGG_DIRECT_CONNECT_REPORT`, see [`read_report`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    pub state: State,
    /// The room's code, ready to share.
    pub code: String,
    /// The code as a link a friend opens, `textures-gg://join/<code>`.
    pub link: String,
    /// Who made the room.
    pub host: String,
    /// Everyone in it, the host first.
    pub players: Vec<String>,
    /// The most players the room takes.
    pub max: u32,
    /// The page and code to confirm a sign-in on.
    pub signin: Signin,
    /// Why the room ended, in the game's words for the player.
    pub why: String,
}

impl Room {
    /// How full the room is, in the words the sheet uses: `2 of 4 players`,
    /// or just the count while the game hasn't said how many it takes.
    pub fn count(&self) -> String {
        let players = self.players.len();
        match self.max {
            0 => players.to_string(),
            max => format!("{players} of {max} players"),
        }
    }
}

/// Where a room is, as the report's `state` names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    /// No line read yet.
    #[default]
    None,
    /// Signing in and reaching textures.gg.
    Starting,
    /// Nobody's signed in: confirm the sign-in code on the page given.
    SigningIn,
    /// In the room, before the host starts.
    Waiting,
    /// Getting the host's mods; the game restarts and joins again.
    GettingMods,
    /// The host started.
    Playing,
    /// Left the room, or it closed.
    Ended,
}

impl State {
    /// The state `name` is; `None` when the game doesn't write one so.
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "starting" => State::Starting,
            "signing-in" => State::SigningIn,
            "waiting" => State::Waiting,
            "getting-mods" => State::GettingMods,
            "playing" => State::Playing,
            "ended" => State::Ended,
            _ => return None,
        })
    }
}

/// The page and code a sign-in is confirmed with, as the report gives them.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Signin {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub code: String,
}

/// A line of the room report, as the game writes it: every field on every
/// line, `""` or `[]` when it doesn't apply.
#[derive(serde::Deserialize)]
struct Line {
    #[serde(default)]
    state: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    players: Vec<String>,
    #[serde(default)]
    max: u32,
    #[serde(default)]
    signin: Signin,
    #[serde(default)]
    why: String,
}

/// The room report's last line that parses, as a [`Room`]; an empty room
/// when none does. The game writes each line whole and appends, so a reader
/// takes the last and skips one that doesn't parse.
pub fn read_report(text: &str) -> Room {
    let mut room = Room::default();
    for line in text.lines() {
        let Ok(parsed) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        let Some(state) = State::named(&parsed.state) else {
            continue;
        };
        room = Room {
            state,
            code: parsed.code,
            link: parsed.link,
            host: parsed.host,
            players: parsed.players,
            max: parsed.max,
            signin: parsed.signin,
            why: parsed.why,
        };
    }
    room
}

/// What Play's jobs found.
pub enum Done {
    /// The meeting point's queues, or why they couldn't be read.
    Queues(Result<Vec<Queue>, String>),
}

/// What the game's log says, from its start: who it last found to play
/// (until it searches again).
///
/// The game writes its mods' lines as `<mod id>: <what it says>`, so a line
/// reads `tgg.direct-connect: find a match: playing waffle as side 1`.
pub fn read_log(text: &str) -> Log {
    let mut log = Log::default();
    for line in text.lines() {
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
    use super::{Log, Queue, Room, Signin, State, matching, read_report, searching, searching_now};
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
    fn the_game_log_says_who_it_found() {
        let log = read(
            "tgg.direct-connect: meet: in room FOXD-7K2Q\n\
             tgg.direct-connect: find a match: playing waffle as side 1\n",
        );
        assert_eq!(
            log,
            Log {
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

    /// A report line, as the game writes it.
    fn line(state: &str, code: &str, players: &[&str]) -> String {
        format!(
            "{{\"state\":\"{state}\",\"code\":\"{code}\",\"link\":\"{}\",\"host\":\"saavy\",\
             \"players\":[{}],\"max\":4,\"signin\":{{\"url\":\"\",\"code\":\"\"}},\"why\":\"\"}}",
            if code.is_empty() {
                String::new()
            } else {
                format!("textures-gg://join/{code}")
            },
            players
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<String>>()
                .join(","),
        )
    }

    #[test]
    fn the_room_report_reads_its_last_line() {
        let text = format!(
            "{}\n{}\n",
            line("starting", "", &[]),
            line("waiting", "ABCD", &["saavy", "bo"]),
        );
        let room = read_report(&text);
        assert_eq!(room.state, State::Waiting);
        assert_eq!(room.code, "ABCD");
        assert_eq!(room.link, "textures-gg://join/ABCD");
        assert_eq!(room.host, "saavy");
        assert_eq!(room.players, ["saavy", "bo"]);
        assert_eq!(room.max, 4);
        assert_eq!(room.count(), "2 of 4 players");
        assert_eq!(room.why, "");
    }

    #[test]
    fn a_report_line_that_doesnt_parse_is_skipped() {
        // The last line is one the game never finished writing, and one
        // before it says a state it doesn't write: the line before them
        // stands.
        let text = format!(
            "{}\n{{\"state\":\"waiting\",\"code\":\"WXYZ\",\"players\":\n\
             {{\"state\":\"flying\"}}\n",
            line("waiting", "ABCD", &["saavy"]),
        );
        assert_eq!(read_report(&text).code, "ABCD");
    }

    #[test]
    fn an_empty_report_reads_as_no_room() {
        assert_eq!(read_report(""), Room::default());
        assert_eq!(read_report("\n\n"), Room::default());
        assert_eq!(Room::default().state, State::None);
    }

    #[test]
    fn a_signed_out_room_reads_its_sign_in() {
        let text = "{\"state\":\"signing-in\",\"code\":\"\",\"link\":\"\",\"host\":\"\",\
                    \"players\":[],\"max\":4,\"signin\":{\"url\":\"https://textures.gg/device\",\
                    \"code\":\"ABCD-EFGH\"},\"why\":\"\"}\n";
        let room = read_report(text);
        assert_eq!(room.state, State::SigningIn);
        assert_eq!(
            room.signin,
            Signin {
                url: "https://textures.gg/device".into(),
                code: "ABCD-EFGH".into(),
            }
        );
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
