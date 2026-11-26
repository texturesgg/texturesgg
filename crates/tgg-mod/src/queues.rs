//! The meeting point's queues (`GET <meet>/queues`): the standing ones
//! textures.gg keeps open for its sets, and the ones players' own sets made
//! while someone searches in them. A set searches in the queue whose mods are
//! exactly its gameplay mods ([`crate::sets::SetPlan::gameplay`]).

use crate::error::Error;
use crate::net::{Net, NetError};
use crate::sets::QueueMod;
use serde::{Deserialize, Serialize};

/// The most queues asked for at once.
const MOST: u32 = 20;
/// The longest answer read.
const LIMIT: u64 = 256 * 1024;

/// A queue the meeting point lists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    /// `standing` for one textures.gg keeps open, `player` for one a
    /// player's own set made.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub label: String,
    /// How many are searching in it now.
    #[serde(default)]
    pub searching: u32,
    /// The mods that have to match to meet here.
    #[serde(default)]
    pub mods: Vec<QueueMod>,
    /// The textures.gg set a standing queue comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<String>,
}

impl Queue {
    /// One textures.gg keeps open, listed even with nobody in it.
    pub fn standing(&self) -> bool {
        self.kind == "standing"
    }
}

/// What `GET <meet>/queues` answers.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Listing {
    #[serde(default)]
    pub queues: Vec<Queue>,
}

/// The queues open now, the standing ones first, as the meeting point
/// orders them.
pub fn queues(net: &dyn Net) -> Result<Vec<Queue>, Error> {
    let url = format!("{}/queues?limit={MOST}", net.meet_url());
    let body = match net.get(&url, LIMIT) {
        Ok(body) => body,
        Err(NetError::NotFound { .. }) => return Ok(Vec::new()),
        Err(error) => return Err(Error::from_net(error)),
    };
    serde_json::from_slice::<Listing>(&body)
        .map(|listing| listing.queues)
        .map_err(|error| Error::Registry(format!("the queues: {error}")))
}
