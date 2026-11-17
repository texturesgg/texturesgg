//! A replay's stats, from its frames: for a game of two players, each
//! one's kills, damage dealt, openings (and openings per kill), L-cancel
//! rate and actions per minute. The list never reads frames; these are read
//! for one replay when the player opens it.
//!
//! The definitions are simple ones in Slippi's spirit, not its exact
//! conversion logic: an opening is the opponent taking damage after a
//! stretch without any; an action is a button newly pressed.

use std::path::Path;

/// Frames without damage before a hit counts as a new opening.
const OPENING_GAP: usize = 45;
/// Frames in a minute.
const MINUTE: f32 = 60.0 * 60.0;

/// One player's stats.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerStats {
    /// The port, 0-based.
    pub port: u8,
    pub stocks_left: u8,
    pub kills: u32,
    pub damage_dealt: f32,
    pub openings: u32,
    /// L-cancels landed and missed.
    pub l_cancels: (u32, u32),
    pub actions_per_minute: f32,
}

impl PlayerStats {
    /// Openings per kill, when there was a kill.
    pub fn openings_per_kill(&self) -> Option<f32> {
        (self.kills > 0).then(|| self.openings as f32 / self.kills as f32)
    }

    /// The share of L-cancels landed, when there were any.
    pub fn l_cancel_rate(&self) -> Option<f32> {
        let (landed, missed) = self.l_cancels;
        let all = landed + missed;
        (all > 0).then(|| landed as f32 / all as f32)
    }
}

/// The stats of the two-player game in the replay at `path`; none for a
/// game of more or fewer players.
pub fn replay_stats(path: &Path) -> Result<Vec<PlayerStats>, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let mut events = std::io::Cursor::new(events_only(&bytes)?);
    let game = peppi::io::slippi::read(&mut events, None).map_err(|error| error.to_string())?;
    let ports = &game.frames.ports;
    if ports.len() != 2 {
        return Ok(Vec::new());
    }
    let frames = game.frames.len();
    let series = |index: usize| {
        let post = &ports[index].leader.post;
        let pre = &ports[index].leader.pre;
        (
            post.percent.values().to_vec(),
            post.stocks.values().to_vec(),
            post.l_cancel.as_ref().map(|array| array.values().to_vec()),
            pre.buttons_physical.values().to_vec(),
        )
    };
    let players = [series(0), series(1)];
    let mut stats: Vec<PlayerStats> = ports
        .iter()
        .enumerate()
        .map(|(index, port)| PlayerStats {
            port: port.port as u8,
            stocks_left: players[index].1.last().copied().unwrap_or(0),
            ..Default::default()
        })
        .collect();
    for me in 0..2 {
        let them = 1 - me;
        let (percent, stocks, _, _) = &players[them];
        let mut quiet = OPENING_GAP;
        for frame in 1..frames.min(percent.len()) {
            if stocks[frame] < stocks[frame - 1] {
                stats[me].kills += 1;
                quiet = OPENING_GAP;
                continue;
            }
            let gained = percent[frame] - percent[frame - 1];
            if gained > 0.0 {
                stats[me].damage_dealt += gained;
                if quiet >= OPENING_GAP {
                    stats[me].openings += 1;
                }
                quiet = 0;
            } else {
                quiet += 1;
            }
        }
        let (_, _, l_cancel, buttons) = &players[me];
        if let Some(l_cancel) = l_cancel {
            // Each landing holds its value for a few frames: count where it changes.
            for frame in 1..l_cancel.len() {
                if l_cancel[frame] != l_cancel[frame - 1] {
                    match l_cancel[frame] {
                        1 => stats[me].l_cancels.0 += 1,
                        2 => stats[me].l_cancels.1 += 1,
                        _ => {}
                    }
                }
            }
        }
        let presses: u32 = buttons
            .windows(2)
            .map(|pair| (pair[1] & !pair[0]).count_ones())
            .sum();
        let minutes = frames as f32 / MINUTE;
        stats[me].actions_per_minute = if minutes > 0.0 {
            presses as f32 / minutes
        } else {
            0.0
        };
    }
    Ok(stats)
}

/// The start of every replay: `{`, the key `raw`, and a typed, counted byte
/// array whose length follows as a big-endian `i32`.
const HEAD: &[u8] = b"{U\x03raw[$U#l";

/// The replay `bytes` with its `metadata` left out: the events are all the
/// stats need, and a metadata peppi doesn't read (tgg-melee's carries
/// arrays) shouldn't keep them from being read.
fn events_only(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let rest = bytes.strip_prefix(HEAD).ok_or("not a Slippi replay")?;
    let length = rest.get(..4).ok_or("cut short")?;
    let length = u32::from_be_bytes(length.try_into().expect("four bytes")) as usize;
    // A replay still being written says 0: its events run to the end.
    let end = if length == 0 {
        bytes.len()
    } else {
        (HEAD.len() + 4 + length).min(bytes.len())
    };
    let mut out = bytes[..end].to_vec();
    if length == 0 {
        let written = (end - HEAD.len() - 4) as u32;
        out[HEAD.len()..HEAD.len() + 4].copy_from_slice(&written.to_be_bytes());
    }
    out.push(b'}');
    Ok(out)
}

#[cfg(test)]
mod tests {
    /// A real game from the corpus, when it's on this machine: ciao's
    /// three-stock Peach win over Krowten's Fox on Pokémon Stadium.
    #[test]
    fn a_real_game_reads_as_it_was_played() {
        // `$TGG_SLP_CORPUS`, else beside the core checkout (or a worktree of it).
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let corpus = std::env::var_os("TGG_SLP_CORPUS")
            .map(std::path::PathBuf::from)
            .into_iter()
            .chain(["../../../slp-corpus", "../../../../../../slp-corpus"].map(|up| here.join(up)))
            .find(|folder| folder.is_dir());
        let Some(path) = corpus.map(|corpus| corpus.join("friends2/Game_20260815T011035.slp"))
        else {
            return;
        };
        let stats = super::replay_stats(&path).expect("it reads");
        assert_eq!(stats.len(), 2);
        let (peach, fox) = (&stats[0], &stats[1]);
        assert_eq!(peach.kills, 4, "Peach took all four of Fox's stocks");
        assert_eq!(fox.kills, 1, "and lost one");
        assert_eq!(peach.stocks_left, 3);
        assert!(
            peach.damage_dealt > 300.0 && peach.openings > peach.kills,
            "{peach:?}"
        );
        assert!(peach.actions_per_minute > 30.0, "{peach:?}");
    }
}
