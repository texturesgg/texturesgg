//! The viewport: the app's compositor. Rune lays out the stage's effect slot
//! like any box; each frame this finds the slot, keeps the model's clock
//! (Melee's 60 Hz ticks), poses the model and what its move spawns on it
//! (the shine at the hip, the blaster and its lasers), has hsd-render draw
//! them into a texture the slot's size, then copies it in while Rune draws
//! the rest of the window. It owns only GPU resources and the clock; what
//! shows, the camera and whether it plays are the stage's state.

use super::copy::{self, Copy, Target};
use super::spawns::Spawns;
use super::{Shown, VIEWPORT};
use crate::state::App;
use dat_parser::descriptor::jobj::flags::BILLBOARD_FIELD;
use dat_parser::hsd::scene::{HsdTextureIndex, HsdTextureSourceId};
use dat_parser::math::Mat4;
use hsd_render::{
    HsdRenderer, ModelId, Orbit, PendingPick, PickReadback, PreparedGeometry,
    neutral_preview_lighting,
};
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::KIND_EFFECT;

/// Melee's animation clock.
const TICK: f64 = 1.0 / 60.0;
/// Catch-up cap after a stall, so a hitch never replays seconds of animation.
const MAX_TICKS: u32 = 4;

#[derive(Default)]
pub struct Viewport {
    gpu: Option<Gpu>,
    /// Time not yet spent on a tick, in seconds.
    clock: f64,
    /// The model plays, so the window keeps drawing.
    playing: bool,
    /// Edits the renderer refused this frame, for the stage to say.
    refused: Vec<String>,
    /// The one pick in flight. Rune submits a frame's work after the
    /// compositor returns, so a pick encoded this frame starts reading back
    /// on the next.
    pick: Pick,
}

/// Where the pick in flight is.
#[derive(Default)]
enum Pick {
    #[default]
    None,
    Encoded(PickReadback),
    Reading(PendingPick),
}

/// What lives on Rune's device: the renderer with the model and what its
/// move spawns, the texture it draws into, and the copy pass.
struct Gpu {
    key: DeviceKey,
    /// The `Shown::generation` the renderer holds.
    generation: u64,
    renderer: HsdRenderer,
    model: ModelId,
    spawns: Spawns,
    /// The camera the pose was last faced toward, for billboards.
    posed_orbit: Option<Orbit>,
    target: Target,
    copy: Copy,
}

type Error = Box<dyn std::error::Error>;

impl Viewport {
    /// The slot's rect in device pixels this frame, when it shows.
    fn slot(f: &Frame<'_, App>) -> Option<[f32; 4]> {
        f.rt.instances
            .iter()
            .enumerate()
            .find(|(n, i)| i.params[1] == KIND_EFFECT && f.rt.marks.get(*n) == Some(&VIEWPORT))
            .map(|(_, i)| i.rect)
    }

    /// Run the clock, keep the GPU copy current, and draw into the target,
    /// recording ahead of Rune's pass.
    fn draw(&mut self, f: &mut Frame<'_, App>, size: (u32, u32)) -> Result<(), Error> {
        let key = f.device_key();
        let (device, queue) = (f.device, f.queue);
        let Some(shown) = f.rt.state.stage.shown.as_mut() else {
            return Ok(());
        };
        let stale = self
            .gpu
            .as_ref()
            .is_none_or(|gpu| gpu.key != key || gpu.generation != shown.generation);
        if stale {
            let focus = shown.model.focus();
            let (scene, work) = shown.model.evaluate()?;
            let geometry = PreparedGeometry::new(scene, work)?.with_focus(focus);
            let (renderer, model) = HsdRenderer::with_model(
                device,
                queue,
                copy::FORMAT,
                geometry,
                neutral_preview_lighting(),
                size,
                shown.orbit,
            )?;
            let copy = Copy::new(device);
            let target = copy.target(device, size);
            let spawns = Spawns::new(shown.shared.clone());
            // A pick on the old renderer never resolves, or names its packets.
            self.pick = Pick::None;
            self.gpu = Some(Gpu {
                key,
                generation: shown.generation,
                renderer,
                model,
                spawns,
                posed_orbit: None,
                target,
                copy,
            });
            shown.posed = true;
            // A new renderer starts from the file's pixels and no tint: show every edit again.
            shown.pending = shown.edited.keys().copied().collect();
            shown.highlight_changed = true;
        }
        let gpu = self.gpu.as_mut().expect("built above");
        if gpu.target.size != size {
            gpu.renderer.resize(device, queue, size.0, size.1)?;
            gpu.target = gpu.copy.target(device, size);
        }
        if std::mem::take(&mut shown.restart) {
            gpu.spawns.reset();
        }
        gpu.renderer.set_orbit(queue, shown.orbit)?;
        // The camera as the model allows it (pitch short of the poles, the pan
        // within its bounds, the zoom in range), so a drag past a limit turns
        // back from the limit, not from somewhere past it.
        shown.orbit = gpu.renderer.orbit();
        let ticks = advance(&mut self.clock, shown, gpu, f.dt)?;
        let turned =
            gpu.posed_orbit != Some(gpu.renderer.orbit()) && faces_camera(shown, &gpu.spawns);
        if ticks > 0 || std::mem::take(&mut shown.posed) || turned {
            pose(shown, gpu, device, queue)?;
        }
        for index in std::mem::take(&mut shown.pending) {
            let Some((size, pixels)) = shown.edited.get(&index) else {
                continue;
            };
            // A refused edit is that edit's problem: forget it so a rebuilt renderer
            // never replays it, say so, and carry on with the rest.
            if let Err(error) = gpu.renderer.update_scene_texture(
                queue,
                gpu.model,
                HsdTextureIndex(index),
                *size,
                pixels,
            ) {
                shown.edited.remove(&index);
                self.refused
                    .push(format!("The stage couldn't show an edit: {error}"));
            }
        }
        if std::mem::take(&mut shown.highlight_changed) {
            gpu.renderer
                .set_highlight(queue, gpu.model, &shown.highlight)?;
        }
        gpu.renderer.encode(f.encoder, &gpu.target.view);
        // Pick in the same submission, so it sees this frame's pose. One at a
        // time: a click made meanwhile waits, and a newer one replaces it.
        if matches!(self.pick, Pick::None)
            && let Some([across, down]) = shown.pick.take()
        {
            let at = (
                (across * size.0 as f32) as u32,
                (down * size.1 as f32) as u32,
            );
            if at.0 < size.0 && at.1 < size.1 {
                // A pick that fails is logged, never fatal to the stage.
                match gpu.renderer.encode_pick(device, f.encoder, at) {
                    Ok(readback) => self.pick = Pick::Encoded(readback),
                    Err(error) => tgg_app::log(&format!("pick failed: {error}")),
                }
            }
        }
        Ok(())
    }

    /// Move the pick along: start reading back last frame's, or collect
    /// one that's arrived and say what it landed on.
    fn poll_pick(&mut self, f: &mut Frame<'_, App>) {
        self.pick = match std::mem::take(&mut self.pick) {
            Pick::None => Pick::None,
            Pick::Encoded(readback) => Pick::Reading(readback.map()),
            Pick::Reading(pending) => {
                if let Err(error) = f.device.poll(wgpu::PollType::Poll) {
                    tgg_app::log(&format!("pick poll failed: {error}"));
                }
                match pending.take() {
                    None => Pick::Reading(pending),
                    Some(Err(error)) => {
                        tgg_app::log(&format!("pick failed: {error}"));
                        Pick::None
                    }
                    Some(Ok(picked)) => {
                        if let (Some(gpu), Some(shown)) = (&self.gpu, &f.rt.state.stage.shown) {
                            f.rt.state.stage.picked = Some(picked_textures(gpu, shown, picked));
                        }
                        Pick::None
                    }
                }
            }
        };
    }

    /// Move the timeline's playhead and frame count to the model's frame.
    fn report(f: &mut Frame<'_, App>) {
        let track = f.rt.presented("stage/track").map(|(rect, _)| rect[2]);
        let stage = &f.rt.state.stage;
        let Some(shown) = &stage.shown else { return };
        let (frame, end) = (shown.model.frame(), shown.model.end_frame());
        if let Some(width) = track {
            let along = if end > 0.0 {
                (frame / end).clamp(0.0, 1.0)
            } else {
                0.0
            };
            stage.playhead.set([along * width, 0.0]);
        }
        stage.frame_count.set(format!(
            "F {} / {}",
            frame.floor() as u32,
            end.floor() as u32
        ));
    }
}

/// What a pick landed on: for each texture the picked surface draws, most
/// defining first, the scene's ids for it. Nothing when it missed the
/// model (the background, or a shot its move spawned).
fn picked_textures(
    gpu: &Gpu,
    shown: &Shown,
    picked: Option<hsd_render::PickId>,
) -> Vec<Vec<HsdTextureSourceId>> {
    let packet = picked
        .and_then(|pick| gpu.renderer.resolve_pick(pick))
        .filter(|(model, _)| *model == gpu.model);
    let Some((_, packet)) = packet else {
        return Vec::new();
    };
    let scene = shown.model.scene();
    gpu.renderer
        .packet_textures(gpu.model, packet)
        .unwrap_or_default()
        .into_iter()
        .map(|texture| {
            texture
                .scene_textures
                .iter()
                .filter_map(|index| Some(scene.textures.get(index.0)?.id))
                .collect()
        })
        .collect()
}

/// Advance the model and what it spawns by the ticks due since the last
/// frame, letting go of shots that ran out; how many ticks.
fn advance(clock: &mut f64, shown: &mut Shown, gpu: &mut Gpu, dt: f64) -> Result<u32, Error> {
    if !shown.playing || !shown.model.is_animated() {
        *clock = 0.0;
        return Ok(0);
    }
    *clock += dt;
    let mut ticks = 0;
    while *clock >= TICK {
        *clock -= TICK;
        if ticks < MAX_TICKS {
            shown.model.advance()?;
            for shot in gpu.spawns.advance(shown.model.frame())? {
                gpu.renderer.remove_model(shot);
            }
            ticks += 1;
        }
    }
    Ok(ticks)
}

/// Whether anything drawn turns to face the camera: a billboarded joint of
/// the model, or what its move spawns (the shine).
fn faces_camera(shown: &Shown, spawns: &Spawns) -> bool {
    spawns.faces_camera()
        || shown.model.scene().roots.iter().any(|root| {
            root.joints
                .iter()
                .any(|joint| joint.flags & BILLBOARD_FIELD != 0)
        })
}

/// Pose the model and what its move spawns for this frame, facing their
/// billboards toward the camera, and upload them.
fn pose(
    shown: &mut Shown,
    gpu: &mut Gpu,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), Error> {
    let view = gpu.renderer.camera().view_matrix();
    gpu.posed_orbit = Some(gpu.renderer.orbit());
    shown.model.set_view(Some(view));
    // A new move spawns its own; let go of the last one's.
    if let Some(playback) = shown.model.fighter() {
        for replaced in gpu.spawns.follow(playback)? {
            gpu.renderer.remove_model(replaced);
        }
    }
    // Where each part the spawns follow is, found before the pose and read
    // from it after.
    let fighter = shown.model.fighter();
    let parts: Vec<_> = gpu
        .spawns
        .parts()
        .into_iter()
        .filter_map(|part| Some((part, fighter?.part_joint(part)?)))
        .collect();
    let scale = fighter.map_or(1.0, |playback| playback.scale());
    let (scene, work) = shown.model.evaluate()?;
    let joints: Vec<(u8, Mat4)> = parts
        .into_iter()
        .map(|(part, (root, joint))| (part, work.roots[root].joint_world_matrices[joint.0]))
        .collect();
    gpu.renderer
        .update_draw_work(queue, gpu.model, scene, work)?;
    let joint = |part| {
        joints
            .iter()
            .find(|(each, _)| *each == part)
            .map(|&(_, matrix)| matrix)
    };
    gpu.spawns
        .upload(&mut gpu.renderer, device, queue, joint, view, scale)?;
    Ok(())
}

impl Compositor<App> for Viewport {
    fn animating(&self) -> bool {
        // A pick in flight needs frames to arrive on.
        self.playing || !matches!(self.pick, Pick::None)
    }

    fn render(&mut self, f: &mut Frame<'_, App>) {
        self.poll_pick(f);
        if let Some([_, _, w, h]) = Self::slot(f) {
            let size = ((w.round() as u32).max(1), (h.round() as u32).max(1));
            if let Err(error) = self.draw(f, size) {
                tgg_app::log(&format!("viewport: {error}"));
                f.rt.state.stage.shown = None;
                self.gpu = None;
                self.pick = Pick::None;
            }
            Self::report(f);
            f.rt.state.stage.rejected.append(&mut self.refused);
        } else {
            // Off the stage, nothing draws and the clock waits.
            self.clock = 0.0;
        }
        self.playing =
            f.rt.state
                .stage
                .shown
                .as_ref()
                .is_some_and(|shown| shown.playing && shown.model.is_animated());
        let (device, queue, size) = (f.device, f.queue, f.size);
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        let gpu = &mut self.gpu;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark == VIEWPORT
                && let Some(gpu) = gpu.as_mut()
            {
                gpu.copy.draw(device, pass, site, &gpu.target);
            }
        });
    }
}
