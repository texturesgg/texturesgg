//! Drawing a frame: the renderer on gpui's device, the animation clock's
//! ticks, and reading back what a click landed on.

use super::{Gpu, Viewport, ViewportEvent};
use crate::Error;
use dat_parser::hsd::scene::HsdTextureIndex;
use gpui::{Bounds, Context, DevicePixels, Pixels, SurfaceSource, Window, size};
use gpui_wgpu::{WgpuContextHandle, WgpuRenderTarget};
use hsd_render::{HsdRenderer, neutral_preview_lighting};
use std::time::{Duration, Instant};

/// HSD output is raw GX color; gpui-ce composites into a non-sRGB surface.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Melee's animation clock.
const TICK: Duration = Duration::from_nanos(16_666_667);

/// Cap catch-up after a stall so a hitch never replays seconds of animation.
const MAX_TICKS_PER_FRAME: u32 = 4;

impl Viewport {
    /// Render one frame into the target and paint it at `bounds`.
    pub(super) fn paint(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        let scale = window.scale_factor();
        self.painted = Some((bounds, scale));
        let device_size = size(
            DevicePixels((f32::from(bounds.size.width) * scale).round().max(1.0) as i32),
            DevicePixels((f32::from(bounds.size.height) * scale).round().max(1.0) as i32),
        );
        let rendered = self.render_frame(window, device_size);
        if !self.rejected.is_empty() {
            // Painting can't update other entities; report once it's done.
            let rejected = std::mem::take(&mut self.rejected);
            cx.defer_in(window, move |_, _, cx| {
                for error in rejected {
                    cx.emit(ViewportEvent::TextureRejected(error));
                }
            });
        }
        if let Err(error) = rendered {
            crate::log(&format!("viewport error: {error}"));
            self.error = Some(error.to_string());
            self.drop_gpu();
            return;
        }
        let Some(gpu) = &self.gpu else { return };
        window.paint_surface(
            bounds,
            SurfaceSource::Texture {
                texture: gpu.target.texture(),
                size: gpu.target.size(),
            },
        );
        self.poll_pick(window, cx);
        self.cpu_frame_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.frame_times.push_back(started);
        while self.frame_times.len() > 120 {
            self.frame_times.pop_front();
        }
        // Mirror the status to stderr so headless runs can be measured.
        if self.report
            && self
                .last_report
                .is_none_or(|last| started.duration_since(last) >= Duration::from_secs(2))
        {
            self.last_report = Some(started);
            crate::log(&format!("viewport: {}", self.status()));
        }
    }

    fn render_frame(
        &mut self,
        window: &Window,
        device_size: gpui::Size<DevicePixels>,
    ) -> Result<(), Error> {
        let Some(context) = WgpuContextHandle::from_window(window) else {
            self.drop_gpu();
            return Ok(());
        };
        if context.device_lost()
            || self
                .gpu
                .as_ref()
                .is_some_and(|gpu| !gpu.context.is_same_device(&context))
        {
            // Resources belong to the old device; rebuild after recovery.
            self.drop_gpu();
            if context.device_lost() {
                return Ok(());
            }
        }
        let (width, height) = (device_size.width.0 as u32, device_size.height.0 as u32);
        match &mut self.gpu {
            Some(gpu) if gpu.target.size() != device_size => {
                gpu.target.resize(&context, device_size);
                gpu.renderer
                    .resize(context.device(), context.queue(), width, height)?;
                self.dirty = true;
            }
            Some(_) => {}
            None => {
                let geometry = crate::geometry_of(&mut self.model)?;
                let (renderer, model) = HsdRenderer::with_model(
                    context.device(),
                    context.queue(),
                    TARGET_FORMAT,
                    geometry,
                    neutral_preview_lighting(),
                    (width, height),
                    self.orbit,
                )?;
                let target = WgpuRenderTarget::with_format(&context, device_size, TARGET_FORMAT);
                // A rebuilt renderer starts from the original pixels and no
                // tint; reapply every edit and the selection.
                self.pending = self.edited.keys().copied().collect();
                self.highlight_dirty = true;
                self.dirty = true;
                self.gpu = Some(Gpu {
                    context: context.clone(),
                    target,
                    renderer,
                    model,
                });
            }
        }
        self.advance_animation()?;
        let pick = self.picks.next();
        let changed = std::mem::take(&mut self.dirty)
            || !self.pending.is_empty()
            || self.highlight_dirty
            || pick.is_some();
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        if !changed {
            // The target still holds this frame.
            return Ok(());
        }
        for index in self.pending.drain(..) {
            let Some((size, pixels)) = self.edited.get(&index) else {
                continue;
            };
            // A refused update is that edit's problem, not the renderer's:
            // forget it, so a rebuilt renderer never replays it, and carry on.
            if let Err(error) = gpu.renderer.update_scene_texture(
                context.queue(),
                gpu.model,
                HsdTextureIndex(index),
                *size,
                pixels,
            ) {
                crate::log(&format!("texture update rejected: {error}"));
                self.edited.remove(&index);
                self.rejected.push(error.to_string());
            }
        }
        if std::mem::take(&mut self.highlight_dirty) {
            gpu.renderer
                .set_highlight(context.queue(), gpu.model, &self.highlight)?;
        }
        gpu.renderer.set_orbit(context.queue(), self.orbit)?;
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("editor viewport"),
                });
        gpu.renderer.encode(&mut encoder, gpu.target.view());
        // Pick in the same submission, so it sees this frame's pose.
        // A pick that fails is reported, never fatal to the viewport.
        let readback = match pick {
            Some(at) if at.0 < width && at.1 < height => gpu
                .renderer
                .encode_pick(context.device(), &mut encoder, at)
                .inspect_err(|error| crate::log(&format!("pick failed: {error}")))
                .ok(),
            _ => None,
        };
        context.queue().submit([encoder.finish()]);
        if let Some(readback) = readback {
            self.picks.start(readback.map());
        }
        Ok(())
    }

    /// Drop the GPU resources, to rebuild on a later frame. A pick in flight
    /// on the old device never resolves, so clicks go with them.
    pub(super) fn drop_gpu(&mut self) {
        self.gpu = None;
        self.picks.clear();
    }

    /// Collect a finished pick and report what it landed on.
    fn poll_pick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(picking), Some(gpu)) = (self.picks.in_flight(), &self.gpu) else {
            return;
        };
        if let Err(error) = gpu.context.device().poll(wgpu::PollType::Poll) {
            crate::log(&format!("pick poll failed: {error}"));
        }
        let Some(picked) = picking.take() else {
            return;
        };
        self.picks.finish();
        // A pick of anything but the viewport's own model picks none of its
        // textures.
        let packet = match picked {
            Ok(picked) => picked
                .and_then(|pick| gpu.renderer.resolve_pick(pick))
                .filter(|(model, _)| *model == gpu.model)
                .map(|(_, packet)| packet),
            Err(error) => {
                crate::log(&format!("pick failed: {error}"));
                return;
            }
        };
        let scene = self.model.scene();
        let textures = packet
            .and_then(|packet| gpu.renderer.packet_textures(gpu.model, packet).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|texture| {
                texture
                    .scene_textures
                    .iter()
                    .filter_map(|index| Some(scene.textures.get(index.0)?.id))
                    .collect()
            })
            .collect();
        // Painting can't update other entities; report once it's done.
        cx.defer_in(window, move |_, _, cx| {
            cx.emit(ViewportEvent::Picked(textures))
        });
    }

    /// Advance the animation at Melee's 60 Hz clock, independent of display rate.
    fn advance_animation(&mut self) -> Result<(), Error> {
        if !self.is_animated() || self.paused_at.is_some() {
            return Ok(());
        }
        let now = Instant::now();
        let start = *self.clock_start.get_or_insert(now);
        let due = (now.duration_since(start).as_nanos() / TICK.as_nanos()) as u64;
        let pending = due.saturating_sub(self.ticks);
        if pending == 0 {
            return Ok(());
        }
        let steps = pending.min(u64::from(MAX_TICKS_PER_FRAME));
        for _ in 0..steps {
            self.model.advance()?;
        }
        // Drop any backlog beyond the cap instead of replaying it later.
        self.ticks = due;
        self.upload_pose()
    }

    /// Draw the fighter's current pose.
    pub(super) fn upload_pose(&mut self) -> Result<(), Error> {
        let Some(gpu) = &mut self.gpu else {
            return Ok(());
        };
        if !self.model.is_animated() {
            return Ok(());
        }
        let (scene, work) = self.model.evaluate()?;
        self.dirty = true;
        Ok(gpu
            .renderer
            .update_draw_work(gpu.context.queue(), gpu.model, scene, work)?)
    }
}
