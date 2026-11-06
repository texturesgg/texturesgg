//! The stage: the one model showing, drawn by hsd-render on Rune's own
//! device. Melee shows a fighter or stage on it; the editor will take the
//! same model over. Its state is the model, its pose and camera and whether
//! it plays; `actions` changes them, `viewport` draws the model and runs its
//! clock, and `view` lays out the plate and the timeline under it.

pub mod actions;
mod copy;
pub mod view;
pub mod viewport;

use hsd_render::Orbit;
use melee_dat::{MeleeModel, MeleeSlot};
use rune::{Live, LiveText};
use tgg_app::moves::MoveRow;

/// The viewport's effect slot.
pub const VIEWPORT: u32 = 1;

pub struct Stage {
    pub shown: Option<Shown>,
    /// How many models have been put on the stage; each takes the next number.
    pub shown_count: u64,
    /// Where the timeline's playhead sits along its track, in points; the
    /// viewport writes it each frame, so the view needn't rebuild.
    pub playhead: Live<[f32; 2]>,
    /// The frame count beside the timeline ("F 12 / 60"), likewise.
    pub frame_count: LiveText,
}

impl Default for Stage {
    fn default() -> Self {
        Self { shown: None, shown_count: 0, playhead: Live::new([0.0, 0.0]), frame_count: LiveText::new(FRAME_COUNT_SPACE) }
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
    pub playing: bool,
    /// The move playing, if one was chosen; the idle otherwise.
    pub animation: Option<usize>,
    /// Its fighter's moves; empty for a stage.
    pub moves: Vec<MoveRow>,
    /// Its number among the models shown, so the viewport rebuilds its copy
    /// on the GPU when the model is a new one.
    pub generation: u64,
    /// The pose changed while paused (a seek, a new move): draw it again.
    pub posed: bool,
}

impl Shown {
    /// The playing move's name, else "Idle" for a fighter; nothing for a
    /// stage.
    pub fn move_label(&self) -> Option<String> {
        if self.moves.is_empty() {
            return None;
        }
        let Some(animation) = self.animation else { return Some("Idle".into()) };
        self.moves.iter().find_map(|row| match row {
            MoveRow::Move { animation: playing, title, .. } if *playing == animation => Some(title.clone()),
            _ => None,
        })
    }
}
