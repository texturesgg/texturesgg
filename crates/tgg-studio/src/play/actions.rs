//! What Play does: opening it (its queues, the replays its card reads),
//! starting a set, the friend sheet, and reading what a running game says.
//! `tick` runs before each frame while Play shows: it opens Play the first
//! time, works out the card's fighter, and reads the game's log and room
//! report about once a second, asking for the next second's frame so the
//! clock keeps up.

use super::{Done, PendingAction, Queue, gameplay};
use crate::companion::{Activity, Command, Event, LiveState, OnlineState};
use crate::jobs::Done as Job;
use crate::nav::Place;
use crate::sets::How;
use crate::state::App;
use std::path::PathBuf;
use std::time::Duration;
use tgg_mod::ReplayInfo;
use tgg_mod::sets::QueueMod;

/// How often the live queue timer ticks while searching.
const CLOCK_TICK: Duration = Duration::from_secs(1);
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
        let result = tgg_mod::queues(&*net).map_err(|error| error.to_string());
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
/// card's fighter asked for, surface any refused companion command, dispatch
/// any queued post-leave action once back in menus, and keep the live queue
/// timer ticking while actually searching.
pub fn tick(app: &mut App, tx: &mut rune::Tx) {
    if let Some(ack) = app
        .companion
        .as_deref()
        .and_then(crate::companion::Companion::take_error_ack)
    {
        app.play.after_leave = None;
        let msg = match ack.reason.as_str() {
            "in-match" => "Finish or leave the current match first.".to_owned(),
            "busy" => "Leave the current online session in-game first.".to_owned(),
            "invalid" => "Room codes are 4 letters (no I, L, or O).".to_owned(),
            "bad-replay" => "Couldn't load that replay in the running game.".to_owned(),
            "unsupported" => "The running game doesn't have the required mod loaded.".to_owned(),
            _ => "Couldn't complete that action in the running game.".to_owned(),
        };
        app.tell(msg, true);
    }
    if let Some(events) = app
        .companion
        .as_deref()
        .map(crate::companion::Companion::drain_events)
    {
        let has_recorder = app
            .live_game()
            .is_some_and(|live| live.has_cap("replay_record"));
        let mut refresh_replays = false;
        for ev in events {
            match ev {
                Event::ReplaySaved { .. } => refresh_replays = true,
                Event::MatchEnded(_) if !has_recorder => refresh_replays = true,
                Event::MatchEnded(_) => {}
            }
        }
        if refresh_replays && app.replays.scanned {
            app.play.hero = None;
            crate::replays::actions::scan(app);
        }
    }
    if let Some(live) = app.live_game()
        && let Some(action) = app.play.after_leave.clone()
        && live.activity == Activity::Menu
        && !live.online.state.is_queue()
        && !live.online.state.is_room()
    {
        app.play.after_leave = None;
        dispatch_in_menus(app, action);
    }
    if app.nav.place != Place::Play {
        return;
    }
    if !app.play.opened {
        opened(app);
    }
    hero(app);
    if let Some(live) = app.live_game()
        && let Some(live_match) = live.match_state.as_ref()
        && let Some(iso) = app
            .game
            .as_ref()
            .map(|references| references.game().path().to_owned())
    {
        for player in &live_match.players {
            if let Some(ch) = player.character_id.and_then(tgg_app::replays::character)
                && let Some(slot) = crate::replays::actions::costume_slot(app, ch, player.costume)
            {
                app.renders.request(&iso, slot);
            }
        }
    }
    let wake = app
        .live_game()
        .is_some_and(|live| live.online.state == OnlineState::Searching)
        || app
            .companion
            .as_deref()
            .is_some_and(crate::companion::Companion::is_restarting);
    if wake {
        tx.wake_after(CLOCK_TICK);
    }
}

fn sorted_queue_mods(mods: &[QueueMod]) -> Vec<QueueMod> {
    let mut out = mods.to_vec();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Whether the currently running `tgg-melee` session has the exact gameplay
/// mods of a public queue (and online mods loaded) so it can join in-place.
pub fn queue_matches_running(app: &App, queue_mods: &[QueueMod]) -> bool {
    if queue_mods.is_empty() {
        return false;
    }
    if let Some(live) = app.live_game()
        && !live.caps.is_empty()
        && !live.has_cap("online")
    {
        return false;
    }
    let Some(session) = app.sets.session.as_ref() else {
        return false;
    };
    let target = sorted_queue_mods(queue_mods);
    if let How::Queue { mods, .. } = &session.how {
        return sorted_queue_mods(mods) == target;
    }
    if session.profile_key.is_some()
        || session.set_key.is_empty()
        || session.set_key == tgg_mod::sets::VANILLA
    {
        return false;
    }
    app.sets
        .plan(&session.set_key)
        .is_some_and(|plan| gameplay(plan) == target)
}

/// Whether the running game was launched from a different mod set or profile
/// than the currently active set in Studio.
fn needs_set_switch(app: &App) -> bool {
    let Some(session) = app.sets.session.as_ref() else {
        return false;
    };
    if session.profile_key.is_some() {
        return true;
    }
    if let How::Queue { mods, .. } = &session.how {
        if app.sets.is_vanilla() {
            return true;
        }
        return app
            .sets
            .plan(&app.sets.active)
            .is_none_or(|plan| gameplay(plan) != sorted_queue_mods(mods));
    }
    !session.set_key.is_empty() && session.set_key != app.sets.active
}

/// Whether completing `action` requires closing and relaunching `tgg-melee`
/// with a different mod set or profile folder.
pub fn needs_relaunch(app: &App, action: &PendingAction) -> bool {
    match action {
        PendingAction::SwitchSet => true,
        PendingAction::Find | PendingAction::Host | PendingAction::Join(_) => {
            if let Some(live) = app.live_game()
                && !live.caps.is_empty()
                && !live.has_cap("online")
            {
                return true;
            }
            needs_set_switch(app)
        }
        PendingAction::Queue { mods, .. } => !queue_matches_running(app, mods),
        PendingAction::Replay(path) => !crate::replays::actions::replay_fits_running(app, path),
    }
}

/// Whether the running game is currently in an active match or training session
/// where disrupting it should ask for confirmation first.
fn in_active_match(live: &LiveState) -> bool {
    matches!(live.activity, Activity::Match | Activity::Training)
        || matches!(
            live.online.state,
            OnlineState::Matched | OnlineState::RoomPlaying
        )
}

fn action_how(action: &PendingAction) -> How {
    match action {
        PendingAction::Find => How::Search,
        PendingAction::Host => How::Host,
        PendingAction::Join(code) => How::Join(code.clone()),
        PendingAction::Queue { label, mods } => How::Queue {
            label: label.clone(),
            mods: mods.clone(),
        },
        PendingAction::SwitchSet | PendingAction::Replay(_) => How::Offline,
    }
}

fn queue_relaunch(app: &mut App, action: PendingAction) {
    match action {
        PendingAction::Replay(path) => {
            app.sets.relaunch = None;
            app.replays.relaunch = Some(path);
        }
        other => {
            app.replays.relaunch = None;
            app.sets.relaunch = Some(action_how(&other));
        }
    }
    quit_game(app);
}

/// Send an online or replay command to an already-running game that is idle in
/// menus (or cleanly restart into the needed set/profile if mods changed).
fn dispatch_in_menus(app: &mut App, action: PendingAction) {
    if needs_relaunch(app, &action) {
        queue_relaunch(app, action);
        return;
    }
    let Some(companion) = &app.companion else {
        if let PendingAction::Replay(path) = action {
            app.replays.relaunch = Some(path);
            quit_game(app);
        } else {
            crate::sets::actions::launch(app, action_how(&action));
        }
        return;
    };
    match action {
        PendingAction::Find | PendingAction::Queue { .. } => {
            companion.send(Command::StartSearch {
                mode: "vs".to_owned(),
            });
            companion.send(Command::Focus);
        }
        PendingAction::Host => {
            companion.send(Command::HostRoom);
            companion.send(Command::Focus);
        }
        PendingAction::Join(code) => {
            companion.send(Command::JoinRoom { code });
            companion.send(Command::Focus);
        }
        PendingAction::Replay(path) => {
            companion.send(Command::LoadReplay {
                path: path.to_string_lossy().into_owned(),
            });
            companion.send(Command::Focus);
        }
        PendingAction::SwitchSet => {}
    }
}

/// Route an online, replay, or set-switch action through the running game
/// (prompting if in an active match, switching sets/profiles if needed, or
/// launching if not running).
fn run_or_prompt(app: &mut App, action: PendingAction) {
    if let Some(live) = app.live_game() {
        let disruptive = in_active_match(&live)
            || (matches!(action, PendingAction::Replay(_))
                && (live.online.state.is_queue() || live.online.state.is_room()))
            || (matches!(action, PendingAction::Find | PendingAction::Queue { .. })
                && live.online.state.is_room());
        if disruptive {
            app.play.confirm = Some(action);
            return;
        }
        if live.activity == Activity::Replay {
            if needs_relaunch(app, &action) {
                queue_relaunch(app, action);
                return;
            }
            if let Some(companion) = &app.companion {
                app.play.after_leave = Some(action);
                companion.send(Command::LeaveMatch);
                companion.send(Command::Focus);
            }
            return;
        }
        if live.online.state.is_queue() {
            if !needs_relaunch(app, &action)
                && matches!(action, PendingAction::Find | PendingAction::Queue { .. })
            {
                focus_game(app);
                return;
            }
            if needs_relaunch(app, &action) {
                queue_relaunch(app, action);
                return;
            }
            if let Some(companion) = &app.companion {
                app.play.after_leave = Some(action);
                companion.send(Command::StopSearch);
                companion.send(Command::Focus);
            }
            return;
        }
        dispatch_in_menus(app, action);
        return;
    }
    if app.mods.playing.is_some() || app.sets.session.is_some() {
        if needs_relaunch(app, &action) {
            queue_relaunch(app, action);
        }
        return;
    }
    if let PendingAction::Replay(path) = action {
        crate::replays::actions::play(app, &path);
        return;
    }
    crate::sets::actions::launch(app, action_how(&action));
}

/// Find a match: sign in first when signed out; if tgg-melee is already
/// running, ask it to open Find a match (or switch sets / confirm if in match).
pub fn find(app: &mut App) {
    if app.signin.who.is_none() {
        crate::signin::actions::begin(app);
        return;
    }
    run_or_prompt(app, PendingAction::Find);
}

/// Join a public queue row: sends `StartSearch` in-place when the running game
/// already has the queue's mods, or cleanly switches/relaunches (confirming
/// first if in an active match).
pub fn join_queue(app: &mut App, label: String, mods: Vec<QueueMod>) {
    run_or_prompt(app, PendingAction::Queue { label, mods });
}

/// Watch a replay while `tgg-melee` is already running: focuses it if already
/// playing that replay, loads it in-place when its mods fit, or cleanly
/// relaunches (confirming first if in an active match or online session).
pub fn watch_replay(app: &mut App, path: PathBuf) {
    if let Some(live) = app.live_game()
        && live.activity == Activity::Replay
        && !live.replay_path.is_empty()
        && std::path::Path::new(&live.replay_path) == path.as_path()
    {
        focus_game(app);
        return;
    }
    run_or_prompt(app, PendingAction::Replay(path));
}

/// Switch the running `tgg-melee` over to the newly chosen active set.
pub fn switch_set(app: &mut App) {
    run_or_prompt(app, PendingAction::SwitchSet);
}

/// Confirm leaving the current match to complete the pending action.
pub fn confirm_leave(app: &mut App) {
    let Some(action) = app.play.confirm.take() else {
        return;
    };
    if needs_relaunch(app, &action) {
        queue_relaunch(app, action);
        return;
    }
    if let Some(companion) = &app.companion {
        app.play.after_leave = Some(action);
        companion.send(Command::LeaveMatch);
        companion.send(Command::Focus);
    }
}

/// Keep playing the current match and dismiss the confirmation dialog.
pub fn cancel_confirm(app: &mut App) {
    app.play.confirm = None;
}

/// Leave the matchmaking queue without closing tgg-melee.
pub fn leave_queue(app: &mut App) {
    if let Some(companion) = &app.companion {
        companion.send(Command::StopSearch);
    }
}

/// Leave the current room without closing tgg-melee.
pub fn leave_room(app: &mut App) {
    if let Some(companion) = &app.companion {
        companion.send(Command::LeaveRoom);
    }
}

/// Bring the running tgg-melee window to the front.
pub fn focus_game(app: &mut App) {
    if let Some(companion) = &app.companion {
        companion.send(Command::Focus);
    }
}

/// Ask tgg-melee to quit cleanly (falls back to killing the process if
/// companion isn't connected).
pub fn quit_game(app: &mut App) {
    if let Some(companion) = &app.companion
        && companion.send(Command::Quit)
    {
        return;
    }
    crate::mods::actions::stop(app);
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

/// Host a room: if tgg-melee is already running, ask it to host; else launch.
pub fn host(app: &mut App) {
    run_or_prompt(app, PendingAction::Host);
}

/// Join the room whose code was typed, or whose invite was pasted.
pub fn join(app: &mut App) {
    let code = super::typed_code(&app.play.join.text);
    if code.is_empty() {
        return;
    }
    run_or_prompt(app, PendingAction::Join(code));
}

/// Join the room a `textures-gg://join/<code>` link names, with the active
/// set, or straight into the already-running game.
pub fn join_link(app: &mut App, code: &str) {
    app.play.friend = true;
    run_or_prompt(app, PendingAction::Join(code.to_owned()));
}

/// Copy the room's code, for pasting to a friend.
pub fn copy_code(app: &mut App, tx: &mut rune::Tx) {
    if let Some(live) = app.live_game()
        && !live.online.room.code.is_empty()
    {
        tx.copy(live.online.room.code);
    }
}

/// Copy the room's invite, to send a friend: the site's page for it,
/// which chat apps make clickable and which opens the app.
pub fn copy_link(app: &mut App, tx: &mut rune::Tx) {
    if let Some(live) = app.live_game()
        && let Some(invite) = live.online.room.invite()
    {
        tx.copy(invite);
    }
}

/// Copy the sign-in code the room waits on, to confirm on textures.gg.
pub fn copy_sign_in_code(app: &mut App, tx: &mut rune::Tx) {
    if let Some(live) = app.live_game()
        && !live.online.signin.code.is_empty()
    {
        tx.copy(live.online.signin.code);
    }
}

/// Open the page the room's sign-in code is confirmed on.
pub fn open_sign_in(app: &mut App, tx: &mut rune::Tx) {
    let Some(live) = app.live_game() else {
        return;
    };
    let url = live.online.signin.url;
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
