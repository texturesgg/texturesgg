//! The stage: the one model showing, drawn by hsd-render on Rune's own
//! device. Melee shows a fighter or stage on it; the editor will take the
//! same model over. Its state is the model, its pose and camera and whether
//! it plays; `actions` changes them, `viewport` draws the model and runs its
//! clock, and `view` lays out the plate and the timeline under it.

pub mod actions;
mod copy;
mod spawns;
pub mod view;
pub mod viewport;

use dat_parser::hsd::scene::{HsdTextureIndex, HsdTextureSourceId};
use hsd_render::Orbit;
use melee_dat::{MeleeModel, MeleeReferenceStore, MeleeSlot, SharedModel};
use rune::{Live, LiveText};
use std::collections::BTreeMap;
use std::rc::Rc;

/// The viewport's effect slot.
pub const VIEWPORT: u32 = 1;
/// Play's fighter's effect slot.
pub const FIGHTER: u32 = 2;

pub struct Stage {
    pub shown: Option<Shown>,
    /// How many models have been put on the stage; each takes the next number.
    pub shown_count: u64,
    /// Edits the viewport couldn't show, in the player's words, to say so.
    pub rejected: Vec<String>,
    /// What a click on the model landed on, once the viewport has read it
    /// back: for each texture the surface draws, most defining first, the
    /// scene's ids for it. Empty when the click missed the model.
    pub picked: Option<Vec<Vec<HsdTextureSourceId>>>,
    /// Where the timeline's playhead sits along its track, in points; the
    /// viewport writes it each frame, so the view needn't rebuild.
    pub playhead: Live<[f32; 2]>,
    /// The frame count beside the timeline ("F 12 / 60"), likewise.
    pub frame_count: LiveText,
}

impl Default for Stage {
    fn default() -> Self {
        Self {
            shown: None,
            shown_count: 0,
            rejected: Vec::new(),
            picked: None,
            playhead: Live::new([0.0, 0.0]),
            frame_count: LiveText::new(FRAME_COUNT_SPACE),
        }
    }
}

/// The frame count's widest text, which reserves its room.
pub const FRAME_COUNT_SPACE: &str = "F 000 / 000";

/// A model on the stage.
pub struct Shown {
    /// The slot it's from.
    pub slot: MeleeSlot,
    pub model: MeleeModel,
    pub orbit: Orbit,
    /// A drag across the stage's travel since its press, as last heard;
    /// `None` between drags.
    pub gesture: Option<[f32; 2]>,
    /// A click on the model waiting for the viewport to pick what it landed
    /// on, as a fraction of the plate across and down.
    pub pick: Option<[f32; 2]>,
    pub playing: bool,
    /// The models its fighter's costumes share, with their files: what its
    /// moves spawn on it (the shine, the blaster and its lasers).
    pub shared: Vec<(SharedModel, Rc<Vec<u8>>)>,
    /// Its number among the models shown, so the viewport rebuilds its copy
    /// on the GPU when the model is a new one.
    pub generation: u64,
    /// The pose changed while paused (a seek, a new move): draw it again.
    pub posed: bool,
    /// The move started over (played or sought): what it spawns starts over too.
    pub restart: bool,
    /// The reference files a stock costume plays with: its texture names
    /// come from them too.
    pub store: Option<Rc<MeleeReferenceStore>>,
    /// Edited pixels by scene texture, with their size: shown in place of the
    /// file's, and shown again whenever the renderer is rebuilt.
    pub edited: BTreeMap<usize, ((u32, u32), Vec<u8>)>,
    /// Edited scene textures the viewport hasn't uploaded yet.
    pub pending: Vec<usize>,
    /// The scene textures tinted to show which surfaces draw the texture
    /// selected in the editor.
    pub highlight: Vec<HsdTextureIndex>,
    pub highlight_changed: bool,
}

impl Shown {
    /// What the timeline names beside its play button: the idle a fighter
    /// shows, nothing for a stage.
    pub fn move_label(&self) -> Option<String> {
        self.model.fighter().map(|_| "Idle".to_owned())
    }
}
