//! Version ranges in a manifest's `depends`, read as the game reads them.
//!
//! A range is a version (`1.2.3`, exactly; `1.2`, any 1.2.x), `^1.2` (the
//! same leftmost non-zero part, at least 1.2), `~1.2` (the same minor, at
//! least 1.2), `>=1.2`, or `*`. Versions compare by major, minor and patch; a
//! pre-release or build suffix is ignored.

use semver::Version;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Range {
    text: String,
    op: Op,
    /// The parts the range gives, with the ones it leaves out as 0.
    low: [u64; 3],
    given: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Any,
    Exact,
    Caret,
    Tilde,
    AtLeast,
}

#[derive(Debug, thiserror::Error)]
#[error("{0:?} isn't a version range: use 1.2.3, 1.2, ^1.2, ~1.2, >=1.2 or *")]
pub struct RangeError(String);

impl Range {
    pub fn parse(text: &str) -> Result<Self, RangeError> {
        let error = || RangeError(text.to_owned());
        if text == "*" {
            return Ok(Self {
                text: text.to_owned(),
                op: Op::Any,
                low: [0; 3],
                given: 0,
            });
        }
        let (op, rest) = if let Some(rest) = text.strip_prefix(">=") {
            (Op::AtLeast, rest)
        } else if let Some(rest) = text.strip_prefix('^') {
            (Op::Caret, rest)
        } else if let Some(rest) = text.strip_prefix('~') {
            (Op::Tilde, rest)
        } else {
            (Op::Exact, text)
        };
        let core = rest.split(['-', '+']).next().unwrap_or_default();
        let mut low = [0; 3];
        let mut given = 0;
        for part in core.split('.') {
            if given == 3 || part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(error());
            }
            low[given] = part.parse().map_err(|_| error())?;
            given += 1;
        }
        Ok(Self {
            text: text.to_owned(),
            op,
            low,
            given,
        })
    }

    /// Whether `version` is in the range.
    pub fn matches(&self, version: &Version) -> bool {
        let have = [version.major, version.minor, version.patch];
        if self.op == Op::Any {
            return true;
        }
        if have < self.low {
            return false;
        }
        // The last part that must stay the same.
        let fixed = match self.op {
            Op::Any | Op::AtLeast => return true,
            Op::Caret => self.low[..self.given - 1]
                .iter()
                .take_while(|&&part| part == 0)
                .count(),
            Op::Tilde => usize::from(self.given > 1),
            Op::Exact => self.given - 1,
        };
        have[..=fixed] == self.low[..=fixed]
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl TryFrom<String> for Range {
    type Error = RangeError;

    fn try_from(text: String) -> Result<Self, RangeError> {
        Self::parse(&text)
    }
}

impl std::str::FromStr for Range {
    type Err = RangeError;

    fn from_str(text: &str) -> Result<Self, RangeError> {
        Self::parse(text)
    }
}

impl From<Range> for String {
    fn from(range: Range) -> Self {
        range.text
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_match_as_the_game_reads_them() {
        let v = |text: &str| Version::parse(text).expect("version");
        let cases = [
            ("1.2.3", "1.2.3", true),
            ("1.2.3", "1.2.4", false),
            ("1.2", "1.2.9", true),
            ("1.2", "1.3.0", false),
            ("^1.2", "1.9.0", true),
            ("^1.2", "2.0.0", false),
            ("^1.2", "1.1.9", false),
            ("^0.3", "0.3.5", true),
            ("^0.3", "0.4.0", false),
            ("~1.2", "1.2.7", true),
            ("~1.2", "1.3.0", false),
            (">=1.2", "4.0.0", true),
            ("*", "0.0.1", true),
            ("1.2.3", "1.2.3-beta", true),
        ];
        for (range, version, expected) in cases {
            let range = Range::parse(range).expect("range");
            assert_eq!(range.matches(&v(version)), expected, "{range} {version}");
        }
        for bad in ["", "^", "1..2", "v1.2", "1.2.3.4", "> 1"] {
            assert!(Range::parse(bad).is_err(), "{bad}");
        }
    }
}
