//! Two-way loopback TCP companion bridge between `tgg-studio` and a running
//! `tgg-melee` process (`TGG_COMPANION=127.0.0.1:<port>:<token>`).
//!
//! Studio binds an ephemeral loopback port with a random per-launch token;
//! `tgg-melee` connects outbound on startup (and reconnects automatically if it
//! restarts via `tgg_restart_with`). The game pushes full state snapshots on
//! every change, and Studio sends commands (`focus`, `quit`, `start_search`,
//! `stop_search`, `host_room`, `join_room`, `leave_room`) with acks.

use serde::Deserialize;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long Studio holds onto the last `GettingMods` state while `tgg-melee`
/// replaces itself via `tgg_restart_with` and reconnects on the same port.
const RESTART_GRACE: Duration = Duration::from_secs(12);

/// Live state reported by the running `tgg-melee` process.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveState {
    pub pid: u32,
    pub mod_set_id: String,
    pub activity: Activity,
    pub online: Online,
    /// When `online.state` last entered `Searching`, used to tick the timer
    /// locally from `online.search_ms` without per-second network traffic.
    pub updated_at: Option<Instant>,
}

impl LiveState {
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
    pub opponent: String,
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
        }
    }
}

/// Last command acknowledgment from the game, surfaced if a command was refused
/// (e.g. `"in-match"`).
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
    last_ack: Arc<Mutex<Option<Ack>>>,
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
        let last_ack: Arc<Mutex<Option<Ack>>> = Arc::new(Mutex::new(None));
        let restarting_until: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        let t_stopped = stopped.clone();
        let t_token = token.clone();
        let t_writer = writer.clone();
        let t_state = state.clone();
        let t_ack = last_ack.clone();
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
                        &t_ack,
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
            last_ack,
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

    /// Take the latest failed command ack, if any.
    pub fn take_error_ack(&self) -> Option<Ack> {
        let mut guard = self.last_ack.lock().ok()?;
        if guard.as_ref().is_some_and(|ack| !ack.ok) {
            guard.take()
        } else {
            None
        }
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
    token: String,
    #[serde(default)]
    pid: u32,
    #[serde(default)]
    mod_set_id: String,
    #[serde(default)]
    activity: String,
    #[serde(default)]
    online: WireOnline,
    #[serde(default)]
    id: u32,
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    reason: String,
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
    opponent: String,
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

#[allow(clippy::too_many_arguments)]
fn handle_client(
    stream: TcpStream,
    expected_token: &str,
    stopped: &AtomicBool,
    writer: &Mutex<Option<TcpStream>>,
    state: &Mutex<Option<LiveState>>,
    last_ack: &Mutex<Option<Ack>>,
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
    let mut pid = 0u32;

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
            if msg.kind == "hello" && msg.token == expected_token {
                authed = true;
                pid = msg.pid;
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
                        pid,
                        mod_set_id: msg.mod_set_id,
                        activity: Activity::Menu,
                        online: Online::default(),
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
                }
                if let Some(wake) = waker {
                    wake();
                }
                break;
            }
            "state" => {
                let next = LiveState {
                    pid,
                    mod_set_id: msg.mod_set_id,
                    activity: Activity::parse(&msg.activity),
                    online: Online {
                        state: OnlineState::parse(&msg.online.state),
                        queue_label: msg.online.queue_label,
                        search_ms: msg.online.search_ms,
                        opponent: msg.online.opponent,
                        room: RoomInfo {
                            code: msg.online.room.code,
                            link: msg.online.room.link,
                            host: msg.online.room.host,
                            players: msg.online.room.players,
                            max: msg.online.room.max,
                        },
                        signin: msg.online.signin,
                        why: msg.online.why,
                    },
                    updated_at: Some(Instant::now()),
                };
                if let Ok(mut s) = state.lock() {
                    *s = Some(next);
                }
                if let Some(wake) = waker {
                    wake();
                }
            }
            "ack" => {
                if let Ok(mut a) = last_ack.lock() {
                    *a = Some(Ack {
                        id: msg.id,
                        ok: msg.ok,
                        reason: msg.reason,
                    });
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
    fn handshake_state_and_command_roundtrip() {
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
            "{{\"type\":\"hello\",\"v\":1,\"token\":\"{token}\",\"pid\":42,\"mod_set_id\":\"abc\"}}"
        )
        .unwrap();
        writeln!(
            client,
            "{{\"type\":\"state\",\"activity\":\"menu\",\"mod_set_id\":\"abc\",\"online\":{{\"state\":\"searching\",\"queue_label\":\"Competitive\",\"search_ms\":5000}}}}"
        )
        .unwrap();

        let mut got = None;
        for _ in 0..40 {
            if let Some(s) = server.snapshot()
                && s.online.state == OnlineState::Searching
            {
                got = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let s = got.expect("received searching state");
        assert_eq!(s.pid, 42);
        assert_eq!(s.online.queue_label, "Competitive");
        assert!(s.search_secs() >= 5);

        assert!(server.send(Command::StopSearch));
        let mut line = String::new();
        BufReader::new(&client).read_line(&mut line).unwrap();
        assert!(line.contains("\"cmd\":\"stop_search\""));
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
                "{{\"type\":\"hello\",\"v\":1,\"token\":\"{token}\",\"pid\":100,\"mod_set_id\":\"old\"}}"
            )
            .unwrap();
            writeln!(
                client,
                "{{\"type\":\"state\",\"activity\":\"menu\",\"mod_set_id\":\"old\",\"online\":{{\"state\":\"getting-mods\",\"room\":{{\"code\":\"KFXD\",\"host\":\"saavy\"}}}}}}"
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
            "{{\"type\":\"hello\",\"v\":1,\"token\":\"{token}\",\"pid\":200,\"mod_set_id\":\"host_set\"}}"
        )
        .unwrap();
        writeln!(
            next,
            "{{\"type\":\"state\",\"activity\":\"menu\",\"mod_set_id\":\"host_set\",\"online\":{{\"state\":\"room-waiting\",\"room\":{{\"code\":\"KFXD\",\"host\":\"saavy\",\"players\":[\"saavy\",\"me\"],\"max\":4}}}}}}"
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
