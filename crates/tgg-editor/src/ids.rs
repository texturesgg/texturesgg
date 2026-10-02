//! What the app's records name: a skin by its file's SHA-256, and a slot of
//! the game by [`MeleeSlot`]. On disk a skin id is its hash in hex and a slot
//! is its file name (`PlFcRe.dat`).

use melee_dat::MeleeSlot;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::fmt;
use std::str::FromStr;

/// A file in the library: its SHA-256, which also names its copy there.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkinId([u8; 32]);

impl SkinId {
    /// The id of the file holding `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}

/// The hash in lowercase hex.
impl fmt::Display for SkinId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for SkinId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SkinId({self})")
    }
}

/// Text that is not 64 hex digits.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("a skin id is 64 hex digits")]
pub struct NotASkinId;

impl FromStr for SkinId {
    type Err = NotASkinId;

    fn from_str(text: &str) -> Result<Self, NotASkinId> {
        let digits = text.as_bytes();
        if digits.len() != 64 {
            return Err(NotASkinId);
        }
        let mut hash = [0; 32];
        for (byte, pair) in hash.iter_mut().zip(digits.chunks(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| NotASkinId)?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| NotASkinId)?;
        }
        Ok(Self(hash))
    }
}

impl Serialize for SkinId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SkinId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// An optional slot, written as its file name. A name that is no slot's
/// reads as none.
pub(crate) mod optional_slot {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        slot: &Option<MeleeSlot>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        slot.map(MeleeSlot::file_name).serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<MeleeSlot>, D::Error> {
        let name = Option::<String>::deserialize(deserializer)?;
        Ok(name.as_deref().and_then(MeleeSlot::from_file_name))
    }
}

/// A list of slots, written as their file names. Names that are no slot's
/// are left out.
pub(crate) mod slot_list {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(
        slots: &[MeleeSlot],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(slots.iter().map(|slot| slot.file_name()))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<MeleeSlot>, D::Error> {
        let names = Vec::<String>::deserialize(deserializer)?;
        Ok(names
            .iter()
            .filter_map(|name| MeleeSlot::from_file_name(name))
            .collect())
    }
}

/// A map keyed by slot, written with each slot's file name as its key.
/// Entries under a name that is no slot's are left out.
pub(crate) mod slot_map {
    use super::*;
    use std::collections::BTreeMap;

    pub(crate) fn serialize<S: Serializer, V: Serialize>(
        map: &BTreeMap<MeleeSlot, V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.collect_map(map.iter().map(|(slot, value)| (slot.file_name(), value)))
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>, V: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<MeleeSlot, V>, D::Error> {
        let named = BTreeMap::<String, V>::deserialize(deserializer)?;
        Ok(named
            .into_iter()
            .filter_map(|(name, value)| Some((MeleeSlot::from_file_name(&name)?, value)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::SkinId;

    /// The id is the hash, and its text is the hex the library's files and
    /// records are named with.
    #[test]
    fn a_skin_id_is_its_sha256_in_hex_and_reads_back() {
        let id = SkinId::of(b"");
        let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(id.to_string(), hex);
        assert_eq!(hex.parse(), Ok(id));
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{hex}\""));
        for text in ["", "e3b0", &hex.replace('e', "g"), &format!("{hex}00")] {
            assert!(text.parse::<SkinId>().is_err(), "{text}");
        }
    }
}
