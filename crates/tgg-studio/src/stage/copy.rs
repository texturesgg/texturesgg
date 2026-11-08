//! Copying the model's texture into the viewport's slot, inside Rune's pass.
//! hsd-render draws in a pass of its own, with its own depth buffer, into a
//! texture the slot's size; this draws that texture over the slot, clipped
//! to it, so the slot's clip and painter order hold.

use rune::render::EffectSite;
use std::collections::HashMap;

/// What hsd-render draws: raw GX color, which Rune's sRGB-encoded frames take
/// as it is.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub struct Copy {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// One pipeline per pass format and color encoding.
    pipelines: HashMap<(wgpu::TextureFormat, bool), wgpu::RenderPipeline>,
}

/// A texture the model is drawn into, and its binding for the copy.
pub struct Target {
    pub view: wgpu::TextureView,
    pub size: (u32, u32),
    bind: wgpu::BindGroup,
}

impl Copy {
    pub fn new(device: &wgpu::Device) -> Self {
        let texture = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stage copy"),
            entries: &[texture, sampler],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stage copy"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Self {
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("stage copy"),
                ..Default::default()
            }),
            shader: device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("stage copy"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            }),
            layout,
            pipeline_layout,
            pipelines: HashMap::new(),
        }
    }

    /// A texture `size` device pixels to draw the model into.
    pub fn target(&self, device: &wgpu::Device, (width, height): (u32, u32)) -> Target {
        let view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("stage target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stage copy"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Target {
            view,
            size: (width, height),
            bind,
        }
    }

    /// Draw `target` over the slot `site` describes, clipped to the slot.
    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        site: &EffectSite<'_>,
        target: &Target,
    ) {
        let [x, y, w, h] = site.instance.rect;
        let ([ox, oy], [tw, th]) = (site.origin, site.size.map(|side| side as f32));
        let (x, y) = (x - ox, y - oy);
        let clip = site.instance.clip;
        let x0 = (clip[0] - ox).max(x).max(0.0);
        let y0 = (clip[1] - oy).max(y).max(0.0);
        let x1 = (clip[2] - ox).min(x + w).min(tw);
        let y1 = (clip[3] - oy).min(y + h).min(th);
        if w < 1.0 || h < 1.0 || x1 <= x0 || y1 <= y0 {
            return;
        }
        let pipeline = self.pipeline(device, site.format, site.srgb_blend);
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_scissor_rect(
            x0 as u32,
            y0 as u32,
            (x1 - x0).ceil() as u32,
            (y1 - y0).ceil() as u32,
        );
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &target.bind, &[]);
        pass.draw(0..3, 0..1);
        // Hand the pass back to Rune as it was.
        pass.set_viewport(0.0, 0.0, tw, th, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, site.size[0], site.size[1]);
    }

    fn pipeline(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        srgb: bool,
    ) -> &wgpu::RenderPipeline {
        let (shader, layout) = (&self.shader, &self.pipeline_layout);
        self.pipelines.entry((format, srgb)).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("stage copy"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    // An sRGB-encoded frame takes the bytes as they are; a linear one decodes them.
                    entry_point: Some(if srgb { "fs_encoded" } else { "fs_linear" }),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
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
}

const SHADER: &str = r#"
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
