//! What Play does: opening it (its queues, the replays its card reads),
//! starting a set, the friend sheet, and reading what a running game says.
//! `tick` runs before each frame while Play shows: it opens Play the first
//! time, works out the card's fighter, and reads the game's log about once
//! a second, asking for the next second's frame so the clock keeps up.

use super::{Done, Listing, Log, Queue, read_log};
use crate::jobs::Done as Job;
use crate::nav::Place;
use crate::sets::How;
use crate::state::App;
use std::time::{Duration, Instant};
use tgg_mod::ReplayInfo;

/// The meeting point's queues: the standing ones, and the players' own.
const QUEUES_URL: &str = "https://meet.textures.gg/v1/queues?limit=20";
/// How much of its answer Play reads.
const QUEUES_MOST: u64 = 256 * 1024;
/// How often the running game's log is read.
const LOG_EVERY: Duration = Duration::from_millis(900);

/// Play opened: read the queues once, and the replays its card shows.
pub fn opened(app: &mut App) {
    app.play.opened = true;
    // Reads the disc and what's installed (and the sets), then the replays.
    crate::replays::actions::opened(app);
    read_queues(app);
    request_fox(app);
}

/// Ask the meeting point which queues are open, off the window's thread.
pub fn read_queues(app: &mut App) {
    app.play.queues = None;
    let net = app.mods.net.clone();
    app.jobs.spawn("queues", move || {
        let result = net
            .get(QUEUES_URL, QUEUES_MOST)
            .map_err(|error| error.to_string())
            .and_then(|body| {
                serde_json::from_slice::<Listing>(&body)
                    .map(|listing| listing.queues)
                    .map_err(|error| error.to_string())
            });
        Job::Play(Box::new(Done::Queues(result)))
    });
}

/// What a Play job found. True: what shows changed.
pub fn done(app: &mut App, done: Done) -> bool {
    match done {
        Done::Queues(result) => app.play.queues = Some(result),
    }
    true
}

/// Before each frame, while Play shows: open it the first time, keep the
/// card's fighter asked for, and read what a running game says.
pub fn tick(app: &mut App, tx: &mut rune::Tx) {
    if app.nav.place != Place::Play {
        return;
    }
    if !app.play.opened {
        opened(app);
    }
    hero(app);
    match app.sets.session {
        Some(_) => {
            let due = app.play.log_at.is_none_or(|at| at.elapsed() >= LOG_EVERY);
            if due {
                app.play.log_at = Some(Instant::now());
                let path = crate::sets::actions::game_log(app);
                app.play.log = std::fs::read_to_string(path)
                    .map(|text| read_log(&text))
                    .unwrap_or_default();
            }
            // The clock's next second, and the code a host is waiting on.
            tx.wake_after(LOG_EVERY);
        }
        None if app.play.log != Log::default() => {
            app.play.log = Log::default();
            app.play.log_at = None;
        }
        None => {}
    }
}

/// Find a match: sign in first when signed out, else start the set's game
/// searching in its queue.
pub fn find(app: &mut App) {
    if app.signin.who.is_none() {
        crate::signin::actions::begin(app);
        return;
    }
    crate::sets::actions::launch(app, How::Search);
}

/// Play the set offline, its menus and no search.
pub fn play_offline(app: &mut App) {
    crate::sets::actions::launch(app, How::Offline);
}

/// Show the friend sheet.
pub fn open_friend(app: &mut App) {
    app.play.friend = true;
}

pub fn close_friend(app: &mut App) {
    app.play.friend = false;
}

/// Host a room: the game makes it and reports its code, which the sheet
/// reads out.
pub fn host(app: &mut App) {
    crate::sets::actions::launch(app, How::Host);
}

/// Join the room whose code was typed.
pub fn join(app: &mut App) {
    let code = app.play.join.text.trim().to_owned();
    if code.is_empty() {
        return;
    }
    crate::sets::actions::launch(app, How::Join(code));
    if app.sets.launching.is_some() || app.mods.playing.is_some() {
        app.play.friend = false;
    }
}

/// Copy the room's code, for pasting to a friend.
pub fn copy_code(app: &mut App, tx: &mut rune::Tx) {
    if let Some(room) = &app.play.log.room {
        tx.copy(room.clone());
    }
}

/// Show or hide the set picker's menu.
pub fn toggle_menu(app: &mut App) {
    app.play.menu = !app.play.menu;
}

pub fn close_menu(app: &mut App) {
    app.play.menu = false;
}

/// Make `key` the set Play starts, and close the menu.
pub fn choose(app: &mut App, key: &str) {
    app.play.menu = false;
    crate::sets::actions::activate(app, key);
}

/// The card's fighter: the one the player had in their latest match, else
/// Fox's first costume. Asks for its picture once, and again when it moves.
pub fn hero(app: &mut App) {
    if app.play.hero.is_some() || !app.replays.scanned {
        return;
    }
    let Some(iso) = app
        .game
        .as_ref()
        .map(|references| references.game().path().to_owned())
    else {
        return;
    };
    let slot = latest_fighter(app).or_else(|| fox_slot(app));
    app.play.hero = slot;
    if let Some(slot) = slot {
        app.renders.request_fighter(&iso, slot);
    }
}

/// Ask for Fox's picture straight away, so the card has one while the
/// replays are read.
fn request_fox(app: &mut App) {
    let (Some(iso), Some(slot)) = (
        app.game
            .as_ref()
            .map(|references| references.game().path().to_owned()),
        fox_slot(app),
    ) else {
        return;
    };
    app.renders.request_fighter(&iso, slot);
}

/// Fox's first costume, before there's a match to show.
pub fn fox_slot(app: &App) -> Option<melee_dat::MeleeSlot> {
    let fox = melee_dat::Character::from_code("Fx")?;
    crate::replays::actions::costume_slot(app, fox, 0)
}

/// The fighter the player played as in their latest match, if the replay
/// says who they are.
fn latest_fighter(app: &App) -> Option<melee_dat::MeleeSlot> {
    app.replays.items.iter().find_map(|item| {
        let info = item.info.as_ref()?;
        let me = player_of(app, info)?;
        let character = me.character.and_then(tgg_app::replays::character)?;
        crate::replays::actions::costume_slot(app, character, me.costume.unwrap_or(0))
    })
}

/// The player in a replay, by the name they're signed in with, else by
/// their connect code; `None` when the replay doesn't say.
pub fn player_of<'a>(app: &App, info: &'a ReplayInfo) -> Option<&'a tgg_mod::ReplayPlayer> {
    let signed_in = app.signin.who.as_ref()?;
    let name = crate::signin::shown_name(&signed_in.account);
    let handle = signed_in.account.handle.clone();
    info.players
        .iter()
        .find(|player| named(player, &name) || named(player, &handle))
}

/// Whether a replay player goes by `name`: their netplay name, or the code
/// they connect with.
fn named(player: &tgg_mod::ReplayPlayer, name: &str) -> bool {
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return false;
    }
    let same = |found: &Option<String>| {
        found
            .as_deref()
            .is_some_and(|found| found.trim().to_lowercase() == name)
    };
    same(&player.name) || same(&player.code)
}

/// The queues the card's line and the list below it need: the standing one
/// the set searches in, if any.
pub fn queues_of(app: &App) -> Option<&[Queue]> {
    match &app.play.queues {
        Some(Ok(queues)) => Some(queues),
        _ => None,
    }
}
