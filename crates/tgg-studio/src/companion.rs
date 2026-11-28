//! Two-way loopback TCP companion bridge between `tgg-studio` and a running
//! `tgg-melee` process (`TGG_COMPANION=127.0.0.1:<port>:<token>`).
//!
//! Protocol v2: Studio binds an ephemeral loopback port with a random
//! per-launch token; `tgg-melee` connects outbound on startup (and reconnects
//! automatically if it restarts via `tgg_restart_with`). `hello` advertises
//! host + mod capabilities (`caps`), state arrives as independent topic
//! snapshots (`session`, `online`, `match`), one-shot events (`match_ended`,
//! `replay_saved`) stream alongside snapshots, and commands return acks.

use serde::Deserialize;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long Studio holds onto the last `GettingMods` state while `tgg-melee`
/// replaces itself via `tgg_restart_with` and reconnects on the same port.
const RESTART_GRACE: Duration = Duration::from_secs(12);
/// Bound on buffered unread events or error acks if the UI is paused.
const MAX_QUEUED: usize = 32;

/// Live state reported by the running `tgg-melee` process.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveState {
    pub pid: u32,
    pub mod_set_id: String,
    /// Capabilities advertised by the game and its loaded mods in `hello`.
    pub caps: Vec<String>,
    pub activity: Activity,
    /// Active replay file path when `activity == Activity::Replay`, else empty.
    pub replay_path: String,
    pub online: Online,
    /// Live match telemetry while `tgg_in_match()` is active, else `None`.
    pub match_state: Option<LiveMatch>,
    /// When `online.state` last entered `Searching`, used to tick the timer
    /// locally from `online.search_ms` without per-second network traffic.
    pub updated_at: Option<Instant>,
}

impl LiveState {
    /// Whether the connected game advertised capability `cap` in `hello`.
    pub fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|c| c == cap)
    }

    /// Elapsed search seconds while `online.state == OnlineState::Searching`.
    pub fn search_secs(&self) -> u64 {
        let base_ms = self.online.search_ms;
        let extra_ms = self
            .updated_at
            .map(|at| at.elapsed().as_millis() as u64)
            .unwrap_or(0);
        (base_ms + extra_ms) / 1000
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Menu,
    Match,
    Training,
    Replay,
}

impl Activity {
    fn parse(s: &str) -> Self {
        match s {
            "match" => Self::Match,
            "training" => Self::Training,
            "replay" => Self::Replay,
            _ => Self::Menu,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnlineState {
    #[default]
    Idle,
    Connecting,
    SigningIn,
    Choosing,
    Searching,
    Matched,
    RoomWaiting,
    RoomPlaying,
    GettingMods,
    Ended,
}

impl OnlineState {
    fn parse(s: &str) -> Self {
        match s {
            "connecting" => Self::Connecting,
            "signing-in" => Self::SigningIn,
            "choosing" => Self::Choosing,
            "searching" => Self::Searching,
            "matched" => Self::Matched,
            "room-waiting" => Self::RoomWaiting,
            "room-playing" => Self::RoomPlaying,
            "getting-mods" => Self::GettingMods,
            "ended" => Self::Ended,
            _ => Self::Idle,
        }
    }

    pub fn is_queue(self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::SigningIn | Self::Choosing | Self::Searching | Self::Matched
        )
    }

    pub fn is_room(self) -> bool {
        matches!(
            self,
            Self::RoomWaiting | Self::RoomPlaying | Self::GettingMods
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Online {
    pub state: OnlineState,
    pub queue_label: String,
    pub search_ms: u64,
    /// Other players in a matched online session.
    pub players: Vec<String>,
    pub room: RoomInfo,
    pub signin: SigninInfo,
    pub why: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomInfo {
    pub code: String,
    pub link: String,
    pub host: String,
    pub players: Vec<String>,
    pub max: u32,
}

impl RoomInfo {
    pub fn count(&self) -> String {
        let players = self.players.len();
        match self.max {
            0 => players.to_string(),
            max => format!("{players} of {max} players"),
        }
    }

    pub fn invite(&self) -> Option<String> {
        match crate::links::parse(&self.link)? {
            crate::links::Link::Join(code) => Some(crate::links::invite(&code)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct SigninInfo {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub code: String,
}

/// Live match snapshot pushed by `tgg-melee` while in a match, training, or
/// replay (`topic: "match"`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveMatch {
    pub stage: String,
    pub stage_name: String,
    pub stage_id: Option<u16>,
    pub match_kind: String,
    pub timer_secs: Option<u32>,
    pub time_limit: u32,
    pub teams: bool,
    pub paused: bool,
    pub players: Vec<LivePlayer>,
}

impl LiveMatch {
    /// Human-readable stage name (prefers the game's registry name, falls back
    /// to vanilla stage id or raw content id).
    pub fn display_stage(&self) -> String {
        if !self.stage_name.is_empty() {
            return self.stage_name.clone();
        }
        if let Some(name) = self.stage_id.and_then(tgg_app::replays::stage_name) {
            return name.to_owned();
        }
        self.stage.clone()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LivePlayer {
    pub port: u8,
    pub kind: String,
    pub character: String,
    pub character_id: Option<u8>,
    pub costume: u8,
    pub stocks: u8,
    pub percent: u32,
    pub team: u8,
}

impl LivePlayer {
    pub fn display_character(&self) -> String {
        if let Some(ch) = self.character_id.and_then(tgg_app::replays::character) {
            return ch.name().to_owned();
        }
        if let Some((_, slug)) = self.character.split_once('/') {
            return slug.to_owned();
        }
        format!("P{}", self.port)
    }
}

/// One-shot events streamed from the running game (`{"type":"event",...}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    MatchEnded(Option<LiveMatch>),
    ReplaySaved { path: String, frames: u32 },
}

/// Commands Studio can send to the running `tgg-melee` process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Focus,
    Quit,
    LeaveMatch,
    StartSearch { mode: String },
    StopSearch,
    HostRoom,
    JoinRoom { code: String },
    LeaveRoom,
    LoadReplay { path: String },
}

impl Command {
    fn to_json(&self, id: u32) -> String {
        match self {
            Self::Focus => format!("{{\"id\":{id},\"cmd\":\"focus\"}}"),
            Self::Quit => format!("{{\"id\":{id},\"cmd\":\"quit\"}}"),
            Self::LeaveMatch => format!("{{\"id\":{id},\"cmd\":\"leave_match\"}}"),
            Self::StartSearch { mode } => {
                format!(
                    "{{\"id\":{id},\"cmd\":\"start_search\",\"mode\":{}}}",
                    serde_json::to_string(mode).unwrap_or_else(|_| "\"vs\"".to_owned())
                )
            }
            Self::StopSearch => format!("{{\"id\":{id},\"cmd\":\"stop_search\"}}"),
            Self::HostRoom => format!("{{\"id\":{id},\"cmd\":\"host_room\"}}"),
            Self::JoinRoom { code } => {
                format!(
                    "{{\"id\":{id},\"cmd\":\"join_room\",\"code\":{}}}",
                    serde_json::to_string(code).unwrap_or_else(|_| "\"\"".to_owned())
                )
            }
            Self::LeaveRoom => format!("{{\"id\":{id},\"cmd\":\"leave_room\"}}"),
            Self::LoadReplay { path } => {
                format!(
                    "{{\"id\":{id},\"cmd\":\"load_replay\",\"path\":{}}}",
                    serde_json::to_string(path).unwrap_or_else(|_| "\"\"".to_owned())
                )
            }
        }
    }
}

/// Command acknowledgment from the game, queued when `ok == false` so a
/// following `ok: true` ack (e.g. `Focus`) never overwrites an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ack {
    pub id: u32,
    pub ok: bool,
    pub reason: String,
}

/// Loopback TCP server owned by `App` for the lifetime of a launched game.
pub struct Companion {
    port: u16,
    token: String,
    stopped: Arc<AtomicBool>,
    next_id: AtomicU32,
    writer: Arc<Mutex<Option<TcpStream>>>,
    state: Arc<Mutex<Option<LiveState>>>,
    error_acks: Arc<Mutex<VecDeque<Ack>>>,
    events: Arc<Mutex<VecDeque<Event>>>,
    restarting_until: Arc<Mutex<Option<Instant>>>,
}

impl Companion {
    /// Start listening on an ephemeral loopback port.
    pub fn start(waker: Option<Arc<dyn Fn() + Send + Sync>>) -> Result<Self, String> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| format!("companion bind: {e}"))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("companion addr: {e}"))?
            .port();
        let token = make_token();
        let stopped = Arc::new(AtomicBool::new(false));
        let writer: Arc<Mutex<Option<TcpStream>>> = Arc::new(Mutex::new(None));
        let state: Arc<Mutex<Option<LiveState>>> = Arc::new(Mutex::new(None));
        let error_acks: Arc<Mutex<VecDeque<Ack>>> = Arc::new(Mutex::new(VecDeque::new()));
        let events: Arc<Mutex<VecDeque<Event>>> = Arc::new(Mutex::new(VecDeque::new()));
        let restarting_until: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        let t_stopped = stopped.clone();
        let t_token = token.clone();
        let t_writer = writer.clone();
        let t_state = state.clone();
        let t_acks = error_acks.clone();
        let t_events = events.clone();
        let t_restart = restarting_until.clone();

        std::thread::Builder::new()
            .name("companion".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if t_stopped.load(Ordering::Relaxed) {
                        break;
                    }
                    handle_client(
                        stream,
                        &t_token,
                        &t_stopped,
                        &t_writer,
                        &t_state,
                        &t_acks,
                        &t_events,
                        &t_restart,
                        waker.as_deref(),
                    );
                }
            })
            .map_err(|e| format!("companion thread: {e}"))?;

        Ok(Self {
            port,
            token,
            stopped,
            next_id: AtomicU32::new(1),
            writer,
            state,
            error_acks,
            events,
            restarting_until,
        })
    }

    /// Environment variable to pass to `tgg-melee`.
    pub fn env_var(&self) -> (String, OsString) {
        (
            "TGG_COMPANION".to_owned(),
            format!("127.0.0.1:{}:{}", self.port, self.token).into(),
        )
    }

    /// Whether a `tgg_restart_with` handoff is currently within its grace window.
    pub fn is_restarting(&self) -> bool {
        let Ok(mut guard) = self.restarting_until.lock() else {
            return false;
        };
        if let Some(deadline) = *guard {
            if Instant::now() < deadline {
                return true;
            }
            *guard = None;
            if let Ok(mut s) = self.state.lock()
                && self.writer.lock().ok().is_none_or(|w| w.is_none())
            {
                *s = None;
            }
        }
        false
    }

    /// True if a `tgg-melee` process is currently connected or mid-restart handoff.
    pub fn has_live_connection(&self) -> bool {
        if self.stopped.load(Ordering::Relaxed) {
            return false;
        }
        self.writer.lock().ok().is_some_and(|w| w.is_some()) || self.is_restarting()
    }

    /// Block on a worker thread until a restarted `tgg-melee` disconnects (or
    /// the restart grace window expires / `stop` is requested).
    pub fn wait_until_disconnected(&self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) && self.has_live_connection() {
            std::thread::sleep(Duration::from_millis(200));
        }
        if stop.load(Ordering::Relaxed) {
            let _ = self.send(Command::Quit);
        }
    }

    /// Current live state snapshot, if a game is connected (or mid-restart handoff).
    pub fn snapshot(&self) -> Option<LiveState> {
        let _ = self.is_restarting();
        self.state.lock().ok()?.clone()
    }

    /// Pop the oldest failed command ack, if any.
    pub fn take_error_ack(&self) -> Option<Ack> {
        self.error_acks.lock().ok()?.pop_front()
    }

    /// Drain all pending one-shot events received since the last call.
    pub fn drain_events(&self) -> Vec<Event> {
        let Ok(mut guard) = self.events.lock() else {
            return Vec::new();
        };
        guard.drain(..).collect()
    }

    /// Send a command to the connected game. Returns `true` if written.
    pub fn send(&self, cmd: Command) -> bool {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let line = cmd.to_json(id);
        let Ok(mut guard) = self.writer.lock() else {
            return false;
        };
        let Some(stream) = guard.as_mut() else {
            return false;
        };
        writeln!(stream, "{line}").is_ok() && stream.flush().is_ok()
    }

    /// Shut down the server and drop any active connection.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.restarting_until.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.writer.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.state.lock() {
            *guard = None;
        }
        // Wake the blocking `accept()` so the listener thread exits promptly.
        let _ = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port));
    }
}

impl Drop for Companion {
    fn drop(&mut self) {
        self.stop();
    }
}

fn make_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let a = RandomState::new().build_hasher().finish();
    let b = RandomState::new().build_hasher().finish();
    format!("{a:016x}{b:016x}")
}

#[derive(Deserialize)]
struct WireMsg {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    v: u32,
    #[serde(default)]
    token: String,
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    mod_set_id: String,
    #[serde(default)]
    caps: Vec<String>,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    id: u32,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    reason: String,
}

#[derive(Default, Deserialize)]
struct WireSession {
    #[serde(default)]
    activity: String,
    #[serde(default)]
    mod_set_id: String,
    #[serde(default)]
    replay_path: String,
}

#[derive(Default, Deserialize)]
struct WireOnline {
    #[serde(default)]
    state: String,
    #[serde(default)]
    queue_label: String,
    #[serde(default)]
    search_ms: u64,
    #[serde(default)]
    players: Vec<String>,
    #[serde(default)]
    room: WireRoom,
    #[serde(default)]
    signin: SigninInfo,
    #[serde(default)]
    why: String,
}

#[derive(Default, Deserialize)]
struct WireRoom {
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
}

#[derive(Default, Deserialize)]
struct WireMatch {
    #[serde(default)]
    stage: String,
    #[serde(default)]
    stage_name: String,
    #[serde(default)]
    stage_id: i32,
    #[serde(default)]
    match_kind: String,
    #[serde(default)]
    timer_secs: Option<u32>,
    #[serde(default)]
    time_limit: u32,
    #[serde(default)]
    teams: bool,
    #[serde(default)]
    paused: bool,
    #[serde(default)]
    players: Vec<WirePlayer>,
}

#[derive(Default, Deserialize)]
struct WirePlayer {
    #[serde(default)]
    port: u8,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    character: String,
    #[serde(default)]
    character_id: i32,
    #[serde(default)]
    costume: u8,
    #[serde(default)]
    stocks: u8,
    #[serde(default)]
    percent: u32,
    #[serde(default)]
    team: u8,
}

#[derive(Default, Deserialize)]
struct WireReplaySaved {
    #[serde(default)]
    path: String,
    #[serde(default)]
    frames: u32,
}

impl From<WireMatch> for LiveMatch {
    fn from(m: WireMatch) -> Self {
        Self {
            stage: m.stage,
            stage_name: m.stage_name,
            stage_id: u16::try_from(m.stage_id).ok(),
            match_kind: m.match_kind,
            timer_secs: m.timer_secs,
            time_limit: m.time_limit,
            teams: m.teams,
            paused: m.paused,
            players: m
                .players
                .into_iter()
                .map(|p| LivePlayer {
                    port: p.port,
                    kind: p.kind,
                    character: p.character,
                    character_id: u8::try_from(p.character_id).ok(),
                    costume: p.costume,
                    stocks: p.stocks,
                    percent: p.percent,
                    team: p.team,
                })
                .collect(),
        }
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, item: T) {
    if queue.len() >= MAX_QUEUED {
        queue.pop_front();
    }
    queue.push_back(item);
}

#[allow(clippy::too_many_arguments)]
fn handle_client(
    stream: TcpStream,
    expected_token: &str,
    stopped: &AtomicBool,
    writer: &Mutex<Option<TcpStream>>,
    state: &Mutex<Option<LiveState>>,
    error_acks: &Mutex<VecDeque<Ack>>,
    events: &Mutex<VecDeque<Event>>,
    restarting_until: &Mutex<Option<Instant>>,
    waker: Option<&(dyn Fn() + Send + Sync)>,
) {
    let _ = stream.set_nodelay(true);
    let Ok(write_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut authed = false;
    let mut restarting = false;

    loop {
        if stopped.load(Ordering::Relaxed) {
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(msg) = serde_json::from_str::<WireMsg>(line.trim()) else {
            continue;
        };
        if !authed {
            if msg.kind == "hello" && msg.v == 2 && msg.token == expected_token {
                authed = true;
                if let Ok(mut r) = restarting_until.lock() {
                    *r = None;
                }
                if let Ok(mut w) = writer.lock() {
                    *w = Some(write_half.try_clone().unwrap_or_else(|_| {
                        panic!("clone tcp stream");
                    }));
                }
                if let Ok(mut s) = state.lock() {
                    *s = Some(LiveState {
                        pid: msg.pid,
                        mod_set_id: msg.mod_set_id,
                        caps: msg.caps,
                        activity: Activity::Menu,
                        replay_path: String::new(),
                        online: Online::default(),
                        match_state: None,
                        updated_at: Some(Instant::now()),
                    });
                }
                if let Some(wake) = waker {
                    wake();
                }
            } else {
                break;
            }
            continue;
        }
        match msg.kind.as_str() {
            "restarting" => {
                restarting = true;
                if let Ok(mut r) = restarting_until.lock() {
                    *r = Some(Instant::now() + RESTART_GRACE);
                }
                if let Ok(mut s) = state.lock()
                    && let Some(live) = s.as_mut()
                {
                    live.online.state = OnlineState::GettingMods;
                    live.match_state = None;
                }
                if let Some(wake) = waker {
                    wake();
                }
                break;
            }
            "state" => {
                let mut changed = false;
                if let Ok(mut s) = state.lock()
                    && let Some(live) = s.as_mut()
                {
                    match msg.topic.as_str() {
                        "session" => {
                            if let Ok(sess) = serde_json::from_value::<WireSession>(msg.data) {
                                live.activity = Activity::parse(&sess.activity);
                                live.mod_set_id = sess.mod_set_id;
                                live.replay_path = sess.replay_path;
                                changed = true;
                            }
                        }
                        "online" => {
                            if let Ok(online) = serde_json::from_value::<WireOnline>(msg.data) {
                                live.online = Online {
                                    state: OnlineState::parse(&online.state),
                                    queue_label: online.queue_label,
                                    search_ms: online.search_ms,
                                    players: online.players,
                                    room: RoomInfo {
                                        code: online.room.code,
                                        link: online.room.link,
                                        host: online.room.host,
                                        players: online.room.players,
                                        max: online.room.max,
                                    },
                                    signin: online.signin,
                                    why: online.why,
                                };
                                live.updated_at = Some(Instant::now());
                                changed = true;
                            }
                        }
                        "match" => {
                            if msg.data.is_null() {
                                live.match_state = None;
                                changed = true;
                            } else if let Ok(m) = serde_json::from_value::<WireMatch>(msg.data) {
                                live.match_state = Some(m.into());
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
                if changed && let Some(wake) = waker {
                    wake();
                }
            }
            "event" => {
                let ev = match msg.name.as_str() {
                    "match_ended" => {
                        let summary = if msg.data.is_object()
                            && msg.data.as_object().is_some_and(|o| !o.is_empty())
                        {
                            serde_json::from_value::<WireMatch>(msg.data)
                                .ok()
                                .map(LiveMatch::from)
                        } else {
                            None
                        };
                        Some(Event::MatchEnded(summary))
                    }
                    "replay_saved" => serde_json::from_value::<WireReplaySaved>(msg.data)
                        .ok()
                        .filter(|r| !r.path.is_empty())
                        .map(|r| Event::ReplaySaved {
                            path: r.path,
                            frames: r.frames,
                        }),
                    _ => None,
                };
                if let Some(ev) = ev {
                    if let Ok(mut q) = events.lock() {
                        push_bounded(&mut q, ev);
                    }
                    if let Some(wake) = waker {
                        wake();
                    }
                }
            }
            "ack" if !msg.ok => {
                if let Ok(mut a) = error_acks.lock() {
                    push_bounded(
                        &mut a,
                        Ack {
                            id: msg.id,
                            ok: false,
                            reason: msg.reason,
                        },
                    );
                }
                if let Some(wake) = waker {
                    wake();
                }
            }
            _ => {}
        }
    }

    if authed {
        if let Ok(mut w) = writer.lock() {
            *w = None;
        }
        // If the game signaled a restart or dropped while getting mods, keep the
        // last snapshot alive across the `tgg_restart_with` handoff window.
        let keep_for_restart = restarting
            || state.lock().ok().is_some_and(|s| {
                s.as_ref()
                    .is_some_and(|l| l.online.state == OnlineState::GettingMods)
            });
        if keep_for_restart && !stopped.load(Ordering::Relaxed) {
            if let Ok(mut r) = restarting_until.lock() {
                *r = Some(Instant::now() + RESTART_GRACE);
            }
        } else if let Ok(mut s) = state.lock() {
            *s = None;
        }
        if let Some(wake) = waker {
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn handshake_topics_events_and_error_ack_queue() {
        let server = Companion::start(None).expect("start companion");
        let (key, val) = server.env_var();
        assert_eq!(key, "TGG_COMPANION");
        let val = val.to_string_lossy();
        let parts: Vec<&str> = val.split(':').collect();
        assert_eq!(parts.len(), 3);
        let port: u16 = parts[1].parse().unwrap();
        let token = parts[2];

        let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(
            client,
            "{{\"type\":\"hello\",\"v\":2,\"token\":\"{token}\",\"pid\":42,\"mod_set_id\":\"abc\",\"caps\":[\"match\",\"events\",\"online\",\"replay\"]}}"
        )
        .unwrap();
        writeln!(
            client,
            "{{\"type\":\"state\",\"topic\":\"online\",\"data\":{{\"state\":\"searching\",\"queue_label\":\"Competitive\",\"search_ms\":5000}}}}"
        )
        .unwrap();
        writeln!(
            client,
            "{{\"type\":\"state\",\"topic\":\"match\",\"data\":{{\"stage\":\"melee/battlefield\",\"stage_name\":\"Battlefield\",\"stage_id\":31,\"match_kind\":\"stock\",\"timer_secs\":420,\"time_limit\":480,\"teams\":false,\"paused\":false,\"players\":[{{\"port\":1,\"kind\":\"human\",\"character\":\"melee/fox\",\"character_id\":2,\"costume\":0,\"stocks\":3,\"percent\":48,\"team\":0}}]}}}}"
        )
        .unwrap();

        let mut got = None;
        for _ in 0..40 {
            if let Some(s) = server.snapshot()
                && s.online.state == OnlineState::Searching
                && s.match_state.is_some()
            {
                got = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let s = got.expect("received searching + match state");
        assert_eq!(s.pid, 42);
        assert!(s.has_cap("online"));
        assert!(s.has_cap("replay"));
        assert_eq!(s.online.queue_label, "Competitive");
        assert!(s.search_secs() >= 5);
        let m = s.match_state.expect("live match");
        assert_eq!(m.display_stage(), "Battlefield");
        assert_eq!(m.timer_secs, Some(420));
        assert_eq!(m.players.len(), 1);
        assert_eq!(m.players[0].display_character(), "Fox");
        assert_eq!(m.players[0].stocks, 3);
        assert_eq!(m.players[0].percent, 48);

        // Failed ack followed immediately by ok: true ack (e.g. Focus) MUST preserve the error!
        writeln!(
            client,
            "{{\"type\":\"ack\",\"id\":1,\"ok\":false,\"reason\":\"in-match\"}}"
        )
        .unwrap();
        writeln!(client, "{{\"type\":\"ack\",\"id\":2,\"ok\":true}}").unwrap();
        writeln!(
            client,
            "{{\"type\":\"event\",\"name\":\"replay_saved\",\"data\":{{\"path\":\"/tmp/new.slp\",\"frames\":3600}}}}"
        )
        .unwrap();
        writeln!(
            client,
            "{{\"type\":\"state\",\"topic\":\"match\",\"data\":null}}"
        )
        .unwrap();

        let mut cleared = false;
        for _ in 0..40 {
            if server.snapshot().is_some_and(|s| s.match_state.is_none()) {
                cleared = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(cleared, "match_state cleared on null");
        let err = server.take_error_ack().expect("error ack preserved");
        assert_eq!(err.id, 1);
        assert_eq!(err.reason, "in-match");
        assert!(server.take_error_ack().is_none());

        let events = server.drain_events();
        assert_eq!(
            events,
            vec![Event::ReplaySaved {
                path: "/tmp/new.slp".to_owned(),
                frames: 3600,
            }]
        );

        assert!(server.send(Command::LoadReplay {
            path: "/tmp/next.slp".to_owned()
        }));
        let mut line = String::new();
        BufReader::new(&client).read_line(&mut line).unwrap();
        assert!(line.contains("\"cmd\":\"load_replay\""));
        assert!(line.contains("\"/tmp/next.slp\""));
    }

    #[test]
    fn restart_handoff_preserves_state_across_reconnect() {
        let server = Companion::start(None).expect("start companion");
        let (_, val) = server.env_var();
        let val = val.to_string_lossy();
        let parts: Vec<&str> = val.split(':').collect();
        let port: u16 = parts[1].parse().unwrap();
        let token = parts[2];

        // First process connects, reports getting-mods, signals restarting, and drops.
        {
            let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect 1");
            writeln!(
                client,
                "{{\"type\":\"hello\",\"v\":2,\"token\":\"{token}\",\"pid\":100,\"mod_set_id\":\"old\",\"caps\":[\"match\",\"events\",\"online\"]}}"
            )
            .unwrap();
            writeln!(
                client,
                "{{\"type\":\"state\",\"topic\":\"online\",\"data\":{{\"state\":\"getting-mods\",\"room\":{{\"code\":\"KFXD\",\"host\":\"saavy\"}}}}}}"
            )
            .unwrap();
            writeln!(client, "{{\"type\":\"restarting\"}}").unwrap();
            client.flush().unwrap();
        }

        // Wait until the server enters restarting grace; snapshot MUST stay GettingMods!
        for _ in 0..40 {
            if server.is_restarting() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(server.is_restarting());
        assert!(server.has_live_connection());
        let held = server.snapshot().expect("state preserved during restart");
        assert_eq!(held.online.state, OnlineState::GettingMods);
        assert_eq!(held.online.room.host, "saavy");

        // Replacement process connects back on the same port with new PID & mod set.
        let mut next = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("connect 2");
        writeln!(
            next,
            "{{\"type\":\"hello\",\"v\":2,\"token\":\"{token}\",\"pid\":200,\"mod_set_id\":\"host_set\",\"caps\":[\"match\",\"events\",\"online\"]}}"
        )
        .unwrap();
        writeln!(
            next,
            "{{\"type\":\"state\",\"topic\":\"online\",\"data\":{{\"state\":\"room-waiting\",\"room\":{{\"code\":\"KFXD\",\"host\":\"saavy\",\"players\":[\"saavy\",\"me\"],\"max\":4}}}}}}"
        )
        .unwrap();

        let mut reconnected = None;
        for _ in 0..40 {
            if let Some(s) = server.snapshot()
                && s.pid == 200
                && s.online.state == OnlineState::RoomWaiting
            {
                reconnected = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let s = reconnected.expect("reconnected process state");
        assert!(!server.is_restarting());
        assert_eq!(s.mod_set_id, "host_set");
        assert_eq!(s.online.room.players, vec!["saavy", "me"]);
    }
}
