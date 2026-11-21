//! What Play does: opening it (its queues, the replays its card reads),
//! starting a set, the friend sheet, and reading what a running game says.
//! `tick` runs before each frame while Play shows: it opens Play the first
//! time, works out the card's fighter, and reads the game's log and room
//! report about once a second, asking for the next second's frame so the
//! clock keeps up.

use super::{Done, Listing, Log, Queue, Room, read_log, read_report};
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
/// How often the running game's log and room report are read.
const LOG_EVERY: Duration = Duration::from_millis(900);
/// How Play's fighter is seen: turned from facing straight out, in radians,
/// toward the words beside it, with some depth; and how much closer than
/// the renderer's own framing.
const HERO_YAW: f64 = 0.5;
const HERO_ZOOM: f64 = 0.85;
/// How far a drag across Play's fighter turns it, in radians per point.
const SPIN_PER_POINT: f64 = 0.012;

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
                let log = crate::sets::actions::game_log(app);
                app.play.log = std::fs::read_to_string(log)
                    .map(|text| read_log(&text))
                    .unwrap_or_default();
                let report = crate::sets::actions::room_report(app);
                app.play.room = std::fs::read_to_string(report)
                    .map(|text| read_report(&text))
                    .unwrap_or_default();
            }
            // The clock's next second, and the room a host is waiting on.
            tx.wake_after(LOG_EVERY);
        }
        None if app.play.log != Log::default() || app.play.room != Room::default() => {
            app.play.log = Log::default();
            app.play.room = Room::default();
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

/// Host a room: the game makes it and reports its code, link and players,
/// which the sheet reads out.
pub fn host(app: &mut App) {
    begin_room(app);
    crate::sets::actions::launch(app, How::Host);
}

/// Join the room whose code was typed, or whose invite was pasted. The
/// sheet stays: it's where the game's way into the room shows.
pub fn join(app: &mut App) {
    let code = super::typed_code(&app.play.join.text);
    if code.is_empty() {
        return;
    }
    begin_room(app);
    crate::sets::actions::launch(app, How::Join(code));
}

/// Join the room a `textures-gg://join/<code>` link names, with the active
/// set, showing the sheet as the game makes its way in.
pub fn join_link(app: &mut App, code: &str) {
    begin_room(app);
    app.play.friend = true;
    crate::sets::actions::launch(app, How::Join(code.to_owned()));
}

/// A room is about to start: its report is written afresh, so nothing the
/// last game said shows.
fn begin_room(app: &mut App) {
    app.play.room = Room::default();
    app.play.log_at = None;
}

/// Copy the room's code, for pasting to a friend.
pub fn copy_code(app: &mut App, tx: &mut rune::Tx) {
    if !app.play.room.code.is_empty() {
        tx.copy(app.play.room.code.clone());
    }
}

/// Copy the room's invite, to send a friend: the site's page for it,
/// which chat apps make clickable and which opens the app.
pub fn copy_link(app: &mut App, tx: &mut rune::Tx) {
    if let Some(invite) = app.play.room.invite() {
        tx.copy(invite);
    }
}

/// Copy the sign-in code the room waits on, to confirm on textures.gg.
pub fn copy_sign_in_code(app: &mut App, tx: &mut rune::Tx) {
    if !app.play.room.signin.code.is_empty() {
        tx.copy(app.play.room.signin.code.clone());
    }
}

/// Open the page the room's sign-in code is confirmed on.
pub fn open_sign_in(app: &mut App, tx: &mut rune::Tx) {
    let url = app.play.room.signin.url.clone();
    if url.is_empty() {
        return;
    }
    if let Err(error) = tx.open_url(&url) {
        app.tell(format!("Couldn't open {url}: {error}"), true);
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
    stand(app);
}

/// Put the hero's fighter (Fox's, before it's known) in 3D on Play, unless
/// it's there already. When its model can't be read, Play shows its picture.
pub fn stand(app: &mut App) {
    let Some(slot) = app.play.hero.or_else(|| fox_slot(app)) else {
        return;
    };
    if app
        .play
        .fighter
        .as_ref()
        .is_some_and(|fighter| fighter.slot == slot)
    {
        return;
    }
    app.play.fighter = match crate::stage::actions::load(app, slot) {
        Ok(mut fighter) => {
            fighter.orbit.yaw = HERO_YAW;
            fighter.orbit.zoom *= HERO_ZOOM;
            Some(fighter)
        }
        Err(problem) => {
            tgg_app::log(&format!("Play's fighter: {problem}"));
            None
        }
    };
}

/// A drag across Play's fighter turns it around where it stands.
pub fn spin(app: &mut App, drag: &mut rune::Drag, _: &mut rune::Tx) {
    let Some(fighter) = &mut app.play.fighter else {
        return;
    };
    match drag.phase {
        rune::Phase::Down => fighter.gesture = Some([0.0, 0.0]),
        rune::Phase::Move => {
            let Some([heard, _]) = fighter.gesture else {
                return;
            };
            fighter.gesture = Some([drag.dx, 0.0]);
            fighter.orbit.yaw -= f64::from(drag.dx - heard) * SPIN_PER_POINT;
        }
        rune::Phase::Up => fighter.gesture = None,
    }
}

/// Ask for Fox's picture straight away, and stand him on Play, so the card
/// has him while the replays are read.
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
    stand(app);
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
