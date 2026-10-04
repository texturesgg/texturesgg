//! The models a fighter's move spawns, drawn on it: the shine at its hip
//! through its down special, the blaster in its hand and the lasers it fires
//! through its neutral special. Whatever plays the move, the Moves pane or a
//! Shared pane row, they appear.

use crate::Error;
use dat_parser::hsd::draw::HsdEvaluatedDrawWork;
use dat_parser::hsd::scene::HsdScene;
use dat_parser::math::Mat4;
use hsd_render::{HsdRenderer, ModelId, PreparedGeometry};
use melee_dat::{Firing, MeleeFighterPlayback, SharedModel, Shot, SpawnedModel};
use std::rc::Rc;

pub(super) struct Spawns {
    /// The models the fighter's costumes share, with their files.
    shared: Vec<(SharedModel, Rc<Vec<u8>>)>,
    /// The animation the models were spawned for; `None` spawns afresh.
    animation: Option<usize>,
    models: Vec<SpawnedModel>,
    /// Each model's id among the renderer's, once added; `None` for one
    /// whose fighter part the costume lacks.
    ids: Vec<Option<ModelId>>,
    /// What the move fires, and when.
    firings: Vec<Firing>,
    /// Shots in flight, each with its id among the renderer's once added.
    shots: Vec<(Shot, Option<ModelId>)>,
    /// Firings due to fire on the next pose, which has their part's matrix.
    due: Vec<usize>,
    /// The fighter's frame at the last step, to see a fire frame pass.
    frame: f32,
}

impl Spawns {
    pub fn new(shared: Vec<(SharedModel, Rc<Vec<u8>>)>) -> Self {
        Self {
            shared,
            animation: None,
            models: Vec::new(),
            ids: Vec::new(),
            firings: Vec::new(),
            shots: Vec::new(),
            due: Vec::new(),
            frame: 0.0,
        }
    }

    /// Spawn afresh on the next frame, as when the move starts again.
    pub fn reset(&mut self) {
        self.animation = None;
    }

    /// Spawn the models `playback`'s move does when it isn't the move they
    /// were spawned for, caught up to where it is. Returns the renderer ids
    /// of the models and shots it replaces, for the renderer to let go.
    pub fn follow(&mut self, playback: &MeleeFighterPlayback) -> Result<Vec<ModelId>, Error> {
        let current = playback.current();
        if self.animation == Some(current) {
            return Ok(Vec::new());
        }
        self.animation = Some(current);
        let action = playback
            .animations()
            .get(current)
            .and_then(|animation| animation.action.as_deref());
        let report = |model: &SharedModel, error: &melee_dat::MeleeError| {
            crate::log(&format!("couldn't spawn {}: {error}", model.name()))
        };
        let (models, firings) = match action {
            Some(action) => (
                self.shared
                    .iter()
                    .flat_map(|(model, bytes)| {
                        model
                            .spawned(bytes, action)
                            .inspect_err(|error| report(model, error))
                            .unwrap_or_default()
                    })
                    .collect(),
                self.shared
                    .iter()
                    .filter_map(|(model, bytes)| {
                        model
                            .firing(bytes, action, current)
                            .inspect_err(|error| report(model, error))
                            .ok()
                            .flatten()
                    })
                    .collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        self.models = models;
        self.firings = firings;
        self.due.clear();
        // They start with the move: catch up to where it is.
        for _ in 0..playback.frame().max(0.0) as u32 {
            for model in &mut self.models {
                model.advance()?;
            }
        }
        self.frame = playback.frame();
        let mut replaced: Vec<ModelId> = std::mem::take(&mut self.ids)
            .into_iter()
            .flatten()
            .collect();
        replaced.extend(self.shots.drain(..).filter_map(|(_, id)| id));
        Ok(replaced)
    }

    /// Step every model and shot one frame with the fighter, now at `frame`
    /// of its move, and see what it fires. Returns the renderer ids of the
    /// shots that ran out.
    pub fn advance(&mut self, frame: f32) -> Result<Vec<ModelId>, Error> {
        for model in &mut self.models {
            model.advance()?;
        }
        for (shot, _) in &mut self.shots {
            shot.advance();
        }
        let mut expired = Vec::new();
        self.shots.retain(|(shot, id)| {
            if !shot.alive() {
                expired.extend(*id);
            }
            shot.alive()
        });
        // A fire frame passed since the last step; the move may have looped.
        let last = self.frame;
        let passed = |at: f32| {
            if frame >= last {
                last < at && at <= frame
            } else {
                at > last || at <= frame
            }
        };
        for (index, firing) in self.firings.iter().enumerate() {
            if firing.frames.iter().any(|&at| passed(at)) {
                self.due.push(index);
            }
        }
        self.frame = frame;
        Ok(expired)
    }

    /// Whether something spawned turns to face the camera (the shine).
    pub fn faces_camera(&self) -> bool {
        !self.models.is_empty()
    }

    /// The fighter parts the next [`Self::upload`] needs the matrices of:
    /// those the models follow, and those due to fire.
    pub fn parts(&self) -> Vec<u8> {
        let mut parts: Vec<u8> = self.models.iter().map(|model| model.spawn().part).collect();
        parts.extend(self.due.iter().map(|&firing| self.firings[firing].part));
        parts
    }

    /// Fire what is due and pose every model and shot for this frame, then
    /// draw each with `renderer`: `joint` gives a fighter part's world
    /// matrix in the fighter's pose (`None` when the costume lacks it),
    /// `view` the camera's, and `scale` the fighter's model scale.
    pub fn upload(
        &mut self,
        renderer: &mut HsdRenderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        joint: impl Fn(u8) -> Option<Mat4>,
        view: Mat4,
        scale: f32,
    ) -> Result<(), Error> {
        // Shots due leave the muzzle as it is this frame; the viewport's
        // fighter faces right.
        for firing in std::mem::take(&mut self.due) {
            let firing = &self.firings[firing];
            if let Some(muzzle) = joint(firing.part) {
                self.shots.push((firing.fire(muzzle, 1.0)?, None));
            }
        }
        for (shot, id) in &mut self.shots {
            shot.pose()?;
            let (scene, work) = shot.drawn();
            draw(renderer, device, queue, id, scene, work)?;
        }
        self.ids.resize(self.models.len(), None);
        for (model, id) in self.models.iter_mut().zip(&mut self.ids) {
            // A costume without the part draws nothing there.
            let Some(joint) = joint(model.spawn().part) else {
                continue;
            };
            model.set_view(Some(view));
            model.pose(joint, scale)?;
            let (scene, work) = model.drawn();
            draw(renderer, device, queue, id, scene, work)?;
        }
        Ok(())
    }

    /// Forget the renderer's ids, which a rebuilt renderer doesn't have.
    pub fn forget_renderer(&mut self) {
        self.ids.clear();
        for (_, id) in &mut self.shots {
            *id = None;
        }
    }
}

/// Upload `scene` posed as `work` to `renderer`: as the model `id` names,
/// or as a new one whose id it keeps.
fn draw(
    renderer: &mut HsdRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    id: &mut Option<ModelId>,
    scene: &HsdScene,
    work: &HsdEvaluatedDrawWork,
) -> Result<(), Error> {
    match id {
        Some(id) => renderer.update_draw_work(queue, *id, scene, work)?,
        None => {
            *id = Some(renderer.add_model(device, queue, PreparedGeometry::new(scene, work)?)?)
        }
    }
    Ok(())
}
