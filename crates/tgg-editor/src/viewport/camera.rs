//! The camera under the pointer: orbit, pan and zoom, and carrying a view
//! from one model to the next.

use super::{ViewState, Viewport};
use gpui::{
    Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent,
    Window, px,
};
use hsd_render::{CameraView, Orbit};

const ORBIT_RADIANS_PER_PIXEL: f64 = 0.01;

/// A press that moves less than this (in logical pixels) before release is a
/// click, not an orbit.
const CLICK_SLOP: f32 = 4.0;

impl Viewport {
    pub(super) fn start_drag(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Focus the viewport so its keys (Space to pause) apply.
        window.focus(&self.focus, cx);
        self.drag_from = Some(event.position);
        self.press = Some(event.position);
    }

    pub(super) fn end_drag(
        &mut self,
        event: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.drag_from = None;
        let (Some(press), Some((bounds, scale))) = (self.press.take(), self.painted) else {
            return;
        };
        let moved = event.position - press;
        if f32::from(moved.x).abs() > CLICK_SLOP || f32::from(moved.y).abs() > CLICK_SLOP {
            return;
        }
        let local = event.position - bounds.origin;
        let (x, y) = (f32::from(local.x) * scale, f32::from(local.y) * scale);
        if x >= 0.0 && y >= 0.0 {
            self.picks.click((x as u32, y as u32));
            // The pick is encoded with the next frame.
            cx.notify();
        }
    }

    /// A release outside the viewport ends an orbit but is never a click.
    pub(super) fn cancel_drag(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag_from = None;
        self.press = None;
    }

    /// Where the camera looks and what the model is doing, to carry into
    /// another viewport of the same costume.
    pub fn view_state(&self) -> ViewState {
        let clock = self.clock();
        ViewState {
            orbit: self.orbit,
            animation: self.playback().map(|playback| playback.current()),
            frame: clock.as_ref().map_or(0.0, |clock| clock.frame),
            rate: clock.as_ref().map_or(1.0, |clock| clock.rate),
            paused: self.is_paused(),
        }
    }

    /// Look and move as another viewport did (`view_state`): the same camera,
    /// move, speed, and frame, paused or playing.
    pub fn apply_view_state(&mut self, state: ViewState, cx: &mut Context<Self>) {
        self.orbit = state.orbit;
        if let Some(animation) = state.animation {
            let current = self.playback().map(|playback| playback.current());
            if current != Some(animation) {
                let _ = self.play(animation, cx);
            }
        }
        if self.is_animated() {
            let _ = self.set_rate(state.rate, cx);
            // Seeking pauses on the frame; resume if it was playing.
            if self.seek(state.frame, cx).is_ok() && !state.paused {
                self.toggle_playback(cx);
            }
        }
        self.redraw(cx);
    }

    /// Look at the whole model from the front again.
    pub fn reset_camera(&mut self, cx: &mut Context<Self>) {
        self.orbit = CameraView::Front.orbit();
        self.redraw(cx);
    }

    /// Start panning with the right or middle button.
    pub(super) fn start_pan(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.drag_from = Some(event.position);
        self.press = None;
    }

    /// Left-drag orbits; Shift+left, right, or middle drag pans.
    pub(super) fn drag(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.drag_from else { return };
        let pan = match event.pressed_button {
            Some(MouseButton::Left) => event.modifiers.shift,
            Some(MouseButton::Right | MouseButton::Middle) => true,
            _ => {
                self.drag_from = None;
                return;
            }
        };
        let delta = event.position - from;
        self.drag_from = Some(event.position);
        let (dx, dy) = (f64::from(f32::from(delta.x)), f64::from(f32::from(delta.y)));
        let orbit = match (pan, self.painted) {
            (true, Some((bounds, scale))) => {
                let scale = f64::from(scale);
                let size = |length: Pixels| (f64::from(f32::from(length)) * scale).max(1.0) as u32;
                self.orbit.panned(
                    dx * scale,
                    dy * scale,
                    size(bounds.size.width),
                    size(bounds.size.height),
                )
            }
            (true, None) => return,
            (false, _) => Orbit {
                yaw: self.orbit.yaw - dx * ORBIT_RADIANS_PER_PIXEL,
                pitch: self.orbit.pitch + dy * ORBIT_RADIANS_PER_PIXEL,
                ..self.orbit
            },
        };
        if let Ok(orbit) = self.clamp(orbit) {
            self.orbit = orbit;
            self.redraw(cx);
        }
    }

    /// The orbit within what the model allows; a framed stage reaches out
    /// to its whole backdrop.
    fn clamp(&self, orbit: Orbit) -> hsd_render::Result<Orbit> {
        match &self.gpu {
            Some(gpu) => gpu.renderer.clamp_orbit(orbit),
            None => orbit.clamped(),
        }
    }

    pub(super) fn zoom(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f64::from(f32::from(event.delta.pixel_delta(px(20.0)).y));
        let orbit = Orbit {
            zoom: self.orbit.zoom * (-delta * 0.002).exp(),
            ..self.orbit
        };
        if let Ok(orbit) = self.clamp(orbit) {
            self.orbit = orbit;
            self.redraw(cx);
        }
    }
}
