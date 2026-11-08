//! Reading what a profile needs from a Slippi replay (`.slp`), and no more:
//! its `metadata`, and from the start of its events the Game Start and the
//! gecko code list. A replay list reads one per row, so a read touches the
//! file's head and its tail, never the frames between.
//!
//! The file is UBJSON (project-slippi's SPEC.md): an object whose `raw`
//! element is a byte array of the console's events, its length written once
//! the game ends (0 while it is being written), and whose `metadata` element
//! follows it. The events start with Event Payloads (`0x35`), which gives
//! every event's payload size; then Game Start (`0x36`), the gecko code list
//! in Message Splitter events (`0x10` with internal command `0x3D`), and the
//! frames.

use serde_json::{Map, Number, Value};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// The start every replay has: `{`, the key `raw`, and a typed, counted
/// byte array whose count follows as a big-endian `i32`.
const HEAD: &[u8] = b"{U\x03raw[$U#l";
/// The longest `metadata` read.
const METADATA_LIMIT: u64 = 1024 * 1024;

const EVENT_PAYLOADS: u8 = 0x35;
const GAME_START: u8 = 0x36;
const MESSAGE_SPLITTER: u8 = 0x10;
const GECKO_LIST: u8 = 0x3D;
/// Events that come once frames start: reading stops at the first.
const FRAME_EVENTS: [u8; 4] = [0x37, 0x38, 0x3A, 0x39];
/// Pokémon Stadium changing form.
const TRANSFORMATION: u8 = 0x41;
/// Pokémon Stadium's stage id.
pub(crate) const POKEMON_STADIUM: u16 = 3;

/// What a replay holds that profiles read.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Replay {
    /// The `metadata` element, as JSON; none while the replay is being
    /// written or when it has none.
    pub metadata: Option<Value>,
    /// From Game Start, when the events have one.
    pub start: Option<Start>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Start {
    /// The Slippi version that recorded it: major, minor, build.
    pub version: [u8; 3],
    pub stage: u16,
    /// Slippi's frozen Pokémon Stadium flag.
    pub frozen_stadium: bool,
    /// Every present player's dashback and shield-drop fixes are off.
    pub ucf_off: bool,
    /// The address of each code the console ran, as `0x` and eight
    /// upper-case hex digits, sorted, without repeats.
    pub codes: Vec<String>,
}

impl Start {
    pub fn version(&self) -> String {
        let [major, minor, build] = self.version;
        format!("{major}.{minor}.{build}")
    }
}

/// Why a file isn't a replay that can be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SlpError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("it isn't a Slippi replay")]
    NotReplay,
    #[error("its metadata is damaged: {0}")]
    Metadata(String),
}

/// Read `path`'s metadata and, unless `skip_events` says the metadata is
/// enough, its Game Start and gecko list.
pub(crate) fn read(path: &Path, skip_events: impl Fn(&Value) -> bool) -> Result<Replay, SlpError> {
    let mut file = BufReader::new(File::open(path)?);
    let len = file.get_ref().metadata()?.len();
    let mut head = [0u8; 15];
    if len < 15 || file.read_exact(&mut head).is_err() || &head[..11] != HEAD {
        return Err(SlpError::NotReplay);
    }
    let raw = u64::from(u32::from_be_bytes(
        head[11..15].try_into().expect("4 bytes"),
    ));
    let events_end = if raw == 0 || 15 + raw > len {
        len
    } else {
        15 + raw
    };
    let mut replay = Replay::default();
    // The metadata follows the raw array, once its length is known.
    if raw != 0 && 15 + raw < len && len - (15 + raw) <= METADATA_LIMIT {
        file.seek(SeekFrom::Start(15 + raw))?;
        let mut tail = Vec::new();
        file.by_ref().take(METADATA_LIMIT).read_to_end(&mut tail)?;
        replay.metadata = metadata(&tail).map_err(SlpError::Metadata)?;
        if replay.metadata.as_ref().is_some_and(&skip_events) {
            return Ok(replay);
        }
    }
    file.seek(SeekFrom::Start(15))?;
    replay.start = start(&mut file.take(events_end - 15))?;
    Ok(replay)
}

/// The `metadata` element in the bytes after the raw array: none when the
/// file ends without one.
fn metadata(tail: &[u8]) -> Result<Option<Value>, String> {
    let mut reader = Ubjson { bytes: tail, at: 0 };
    while reader.at < tail.len() {
        if tail[reader.at] == b'}' {
            return Ok(None);
        }
        let key = reader.key()?;
        let value = reader.value(0)?;
        if key == "metadata" {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Game Start and the gecko list, from the events in `events`.
fn start(events: &mut impl Read) -> Result<Option<Start>, SlpError> {
    let mut byte = [0u8; 1];
    if events.read_exact(&mut byte).is_err() || byte[0] != EVENT_PAYLOADS {
        return Ok(None);
    }
    if events.read_exact(&mut byte).is_err() {
        return Ok(None);
    }
    let mut table = vec![0u8; usize::from(byte[0]).saturating_sub(1)];
    if events.read_exact(&mut table).is_err() {
        return Ok(None);
    }
    let mut sizes: [Option<u16>; 256] = [None; 256];
    for entry in table.as_chunks::<3>().0 {
        sizes[usize::from(entry[0])] = Some(u16::from_be_bytes([entry[1], entry[2]]));
    }
    let mut start: Option<Start> = None;
    let mut gecko = Vec::new();
    let mut payload = Vec::new();
    loop {
        if events.read_exact(&mut byte).is_err() {
            break;
        }
        let command = byte[0];
        let Some(size) = sizes[usize::from(command)] else {
            break;
        };
        if FRAME_EVENTS.contains(&command) {
            // Slippi 3.18 and later record Stadium's transformations
            // (0x41) but not always the frozen flag: a Stadium game that
            // never transforms was frozen. Telling takes reading on until
            // the first transformation, which comes early in a game that
            // has them, and the whole file in one that doesn't.
            if let Some(start) = &mut start
                && start.stage == POKEMON_STADIUM
                && !start.frozen_stadium
                && sizes[usize::from(TRANSFORMATION)].is_some()
            {
                start.frozen_stadium = !transforms(events, &sizes, size)?;
            }
            break;
        }
        // The payload with its command byte first, so offsets read as the
        // spec gives them.
        payload.clear();
        payload.push(command);
        payload.resize(1 + usize::from(size), 0);
        if events.read_exact(&mut payload[1..]).is_err() {
            break;
        }
        match command {
            GAME_START => start = Some(game_start(&payload)),
            MESSAGE_SPLITTER if payload.len() >= 0x204 && payload[0x203] == GECKO_LIST => {
                let part = usize::from(u16::from_be_bytes([payload[0x201], payload[0x202]]));
                gecko.extend_from_slice(&payload[1..1 + part.min(0x200)]);
            }
            _ => {}
        }
    }
    if let Some(start) = &mut start {
        start.codes = codes(&gecko);
    }
    Ok(start)
}

/// Whether a Stadium transformation event (0x41) follows, the reader being
/// just past the command byte of an event with a `size`-byte payload.
fn transforms(
    events: &mut impl Read,
    sizes: &[Option<u16>; 256],
    mut size: u16,
) -> Result<bool, SlpError> {
    let mut skip = vec![0u8; usize::from(u16::MAX)];
    let mut byte = [0u8; 1];
    loop {
        if events.read_exact(&mut skip[..usize::from(size)]).is_err()
            || events.read_exact(&mut byte).is_err()
        {
            return Ok(false);
        }
        if byte[0] == TRANSFORMATION {
            return Ok(true);
        }
        let Some(next) = sizes[usize::from(byte[0])] else {
            return Ok(false);
        };
        size = next;
    }
}

fn game_start(e: &[u8]) -> Start {
    let byte = |at: usize| e.get(at).copied().unwrap_or(0);
    let be32 = |at: usize| {
        e.get(at..at + 4)
            .map_or(0, |b| u32::from_be_bytes(b.try_into().expect("4 bytes")))
    };
    // Game Info Block at 0x5: the stage at 0xE, each player's type at
    // 0x61 + 0x24 * i (3 is none).
    let stage = u16::from_be_bytes([byte(0x5 + 0xE), byte(0x5 + 0xF)]);
    let present = (0..4).filter(|i| byte(0x5 + 0x61 + 0x24 * i) != 3);
    let mut any = false;
    let mut ucf_off = e.len() >= 0x161;
    for i in present {
        any = true;
        // Dashback fix at 0x141 + 8i, shield-drop fix at 0x145 + 8i.
        if be32(0x141 + 8 * i) != 0 || be32(0x145 + 8 * i) != 0 {
            ucf_off = false;
        }
    }
    Start {
        version: [byte(1), byte(2), byte(3)],
        stage,
        frozen_stadium: byte(0x1A2) != 0,
        ucf_off: ucf_off && any,
        codes: Vec::new(),
    }
}

/// The address of each code in a gecko code list, sorted, without repeats.
pub(crate) fn codes(gecko: &[u8]) -> Vec<String> {
    let mut codes = Vec::new();
    let mut at = 0;
    while at + 8 <= gecko.len() {
        let word = u32::from_be_bytes(gecko[at..at + 4].try_into().expect("4 bytes"));
        let value = u32::from_be_bytes(gecko[at + 4..at + 8].try_into().expect("4 bytes"));
        let kind = gecko[at];
        let address = 0x8000_0000 | (word & 0x01FF_FFFF);
        let next = match kind {
            0xC2 | 0xC3 => 8 + value as usize * 8,
            0x00..=0x05 => 8,
            0x06 | 0x07 => 8 + ((value as usize + 7) & !7),
            // The end of the list, or a kind this reader doesn't step over.
            _ => break,
        };
        codes.push(format!("0x{address:08X}"));
        at += next;
    }
    codes.sort();
    codes.dedup();
    codes
}

/// UBJSON as Slippi and tgg-melee write it, read into JSON.
struct Ubjson<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// How deep containers may nest.
const DEPTH: usize = 32;

impl Ubjson<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len());
        let end = end.ok_or("it ends early")?;
        let bytes = &self.bytes[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn int(&mut self, marker: u8) -> Result<i64, String> {
        Ok(match marker {
            b'i' => i64::from(self.byte()? as i8),
            b'U' => i64::from(self.byte()?),
            b'I' => i64::from(i16::from_be_bytes(self.take(2)?.try_into().expect("2"))),
            b'l' => i64::from(i32::from_be_bytes(self.take(4)?.try_into().expect("4"))),
            b'L' => i64::from_be_bytes(self.take(8)?.try_into().expect("8")),
            other => return Err(format!("a length marked {:?}", other as char)),
        })
    }

    fn length(&mut self) -> Result<usize, String> {
        let marker = self.byte()?;
        let n = self.int(marker)?;
        usize::try_from(n).map_err(|_| "a negative length".to_owned())
    }

    fn text(&mut self) -> Result<String, String> {
        let n = self.length()?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "text that isn't UTF-8".into())
    }

    fn key(&mut self) -> Result<String, String> {
        self.text()
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        let marker = self.byte()?;
        self.typed(marker, depth)
    }

    fn typed(&mut self, marker: u8, depth: usize) -> Result<Value, String> {
        Ok(match marker {
            b'Z' => Value::Null,
            b'T' => Value::Bool(true),
            b'F' => Value::Bool(false),
            b'i' | b'U' | b'I' | b'l' | b'L' => Value::from(self.int(marker)?),
            b'd' => float(f64::from(f32::from_be_bytes(
                self.take(4)?.try_into().expect("4"),
            ))),
            b'D' => float(f64::from_be_bytes(self.take(8)?.try_into().expect("8"))),
            b'C' => Value::String((self.byte()? as char).to_string()),
            b'S' | b'H' => Value::String(self.text()?),
            b'[' | b'{' => {
                if depth >= DEPTH {
                    return Err("it nests too deep".into());
                }
                self.container(marker == b'{', depth + 1)?
            }
            other => return Err(format!("a value marked {:?}", other as char)),
        })
    }

    fn container(&mut self, object: bool, depth: usize) -> Result<Value, String> {
        let mut kind = None;
        let mut count = None;
        if self.peek() == Some(b'$') {
            self.at += 1;
            kind = Some(self.byte()?);
        }
        if self.peek() == Some(b'#') {
            self.at += 1;
            count = Some(self.length()?);
        }
        let close = if object { b'}' } else { b']' };
        let mut array = Vec::new();
        let mut map = Map::new();
        let mut n = 0;
        loop {
            match count {
                Some(count) if n == count => break,
                None => {
                    while self.peek() == Some(b'N') {
                        self.at += 1;
                    }
                    if self.peek() == Some(close) {
                        self.at += 1;
                        break;
                    }
                }
                _ => {}
            }
            let key = if object { Some(self.key()?) } else { None };
            let value = match kind {
                Some(kind) => self.typed(kind, depth)?,
                None => self.value(depth)?,
            };
            match key {
                Some(key) => {
                    map.insert(key, value);
                }
                None => array.push(value),
            }
            n += 1;
        }
        Ok(if object {
            Value::Object(map)
        } else {
            Value::Array(array)
        })
    }
}

fn float(value: f64) -> Value {
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

/// Writing replays, for tests here and in front ends: a replay of `events`
/// (Event Payloads first) with `metadata`, when given, after them.
#[doc(hidden)]
pub mod write {
    use serde_json::Value;

    /// The bytes of a `.slp` file.
    pub fn replay(events: &[u8], metadata: Option<&Value>) -> Vec<u8> {
        let mut out = super::HEAD.to_vec();
        out.extend_from_slice(&(events.len() as u32).to_be_bytes());
        out.extend_from_slice(events);
        if let Some(metadata) = metadata {
            key(&mut out, "metadata");
            value(&mut out, metadata);
        }
        out.push(b'}');
        out
    }

    fn key(out: &mut Vec<u8>, key: &str) {
        length(out, key.len());
        out.extend_from_slice(key.as_bytes());
    }

    fn length(out: &mut Vec<u8>, n: usize) {
        if n < 256 {
            out.push(b'U');
            out.push(n as u8);
        } else {
            out.push(b'l');
            out.extend_from_slice(&(n as i32).to_be_bytes());
        }
    }

    /// `value` as UBJSON.
    pub fn value(out: &mut Vec<u8>, value: &Value) {
        match value {
            Value::Null => out.push(b'Z'),
            Value::Bool(true) => out.push(b'T'),
            Value::Bool(false) => out.push(b'F'),
            Value::Number(n) => match n.as_i64() {
                Some(i) => {
                    out.push(b'L');
                    out.extend_from_slice(&i.to_be_bytes());
                }
                None => {
                    out.push(b'D');
                    out.extend_from_slice(&n.as_f64().unwrap_or(0.0).to_be_bytes());
                }
            },
            Value::String(s) => {
                out.push(b'S');
                key(out, s);
            }
            Value::Array(items) => {
                out.push(b'[');
                for item in items {
                    self::value(out, item);
                }
                out.push(b']');
            }
            Value::Object(map) => {
                out.push(b'{');
                for (k, v) in map {
                    key(out, k);
                    self::value(out, v);
                }
                out.push(b'}');
            }
        }
    }

    /// The events of a replay up to its first frame: Event Payloads, a Game
    /// Start of Slippi `version` on `stage` (frozen Stadium flag `frozen`,
    /// two players with UCF fixes `ucf`), and the gecko list `gecko` in
    /// Message Splitter events; then a Frame Start, which reading stops at
    /// (a Stadium transformation event, [`TRANSFORMATION_EVENT`], may follow).
    pub fn events(version: [u8; 3], stage: u16, frozen: bool, ucf: u32, gecko: &[u8]) -> Vec<u8> {
        const START_SIZE: u16 = 0x2FC;
        const SPLIT_SIZE: u16 = 0x204;
        const FRAME_START_SIZE: u16 = 0x0C;
        const TRANSFORMATION_SIZE: u16 = 0x08;
        let mut out = vec![0x35, 1 + 3 * 4];
        for (command, size) in [
            (0x36u8, START_SIZE),
            (0x10, SPLIT_SIZE),
            (0x3A, FRAME_START_SIZE),
            (0x41, TRANSFORMATION_SIZE),
        ] {
            out.push(command);
            out.extend_from_slice(&size.to_be_bytes());
        }
        let mut start = vec![0u8; 1 + usize::from(START_SIZE)];
        start[0] = 0x36;
        start[1..4].copy_from_slice(&version);
        start[0x5 + 0xE..0x5 + 0x10].copy_from_slice(&stage.to_be_bytes());
        for i in 0..4 {
            // Two humans, two empty ports.
            start[0x5 + 0x61 + 0x24 * i] = if i < 2 { 0 } else { 3 };
            start[0x141 + 8 * i..0x145 + 8 * i].copy_from_slice(&ucf.to_be_bytes());
            start[0x145 + 8 * i..0x149 + 8 * i].copy_from_slice(&ucf.to_be_bytes());
        }
        start[0x1A2] = u8::from(frozen);
        out.extend_from_slice(&start);
        let parts: Vec<&[u8]> = if gecko.is_empty() {
            Vec::new()
        } else {
            gecko.chunks(0x200).collect()
        };
        for (index, part) in parts.iter().enumerate() {
            let mut split = vec![0u8; 1 + usize::from(SPLIT_SIZE)];
            split[0] = 0x10;
            split[1..1 + part.len()].copy_from_slice(part);
            split[0x201..0x203].copy_from_slice(&(part.len() as u16).to_be_bytes());
            split[0x203] = 0x3D;
            split[0x204] = u8::from(index + 1 == parts.len());
            out.extend_from_slice(&split);
        }
        let mut frame = vec![0u8; 1 + usize::from(FRAME_START_SIZE)];
        frame[0] = 0x3A;
        out.extend_from_slice(&frame);
        out
    }

    /// A Stadium transformation event, to follow [`events`].
    pub const TRANSFORMATION_EVENT: [u8; 9] = [0x41, 0, 0, 0, 0, 0, 0, 0, 5];

    /// A gecko list of `C2` injections at each of `addresses` (one line
    /// each), ending with the list's terminator.
    pub fn gecko(addresses: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        for address in addresses {
            out.extend_from_slice(&(0xC200_0000 | (address & 0x01FF_FFFF)).to_be_bytes());
            out.extend_from_slice(&1u32.to_be_bytes());
            out.extend_from_slice(&[0x60, 0, 0, 0, 0, 0, 0, 0]);
        }
        out.extend_from_slice(&[0xF0, 0, 0, 0, 0, 0, 0, 0]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn file(bytes: &[u8]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        std::fs::write(file.path(), bytes).expect("write");
        file
    }

    #[test]
    fn reads_game_start_and_every_code_of_a_long_gecko_list() {
        // More codes than one Message Splitter holds.
        let addresses: Vec<u32> = (0..100).map(|i| 0x8016_E510 + 4 * i).collect();
        let events = write::events([3, 18, 0], 3, true, 1, &write::gecko(&addresses));
        let replay = file(&write::replay(&events, None));
        let read = read(replay.path(), |_| false).expect("read");
        let start = read.start.expect("game start");
        assert_eq!(start.version(), "3.18.0");
        assert_eq!(start.stage, 3);
        assert!(start.frozen_stadium);
        assert!(!start.ucf_off);
        assert_eq!(start.codes.len(), 100);
        assert_eq!(start.codes[0], "0x8016E510");
        assert_eq!(read.metadata, None);
    }

    #[test]
    fn reads_metadata_after_the_events_and_skips_them_when_asked() {
        let metadata = json!({
            "startAt": "2026-11-07T00:00:00Z",
            "playedOn": "tgg-melee",
            "tgg": {"game": "tgg-melee 0.5.0 x", "mods": ["tgg.ucf 0.84.0"], "n": 300, "f": 0.5}
        });
        let events = write::events([3, 18, 0], 31, false, 0, &[]);
        let replay = file(&write::replay(&events, Some(&metadata)));
        let read = read(replay.path(), |_| true).expect("read");
        assert_eq!(read.metadata, Some(metadata));
        assert_eq!(read.start, None);
    }

    #[test]
    fn a_replay_being_written_has_no_metadata_yet() {
        let events = write::events([3, 9, 0], 31, false, 0, &[]);
        let mut bytes = write::replay(&events, None);
        bytes[11..15].copy_from_slice(&[0; 4]);
        bytes.pop();
        let replay = file(&bytes);
        let read = read(replay.path(), |_| true).expect("read");
        assert_eq!(read.metadata, None);
        let start = read.start.expect("game start");
        assert!(start.ucf_off);
        assert!(start.codes.is_empty());
    }

    #[test]
    fn a_stadium_game_that_never_transforms_was_frozen() {
        let events = write::events([3, 18, 0], 3, false, 1, &[]);
        let frozen = file(&write::replay(&events, None));
        let start = read(frozen.path(), |_| false).expect("read").start;
        assert!(start.expect("start").frozen_stadium);
        let mut events = events;
        events.extend_from_slice(&write::TRANSFORMATION_EVENT);
        let transforms = file(&write::replay(&events, None));
        let start = read(transforms.path(), |_| false).expect("read").start;
        assert!(!start.expect("start").frozen_stadium);
    }

    #[test]
    fn other_files_are_refused() {
        let other = file(b"{\"format\": 1}");
        assert!(matches!(
            read(other.path(), |_| true),
            Err(SlpError::NotReplay)
        ));
    }

    #[test]
    fn typed_and_counted_containers_read() {
        // [$U#U\x03 1 2 3] and {#U\x01 Ua T}
        let bytes = b"[$U#U\x03\x01\x02\x03";
        let mut reader = Ubjson { bytes, at: 0 };
        assert_eq!(reader.value(0).expect("array"), json!([1, 2, 3]));
        let bytes = b"{#U\x01U\x01aT";
        let mut reader = Ubjson { bytes, at: 0 };
        assert_eq!(reader.value(0).expect("object"), json!({"a": true}));
    }
}
