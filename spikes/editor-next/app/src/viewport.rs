//! The viewport: hsd-render drawing a fighter on Rune's own GPU device.
//!
//! Rune lays out an `El::effect(VIEWPORT)` slot like any box. Each frame this
//! compositor finds that slot's rect, has hsd-render draw the fighter into a
//! texture of exactly that size (with its own depth buffer, in its own pass,
//! ahead of Rune's), then copies the texture into the slot while Rune draws
//! the rest of the window, so the slot's clip and painter order apply.

use crate::{State, fighter};
use hsd_render::{HsdRenderer, ModelId, PreparedGeometry, neutral_preview_lighting};
use melee_dat::MeleeFighterPlayback;
use rune::compose::{Compositor, DeviceKey, Frame};
use rune::render::{EffectSite, KIND_EFFECT};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

/// The effect slot's mark.
pub const VIEWPORT: u32 = 1;
/// hsd-render's output is raw GX colour; Rune's window target holds
/// sRGB-encoded values, so the bytes copy straight across.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Melee's animation clock.
const TICK: f64 = 1.0 / 60.0;
/// Catch-up cap after a stall, so a hitch never replays seconds of animation.
const MAX_TICKS: u32 = 4;

pub struct Viewport {
    iso: PathBuf,
    /// The costume file showing, and its playback.
    file: String,
    playback: Option<MeleeFighterPlayback>,
    error: Option<String>,
    gpu: Option<Gpu>,
    clock: f64,
    /// Rolling timings for the status line, in milliseconds.
    pub stats: Stats,
}

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub load_ms: f64,
    pub cpu_ms: f64,
    pub size: (u32, u32),
}

struct Gpu {
    key: DeviceKey,
    renderer: HsdRenderer,
    model: ModelId,
    view: wgpu::TextureView,
    size: (u32, u32),
    layout: wgpu::BindGroupLayout,
    bind: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// One copy pipeline per pass format and colour encoding.
    pipelines: HashMap<(wgpu::TextureFormat, bool), wgpu::RenderPipeline>,
}

impl Viewport {
    pub fn new(iso: PathBuf, file: &str) -> Self {
        let mut viewport = Self { iso, file: String::new(), playback: None, error: None, gpu: None, clock: 0.0, stats: Stats::default() };
        viewport.show(file);
        viewport
    }

    /// Load the costume `file` from the disc; the renderer rebuilds for it.
    fn show(&mut self, file: &str) {
        let started = Instant::now();
        self.file = file.to_owned();
        self.gpu = None;
        match fighter::load(&self.iso, file) {
            Ok(playback) => {
                self.playback = Some(playback);
                self.error = None;
            }
            Err(error) => {
                eprintln!("tgg-next: {file}: {error}");
                self.playback = None;
                self.error = Some(error.to_string());
            }
        }
        self.stats.load_ms = started.elapsed().as_secs_f64() * 1000.0;
    }

    /// The slot's rect in device pixels this frame, if it's on screen.
    fn slot(f: &Frame<'_, State>) -> Option<[f32; 4]> {
        f.rt.instances
            .iter()
            .enumerate()
            .find(|(n, i)| i.params[1] == KIND_EFFECT && f.rt.marks.get(*n).copied() == Some(VIEWPORT))
            .map(|(_, i)| i.rect)
    }

    /// Advance the animation, keep the renderer's target the slot's size, and
    /// draw the fighter into it. Records into `encoder` ahead of Rune's pass.
    fn draw_fighter(&mut self, f: &mut Frame<'_, State>, size: (u32, u32)) -> Result<(), fighter::Error> {
        let Some(playback) = self.playback.as_mut() else { return Ok(()) };
        let key = f.device_key();
        if self.gpu.as_ref().is_some_and(|gpu| gpu.key != key) {
            // Resources belong to the old device.
            self.gpu = None;
        }
        let orbit = f.rt.state.orbit;
        let gpu = match &mut self.gpu {
            Some(gpu) => gpu,
            None => {
                let (scene, work) = playback.evaluate()?;
                let geometry = PreparedGeometry::new(scene, work)?;
                let (renderer, model) =
                    HsdRenderer::with_model(f.device, f.queue, FORMAT, geometry, neutral_preview_lighting(), size, orbit)?;
                self.gpu.insert(Gpu::new(f.device, key, renderer, model, size))
            }
        };
        if gpu.size != size {
            gpu.resize(f.device, f.queue, size)?;
        }
        if f.rt.state.playing {
            self.clock += f.dt;
        }
        let mut ticks = 0;
        while self.clock >= TICK {
            self.clock -= TICK;
            if ticks < MAX_TICKS {
                playback.advance()?;
                ticks += 1;
            }
        }
        if ticks > 0 {
            let (scene, work) = playback.evaluate()?;
            gpu.renderer.update_draw_work(f.queue, gpu.model, scene, work)?;
        }
        gpu.renderer.set_orbit(f.queue, orbit)?;
        gpu.renderer.encode(f.encoder, &gpu.view);
        Ok(())
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, key: DeviceKey, renderer: HsdRenderer, model: ModelId, size: (u32, u32)) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport copy"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("viewport copy"), ..Default::default() });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport copy"),
            source: wgpu::ShaderSource::Wgsl(COPY_WGSL.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport copy"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let view = target(device, size);
        let bind = bind_group(device, &layout, &view, &sampler);
        Self { key, renderer, model, view, size, layout, bind, sampler, shader, pipeline_layout, pipelines: HashMap::new() }
    }

    fn resize(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32)) -> Result<(), fighter::Error> {
        self.renderer.resize(device, queue, size.0, size.1)?;
        self.view = target(device, size);
        self.bind = bind_group(device, &self.layout, &self.view, &self.sampler);
        self.size = size;
        Ok(())
    }

    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, srgb: bool) -> &wgpu::RenderPipeline {
        let (shader, layout) = (&self.shader, &self.pipeline_layout);
        self.pipelines.entry((format, srgb)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("viewport copy"),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    // sRGB-encoded frames take the bytes as they are; a linear frame decodes them.
                    entry_point: Some(if srgb { "fs_encoded" } else { "fs_linear" }),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    /// Copy the fighter into the slot, inside Rune's pass.
    fn draw_in(&mut self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'_>, site: &EffectSite<'_>) {
        let [x, y, w, h] = site.instance.rect;
        let (ox, oy) = (site.origin[0], site.origin[1]);
        let (tw, th) = (site.size[0] as f32, site.size[1] as f32);
        let (x, y) = (x - ox, y - oy);
        if w < 1.0 || h < 1.0 || x >= tw || y >= th || x + w <= 0.0 || y + h <= 0.0 {
            return;
        }
        // The slot's clip (window device pixels), within the pass's target.
        let clip = site.instance.clip;
        let x0 = (clip[0] - ox).max(x).max(0.0);
        let y0 = (clip[1] - oy).max(y).max(0.0);
        let x1 = (clip[2] - ox).min(x + w).min(tw);
        let y1 = (clip[3] - oy).min(y + h).min(th);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let pipeline = self.pipeline(device, site.format, site.srgb_blend).clone();
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_scissor_rect(x0 as u32, y0 as u32, (x1 - x0).ceil() as u32, (y1 - y0).ceil() as u32);
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &self.bind, &[]);
        pass.draw(0..3, 0..1);
        // Hand the pass back to Rune as it was.
        pass.set_viewport(0.0, 0.0, tw, th, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, site.size[0], site.size[1]);
    }
}

fn target(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn bind_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, sampler: &wgpu::Sampler) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("viewport copy"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

const COPY_WGSL: &str = r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}
@fragment fn fs_encoded(o: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSample(image, smp, o.uv).rgb, 1.0);
}
fn decode(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}
@fragment fn fs_linear(o: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(decode(textureSample(image, smp, o.uv).rgb), 1.0);
}
"#;

impl Compositor<State> for Viewport {
    fn animating(&self) -> bool {
        self.playback.is_some()
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let started = Instant::now();
        let want = f.rt.state.costume_file();
        if want != self.file {
            self.show(&want);
        }
        let rect = Self::slot(f);
        if let Some([_, _, w, h]) = rect {
            let size = ((w.round() as u32).max(1), (h.round() as u32).max(1));
            if let Err(error) = self.draw_fighter(f, size) {
                eprintln!("tgg-next: viewport: {error}");
                self.error = Some(error.to_string());
                self.playback = None;
                self.gpu = None;
            }
            self.stats.size = size;
        }
        self.stats.cpu_ms = self.stats.cpu_ms * 0.9 + started.elapsed().as_secs_f64() * 1000.0 * 0.1;
        f.rt.state.stats = self.stats;
        f.rt.state.error = self.error.clone();

        let (device, queue, size) = (f.device, f.queue, f.size);
        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);
        let gpu = &mut self.gpu;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if site.mark == VIEWPORT
                && let Some(gpu) = gpu.as_mut()
            {
                gpu.draw_in(device, pass, site);
            }
        });
    }
}
