//! The 3D viewport: hsd-render drawing into an offscreen target on gpui's
//! shared wgpu device, composited by gpui as a surface.
//!
//! Rendering happens in a canvas paint callback, which receives the exact
//! layout bounds, so the target always matches the element in device pixels.
//! GPU resources are rebuilt whenever gpui's device changes or is lost.
//!
//! A click (a press and release without dragging) picks: the next frame
//! encodes an hsd-render pick at the pointer, the readback arrives a frame or
//! two later, and the viewport emits the textures under the pointer. A click
//! made while a pick is in flight waits for it.

mod camera;
mod frame;

use crate::Error;
use dat_parser::hsd::scene::{HsdTextureIndex, HsdTextureSourceId};
use gpui::{
    Bounds, Context, EventEmitter, FocusHandle, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, Point, Render, Styled, Window, canvas, div,
};
use gpui_wgpu::{WgpuContextHandle, WgpuRenderTarget};
use hsd_render::{CameraView, HsdRenderer, ModelId, Orbit, PendingPick};
use melee_dat::MeleeFighterPlayback;
use melee_dat::MeleeModel;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::time::Instant;

/// What a click in the viewport landed on, or an edit it couldn't show.
pub(crate) enum ViewportEvent {
    /// The textures of the surface under the pointer, most defining first,
    /// each as every descriptor pair that draws its pixels; empty for the
    /// background.
    Picked(Vec<Vec<HsdTextureSourceId>>),
    /// Edited pixels the renderer refused (the wrong size, say). The model
    /// keeps showing what it showed before; the viewport carries on.
    TextureRejected(String),
}

/// A viewport's camera and playback, carried from one viewport to another.
#[derive(Clone, Copy, Debug)]
pub struct ViewState {
    orbit: Orbit,
    animation: Option<usize>,
    frame: f32,
    rate: f32,
    paused: bool,
}

/// Where a playing model is in its animation, for the timeline.
pub struct Clock {
    pub label: String,
    pub frame: f32,
    pub end_frame: f32,
    pub rate: f32,
}

struct Gpu {
    context: WgpuContextHandle,
    target: WgpuRenderTarget,
    renderer: HsdRenderer,
    /// The viewport's model among the renderer's: what edits, highlights
    /// and picks address.
    model: ModelId,
}

/// Clicks waiting to pick, and the one readback in flight (`P`, a
/// [`PendingPick`]). A click made while a pick is in flight waits for it, and
/// a newer waiting click replaces an older one: where the user clicked last
/// is what they want picked.
struct Picks<P> {
    /// The latest click not yet encoded, in device pixels.
    waiting: Option<(u32, u32)>,
    in_flight: Option<P>,
}

impl<P> Picks<P> {
    const fn new() -> Self {
        Self {
            waiting: None,
            in_flight: None,
        }
    }

    fn click(&mut self, at: (u32, u32)) {
        self.waiting = Some(at);
    }

    /// The click to encode this frame: the waiting one, once no pick is in
    /// flight.
    fn next(&mut self) -> Option<(u32, u32)> {
        if self.in_flight.is_some() {
            return None;
        }
        self.waiting.take()
    }

    fn start(&mut self, pending: P) {
        self.in_flight = Some(pending);
    }

    /// Whether a click waits to pick or a readback is in flight.
    fn busy(&self) -> bool {
        self.waiting.is_some() || self.in_flight.is_some()
    }

    fn in_flight(&self) -> Option<&P> {
        self.in_flight.as_ref()
    }

    /// The pick in flight resolved, or failed; a waiting click can go next.
    fn finish(&mut self) {
        self.in_flight = None;
    }

    /// Forget every click: a readback on a dropped device never resolves.
    fn clear(&mut self) {
        *self = Self::new();
    }
}

pub(crate) struct Viewport {
    model: MeleeModel,
    /// The latest edited pixels, and their size, per scene texture. Uploaded
    /// on the next frame, and again whenever the GPU resources are rebuilt.
    edited: BTreeMap<usize, ((u32, u32), Vec<u8>)>,
    pending: Vec<usize>,
    /// Edits the renderer refused, to report once painting is done.
    rejected: Vec<String>,
    focus: FocusHandle,
    gpu: Option<Gpu>,
    orbit: Orbit,
    drag_from: Option<Point<Pixels>>,
    /// Where the current press started, while it can still be a click.
    press: Option<Point<Pixels>>,
    /// The painted bounds and scale, to turn a click into a device pixel.
    painted: Option<(Bounds<Pixels>, f32)>,
    picks: Picks<PendingPick>,
    /// Scene textures tinted for the editor's selection, and whether the
    /// renderer still needs them.
    highlight: Vec<HsdTextureIndex>,
    highlight_dirty: bool,
    /// Whether the model or camera changed since the last rendered frame; a
    /// repaint without a change shows the last frame again.
    dirty: bool,
    clock_start: Option<Instant>,
    /// When playback was paused; the clock resumes from the same frame.
    paused_at: Option<Instant>,
    ticks: u64,
    frame_times: VecDeque<Instant>,
    cpu_frame_ms: f64,
    last_report: Option<Instant>,
    /// Mirror the status to stderr every couple of seconds.
    report: bool,
    error: Option<String>,
}

impl EventEmitter<ViewportEvent> for Viewport {}

impl Viewport {
    pub fn new(model: MeleeModel, focus: FocusHandle) -> Self {
        Self {
            model,
            edited: BTreeMap::new(),
            pending: Vec::new(),
            rejected: Vec::new(),
            focus,
            gpu: None,
            orbit: CameraView::Front.orbit(),
            drag_from: None,
            press: None,
            painted: None,
            picks: Picks::new(),
            highlight: Vec::new(),
            highlight_dirty: false,
            dirty: true,
            clock_start: None,
            paused_at: None,
            ticks: 0,
            frame_times: VecDeque::new(),
            cpu_frame_ms: 0.0,
            last_report: None,
            report: true,
            error: None,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// Whether the model is playing, so every frame shows a new pose.
    pub fn is_playing(&self) -> bool {
        self.is_animated() && !self.is_paused()
    }

    /// Show a change on the next frame.
    fn redraw(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        cx.notify();
    }

    /// Whether the viewport must keep drawing frames on its own: while the
    /// model plays, a pick is pending, or the GPU is yet to arrive.
    fn needs_frames(&self) -> bool {
        self.is_playing() || self.picks.busy() || (self.gpu.is_none() && self.error.is_none())
    }

    /// Whether the model animates, so pausing means something.
    pub fn is_animated(&self) -> bool {
        self.model.is_animated()
    }

    /// Where the playing model is in its animation.
    pub fn clock(&self) -> Option<Clock> {
        if !self.model.is_animated() {
            return None;
        }
        let end_frame = self.model.end_frame();
        let (label, frame) = match self.model.fighter() {
            Some(playback) => (
                playback
                    .animations()
                    .get(playback.current())
                    .map(|animation| animation.label())
                    .unwrap_or_default(),
                self.model.frame(),
            ),
            // A stage's groups loop at their own lengths; the counter wraps
            // at the longest so it keeps moving with them.
            None => ("Stage".into(), self.model.frame() % end_frame.max(1.0)),
        };
        Some(Clock {
            label,
            frame,
            end_frame,
            rate: self.model.rate(),
        })
    }

    /// The fighter's playback, when the model plays its animations.
    pub fn playback(&self) -> Option<&MeleeFighterPlayback> {
        self.model.fighter()
    }

    /// Play the fighter's animation `index` from its start, resuming if
    /// paused. A refused animation leaves the current one playing.
    pub fn play(&mut self, index: usize, cx: &mut Context<Self>) -> Result<(), Error> {
        let Some(playback) = self.model.fighter_mut() else {
            return Err(Error::NoAnimations);
        };
        playback.play(index)?;
        self.clock_start = None;
        self.paused_at = None;
        self.ticks = 0;
        self.frame_times.clear();
        self.redraw(cx);
        self.upload_pose()
    }

    /// Show `frame` of the playing animation, pausing so it stays shown.
    pub fn seek(&mut self, frame: f32, cx: &mut Context<Self>) -> Result<(), Error> {
        if !self.model.is_animated() {
            return Ok(());
        }
        self.model.seek(frame)?;
        if self.paused_at.is_none() {
            self.paused_at = Some(Instant::now());
        }
        self.redraw(cx);
        self.upload_pose()
    }

    /// Play at `rate` times the game's speed.
    pub fn set_rate(&mut self, rate: f32, cx: &mut Context<Self>) -> Result<(), Error> {
        if !self.model.is_animated() {
            return Ok(());
        }
        self.model.set_rate(rate)?;
        cx.notify();
        Ok(())
    }

    /// Pause or resume the idle. Resuming shifts the clock by the time spent
    /// paused, so playback continues from the paused frame.
    pub fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        match self.paused_at.take() {
            Some(paused_at) => {
                if let Some(start) = &mut self.clock_start {
                    *start += now.duration_since(paused_at);
                }
                // The frame rate restarts; the pause is no frame time.
                self.frame_times.clear();
            }
            None => self.paused_at = Some(now),
        }
        self.redraw(cx);
    }

    /// One-line status for the editor chrome.
    pub fn status(&self) -> String {
        if let Some(error) = &self.error {
            return format!("error: {error}");
        }
        let Some(gpu) = &self.gpu else {
            return "waiting for the GPU".into();
        };
        let fps = match (self.frame_times.front(), self.frame_times.back()) {
            (Some(first), Some(last)) if self.frame_times.len() > 1 => {
                (self.frame_times.len() - 1) as f64 / last.duration_since(*first).as_secs_f64()
            }
            _ => 0.0,
        };
        let size = gpu.target.size();
        // Frames come only with changes when nothing plays, so a rate
        // would mislead.
        let rate = if self.is_playing() {
            format!("{fps:.0} fps")
        } else if self.is_paused() {
            "paused".into()
        } else {
            "still".into()
        };
        format!(
            "{rate} · {:.2} ms cpu · {}×{} · {}",
            self.cpu_frame_ms,
            size.width.0,
            size.height.0,
            gpu.context.adapter_info().name
        )
    }

    /// Tint the surfaces that draw one of `uses` (a document texture's
    /// descriptor pairs); empty clears the tint.
    pub fn set_highlight(&mut self, uses: &[HsdTextureSourceId], cx: &mut Context<Self>) {
        let scene = self.model.scene();
        let scene_textures: Vec<HsdTextureIndex> = scene
            .textures
            .iter()
            .enumerate()
            .filter(|(_, texture)| uses.contains(&texture.id))
            .map(|(index, _)| HsdTextureIndex(index))
            .collect();
        if scene_textures != self.highlight {
            self.highlight = scene_textures;
            self.highlight_dirty = true;
            self.redraw(cx);
        }
    }

    /// Show edited pixels: each decoded use of an edited texture, with its
    /// (width, height), matched to its scene texture by image and palette
    /// descriptors.
    pub fn update_textures<'a>(
        &mut self,
        decoded: impl IntoIterator<Item = (HsdTextureSourceId, (u32, u32), &'a [u8])>,
        cx: &mut Context<Self>,
    ) {
        let scene = self.model.scene();
        for (id, size, pixels) in decoded {
            let scene_texture = scene.textures.iter().position(|texture| texture.id == id);
            if let Some(index) = scene_texture {
                self.edited.insert(index, (size, pixels.to_vec()));
                self.pending.push(index);
            }
        }
        self.redraw(cx);
    }

    /// Without the status on stderr: for many viewports at once.
    pub fn quiet(mut self) -> Self {
        self.report = false;
        self
    }

    /// What the last painted frame took on the CPU, in milliseconds.
    pub fn cpu_frame_ms(&self) -> f64 {
        self.cpu_frame_ms
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    /// Show `model` in place of the current one, from the same camera and
    /// at the same point of the same animation: the file's bytes changed in
    /// a way the renderer cannot patch, such as its vertex colors.
    pub fn replace_model(&mut self, model: MeleeModel, cx: &mut Context<Self>) {
        let state = self.view_state();
        self.model = model;
        self.drop_gpu();
        self.apply_view_state(state, cx);
    }
}

impl Render for Viewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Draw every frame only while something moves on its own; any other
        // change asks for a frame through `redraw`.
        if self.needs_frames() {
            window.request_animation_frame();
        }
        let viewport = cx.entity();
        div()
            .id("viewport")
            .track_focus(&self.focus)
            .key_context("Viewport")
            .absolute()
            .inset_0()
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::end_drag))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::cancel_drag))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::start_pan))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::start_pan))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::cancel_drag))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::cancel_drag))
            .on_mouse_move(cx.listener(Self::drag))
            .on_scroll_wheel(cx.listener(Self::zoom))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        viewport.update(cx, |viewport, cx| viewport.paint(bounds, window, cx));
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Picks;

    #[test]
    fn a_click_during_a_pick_waits_for_it() {
        let mut picks = Picks::new();
        picks.click((1, 2));
        assert_eq!(picks.next(), Some((1, 2)));
        picks.start("first");
        picks.click((3, 4));
        assert_eq!(picks.next(), None, "one readback at a time");
        picks.finish();
        assert_eq!(picks.next(), Some((3, 4)));
        assert_eq!(picks.next(), None, "a click picks once");
    }

    #[test]
    fn the_latest_waiting_click_wins() {
        let mut picks = Picks::new();
        picks.start("first");
        picks.click((1, 2));
        picks.click((3, 4));
        picks.finish();
        assert_eq!(picks.next(), Some((3, 4)));
    }

    #[test]
    fn clearing_forgets_a_pick_that_never_resolves() {
        let mut picks = Picks::new();
        picks.start("on a lost device");
        picks.click((1, 2));
        picks.clear();
        assert!(picks.in_flight().is_none());
        assert_eq!(picks.next(), None);
        picks.click((3, 4));
        assert_eq!(picks.next(), Some((3, 4)), "later clicks pick again");
    }
}
