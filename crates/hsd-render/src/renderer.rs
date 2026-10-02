//! wgpu backend for prepared HSD geometry.
//!
//! The renderer borrows a caller-owned device and queue and encodes into a
//! caller-provided target, so a host (an offscreen capture, a gpui surface)
//! owns presentation. GX blends and writes raw 8-bit values, which the display
//! interprets as sRGB: every color stays raw, so targets must be non-sRGB
//! formats (no decode on sampling, no encode on output).

use crate::camera::{Camera, Orbit};
use crate::error::{HsdRenderError, Result, invalid_scene};
use crate::geometry::{
    BASE_TEX_COORD_SETS, CullMode, PacketIndex, PreparedGeometry, floats_per_vertex,
    tex_coord_location, tex_coord_offset,
};
use crate::lighting::HsdLightingPreset;
use crate::material::{AddressMode, FilterMode, MAX_TEXTURE_STAGES};
use crate::pick::{PICK_FORMAT, PickReadback, PickedTexture, stages_by_prominence};
use crate::shader::{
    GLOBAL_CAMERA_POSITION_FLOAT, GLOBAL_LIGHTING_FLOAT, GLOBAL_UNIFORM_FLOATS,
    MATERIAL_JOINT_POSITION_OFFSET_BYTES, material_shader, material_uniforms, pick_shader,
};
use dat_parser::hsd::draw::HsdEvaluatedDrawWork;
use dat_parser::hsd::pe::{HsdBlendFactor, HsdBlendMode, HsdCompare};
use dat_parser::hsd::scene::{HsdScene, HsdTextureIndex};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// The raw 8-bit color the renderer clears to, matching the site preview. A
/// host that surrounds the viewport paints the same.
pub const BACKGROUND_RGB: [u8; 3] = [5, 5, 9];
const BACKGROUND_COLOR: wgpu::Color = wgpu::Color {
    r: BACKGROUND_RGB[0] as f64 / 255.0,
    g: BACKGROUND_RGB[1] as f64 / 255.0,
    b: BACKGROUND_RGB[2] as f64 / 255.0,
    a: 1.0,
};
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

struct GpuPacket {
    pipeline: usize,
    bind_group: wgpu::BindGroup,
    material_buffer: wgpu::Buffer,
}

/// Pick pipelines, built on the first pick, and the id target, sized to the
/// color target.
struct PickState {
    pipelines: Vec<wgpu::RenderPipeline>,
    /// Each packet's pick pipeline. A packet that writes no color still
    /// draws, writing depth only, so it occludes what it hides on screen.
    packet_pipelines: Vec<usize>,
    target: Option<(u32, u32, wgpu::TextureView)>,
}

pub struct HsdRenderer {
    geometry: PreparedGeometry,
    lighting: HsdLightingPreset,
    width: u32,
    height: u32,
    orbit: Orbit,
    camera: Camera,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    pipeline_layout: wgpu::PipelineLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    pick: Option<PickState>,
    /// Packets tinted for an editor selection.
    highlighted: Vec<bool>,
    packets: Vec<GpuPacket>,
    /// One per `geometry.textures`, for in-place updates.
    textures: Vec<wgpu::Texture>,
    depth_view: wgpu::TextureView,
}

impl HsdRenderer {
    /// Upload prepared geometry and build one pipeline per distinct material
    /// program and fixed-function state.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_format: wgpu::TextureFormat,
        geometry: PreparedGeometry,
        lighting: HsdLightingPreset,
        (width, height): (u32, u32),
        orbit: Orbit,
    ) -> Result<Self> {
        lighting.validate()?;
        if target_format.is_srgb() {
            return Err(HsdRenderError::SrgbTarget(target_format));
        }
        validate_dimensions(device, width, height)?;
        let orbit = clamp_orbit(&geometry, orbit, width, height)?;
        let camera = Camera::frame(
            &geometry.bounds,
            geometry.focus.as_ref(),
            width,
            height,
            orbit,
        )?;
        geometry.validate_reflections(&camera.view)?;
        // Checked here because a browser reports a failed texture to the
        // device's uncaptured-error handler, not to this call.
        let limit = device.limits().max_texture_dimension_2d;
        if let Some(texture) = geometry
            .textures
            .iter()
            .find(|texture| texture.width > limit || texture.height > limit)
        {
            return invalid_scene(format!(
                "a {}x{} texture is over this device's limit of {limit} pixels a side",
                texture.width, texture.height
            ));
        }

        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HSD globals"),
            entries: &[uniform_entry(0)],
        });
        let mut material_entries = vec![uniform_entry(0)];
        for index in 0..MAX_TEXTURE_STAGES as u32 {
            material_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 1 + index * 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            material_entries.push(wgpu::BindGroupLayoutEntry {
                binding: 2 + index * 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            });
        }
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("HSD material"),
            entries: &material_entries,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("HSD pipeline layout"),
            bind_group_layouts: &[Some(&globals_layout), Some(&material_layout)],
            ..Default::default()
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("HSD evaluated vertices"),
            contents: bytemuck::cast_slice(&geometry.vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("HSD source-ordered triangle indices"),
            contents: bytemuck::cast_slice(&geometry.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("HSD camera and lighting"),
            contents: bytemuck::cast_slice(&global_uniforms(&camera, &lighting)),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("HSD globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let fallback = create_texture(device, queue, "HSD white fallback", (1, 1), &[255; 4])
            .create_view(&Default::default());
        let textures: Vec<_> = geometry
            .textures
            .iter()
            .map(|texture| {
                let label = format!("HSD texture (scene textures {:?})", texture.scene_textures);
                create_texture(
                    device,
                    queue,
                    &label,
                    (texture.width, texture.height),
                    &texture.rgba,
                )
            })
            .collect();
        let texture_views: Vec<_> = textures
            .iter()
            .map(|texture| texture.create_view(&Default::default()))
            .collect();
        let mut samplers = HashMap::new();
        let mut sampler_for = |key: (AddressMode, AddressMode, FilterMode)| {
            samplers
                .entry(key)
                .or_insert_with(|| {
                    device.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some("HSD stage sampler"),
                        address_mode_u: address_mode(key.0),
                        address_mode_v: address_mode(key.1),
                        mag_filter: filter_mode(key.2),
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    })
                })
                .clone()
        };

        let mut pipeline_cache = HashMap::new();
        let mut pipelines = Vec::new();
        let mut packets = Vec::with_capacity(geometry.packets.len());
        for (index, packet) in geometry.packets.iter().enumerate() {
            let material = &packet.material;
            let shader = material_shader(material);
            let key = PipelineKey::new(packet.cull_mode, material, shader);
            let pipeline = *pipeline_cache.entry(key.clone()).or_insert_with(|| {
                pipelines.push(create_pipeline(
                    device,
                    &pipeline_layout,
                    target_format,
                    geometry.tex_coord_sets,
                    &key,
                ));
                pipelines.len() - 1
            });
            let material_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!(
                    "HSD material PObj {:#x}",
                    packet.polygon_source_id.0
                )),
                contents: bytemuck::cast_slice(&material_uniforms(
                    material,
                    packet.joint_position,
                    index as u32 + 1,
                )),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let mut entries = vec![wgpu::BindGroupEntry {
                binding: 0,
                resource: material_buffer.as_entire_binding(),
            }];
            let mut stage_resources = Vec::with_capacity(MAX_TEXTURE_STAGES);
            for index in 0..MAX_TEXTURE_STAGES {
                // A stage without a decoded texture samples white through the
                // fallback with clamped nearest filtering.
                let (view, sampler_key) = match material.stages.get(index) {
                    Some(stage) if stage.texture_index.is_some() => (
                        &texture_views[stage.texture_index.expect("checked above")],
                        (stage.address_u, stage.address_v, stage.mag_filter),
                    ),
                    _ => (
                        &fallback,
                        (
                            AddressMode::ClampToEdge,
                            AddressMode::ClampToEdge,
                            FilterMode::Nearest,
                        ),
                    ),
                };
                let sampler = sampler_for(sampler_key);
                stage_resources.push((view, sampler));
            }
            for (index, (view, sampler)) in stage_resources.iter().enumerate() {
                entries.push(wgpu::BindGroupEntry {
                    binding: 1 + index as u32 * 2,
                    resource: wgpu::BindingResource::TextureView(view),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: 2 + index as u32 * 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                });
            }
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("HSD material"),
                layout: &material_layout,
                entries: &entries,
            });
            packets.push(GpuPacket {
                pipeline,
                bind_group,
                material_buffer,
            });
        }
        let depth_view = create_depth_view(device, width, height);
        finish_error_scope(scope)?;

        Ok(Self {
            geometry,
            lighting,
            width,
            height,
            orbit,
            camera,
            vertex_buffer,
            index_buffer,
            globals_buffer,
            globals_bind_group,
            pipeline_layout,
            pipelines,
            pick: None,
            highlighted: vec![false; packets.len()],
            packets,
            textures,
            depth_view,
        })
    }

    pub fn geometry(&self) -> &PreparedGeometry {
        &self.geometry
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn orbit(&self) -> Orbit {
        self.orbit
    }

    /// The color target's size in device pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Clear `target` and draw every visible packet in HSD pass order.
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("HSD pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(BACKGROUND_COLOR),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.globals_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        for &index in &self.geometry.draw_order {
            let packet = &self.geometry.packets[index];
            if !packet.visible {
                continue;
            }
            let gpu = &self.packets[index];
            pass.set_pipeline(&self.pipelines[gpu.pipeline]);
            pass.set_bind_group(1, &gpu.bind_group, &[]);
            pass.draw_indexed(
                packet.first_index..packet.first_index + packet.index_count,
                0,
                0..1,
            );
        }
    }

    /// Encode a pick of the device pixel `(x, y)` of the color target: which
    /// packet draws it, with the same pose, depth, culling, and cut-outs as
    /// the last [`encode`](Self::encode). Submit the encoder, then
    /// [`PickReadback::map`] the result.
    pub fn encode_pick(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (x, y): (u32, u32),
    ) -> Result<PickReadback> {
        if x >= self.width || y >= self.height {
            return Err(HsdRenderError::PickOutOfBounds {
                x,
                y,
                width: self.width,
                height: self.height,
            });
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pick = self.pick.get_or_insert_with(|| {
            let mut cache = HashMap::new();
            let mut pipelines = Vec::new();
            let packet_pipelines = self
                .geometry
                .packets
                .iter()
                .map(|packet| {
                    let material = &packet.material;
                    let key = PipelineKey::pick(packet.cull_mode, material, pick_shader(material));
                    *cache.entry(key.clone()).or_insert_with(|| {
                        pipelines.push(create_pipeline(
                            device,
                            &self.pipeline_layout,
                            PICK_FORMAT,
                            self.geometry.tex_coord_sets,
                            &key,
                        ));
                        pipelines.len() - 1
                    })
                })
                .collect();
            PickState {
                pipelines,
                packet_pipelines,
                target: None,
            }
        });
        if !matches!(&pick.target, Some((width, height, _)) if (*width, *height) == (self.width, self.height))
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("HSD pick ids"),
                size: wgpu::Extent3d {
                    width: self.width,
                    height: self.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: PICK_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            pick.target = Some((
                self.width,
                self.height,
                texture.create_view(&Default::default()),
            ));
        }
        let (_, _, view) = pick.target.as_ref().expect("sized above");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("HSD pick pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            // Only the picked pixel is shaded.
            pass.set_scissor_rect(x, y, 1, 1);
            pass.set_bind_group(0, &self.globals_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            for &index in &self.geometry.draw_order {
                let packet = &self.geometry.packets[index];
                if !packet.visible {
                    continue;
                }
                pass.set_pipeline(&pick.pipelines[pick.packet_pipelines[index]]);
                pass.set_bind_group(1, &self.packets[index].bind_group, &[]);
                pass.draw_indexed(
                    packet.first_index..packet.first_index + packet.index_count,
                    0,
                    0..1,
                );
            }
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("HSD pick readback"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let (_, _, view) = pick.target.as_ref().expect("sized above");
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: view.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        finish_error_scope(scope)?;
        Ok(PickReadback { buffer })
    }

    /// The textures a picked packet samples, most defining first (see
    /// [`PickedTexture`]).
    pub fn packet_textures(&self, packet: PacketIndex) -> Result<Vec<PickedTexture>> {
        let material = &self
            .geometry
            .packets
            .get(packet.0)
            .ok_or(HsdRenderError::UnknownPacket(packet))?
            .material;
        Ok(stages_by_prominence(material)
            .into_iter()
            .filter_map(|stage| {
                let prepared = &material.stages[stage];
                let texture = &self.geometry.textures[prepared.texture_index?];
                Some(PickedTexture {
                    stage,
                    scene_textures: texture.scene_textures.clone(),
                    reflection: prepared.source == crate::material::StageSource::Reflection,
                })
            })
            .collect())
    }

    /// Tint every packet that samples one of `scene_textures`, for an
    /// editor's selection; an empty slice clears it. Returns how many packets
    /// are tinted.
    pub fn set_highlight(
        &mut self,
        queue: &wgpu::Queue,
        scene_textures: &[HsdTextureIndex],
    ) -> usize {
        for (index, packet) in self.geometry.packets.iter().enumerate() {
            let highlighted = packet.material.stages.iter().any(|stage| {
                stage.texture_index.is_some_and(|texture| {
                    self.geometry.textures[texture]
                        .scene_textures
                        .iter()
                        .any(|scene| scene_textures.contains(scene))
                })
            });
            if highlighted != self.highlighted[index] {
                self.highlighted[index] = highlighted;
                write_joint_position(
                    queue,
                    &self.packets[index],
                    packet.joint_position,
                    highlighted,
                );
            }
        }
        self.highlighted.iter().filter(|&&on| on).count()
    }

    /// Replace the decoded pixels of scene texture `scene_texture`, for
    /// example after an editor patches its image data. The GPU texture is
    /// shared by every scene texture with the same content key, so they all
    /// change. `rgba` is `size` (width, height) RGBA8 pixels, and `size` must
    /// be the texture's: a transposed image has the right byte length but
    /// would draw scrambled. Returns `Ok(false)` when no drawn stage samples
    /// the scene texture.
    pub fn update_scene_texture(
        &mut self,
        queue: &wgpu::Queue,
        scene_texture: HsdTextureIndex,
        size: (u32, u32),
        rgba: &[u8],
    ) -> Result<bool> {
        let Some(index) = self.geometry.texture_for_scene_texture(scene_texture) else {
            return Ok(false);
        };
        let prepared = &mut self.geometry.textures[index];
        check_texture_update(
            scene_texture,
            (prepared.width, prepared.height),
            size,
            rgba.len(),
        )?;
        queue.write_texture(
            self.textures[index].as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(prepared.width * 4),
                rows_per_image: Some(prepared.height),
            },
            self.textures[index].size(),
        );
        // Keep the CPU copy current so a renderer rebuilt from this geometry
        // shows the edit too.
        prepared.rgba.copy_from_slice(rgba);
        Ok(true)
    }

    /// `orbit` clamped to what this scene allows: a framed focus lets the
    /// camera zoom out to, and pan across, the whole scene around it.
    pub fn clamp_orbit(&self, orbit: Orbit) -> Result<Orbit> {
        clamp_orbit(&self.geometry, orbit, self.width, self.height)
    }

    pub fn set_orbit(&mut self, queue: &wgpu::Queue, orbit: Orbit) -> Result<()> {
        let orbit = self.clamp_orbit(orbit)?;
        self.update_camera(queue, orbit, self.width, self.height)
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Result<()> {
        if (width, height) == (self.width, self.height) {
            return Ok(());
        }
        validate_dimensions(device, width, height)?;
        self.update_camera(queue, self.orbit, width, height)?;
        self.depth_view = create_depth_view(device, width, height);
        Ok(())
    }

    /// Upload one animation frame. Topology must match preparation.
    pub fn update_draw_work(
        &mut self,
        queue: &wgpu::Queue,
        scene: &HsdScene,
        work: &HsdEvaluatedDrawWork,
    ) -> Result<()> {
        self.geometry.update_vertices(scene, work)?;
        self.geometry.validate_reflections(&self.camera.view)?;
        queue.write_buffer(
            &self.vertex_buffer,
            0,
            bytemuck::cast_slice(&self.geometry.vertices),
        );
        for ((packet, gpu), &highlighted) in self
            .geometry
            .packets
            .iter()
            .zip(&self.packets)
            .zip(&self.highlighted)
        {
            write_joint_position(queue, gpu, packet.joint_position, highlighted);
        }
        Ok(())
    }

    fn update_camera(
        &mut self,
        queue: &wgpu::Queue,
        orbit: Orbit,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let camera = Camera::frame(
            &self.geometry.bounds,
            self.geometry.focus.as_ref(),
            width,
            height,
            orbit,
        )?;
        self.geometry.validate_reflections(&camera.view)?;
        queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::cast_slice(&global_uniforms(&camera, &self.lighting)),
        );
        self.camera = camera;
        self.orbit = orbit;
        self.width = width;
        self.height = height;
        Ok(())
    }
}

fn clamp_orbit(
    geometry: &PreparedGeometry,
    orbit: Orbit,
    width: u32,
    height: u32,
) -> Result<Orbit> {
    let reach = geometry
        .focus
        .map_or(1.0, |focus| focus.reach(&geometry.bounds, width, height));
    orbit.clamped_within(reach)
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PipelineKey {
    cull_mode: CullMode,
    blend: HsdBlendMode,
    color_update: bool,
    alpha_update: bool,
    depth_write: bool,
    depth_compare: HsdCompare,
    shader: String,
}

impl PipelineKey {
    /// A packet's pick pipeline: its depth, culling, and cut-outs, with an
    /// unblended id write. A packet that updates no color writes no id, so it
    /// can hide surfaces but never be the pick.
    fn pick(
        cull_mode: CullMode,
        material: &crate::material::PreparedMaterial,
        shader: String,
    ) -> Self {
        Self {
            blend: HsdBlendMode::None,
            color_update: material.color_update,
            alpha_update: material.color_update,
            ..Self::new(cull_mode, material, shader)
        }
    }

    fn new(
        cull_mode: CullMode,
        material: &crate::material::PreparedMaterial,
        shader: String,
    ) -> Self {
        Self {
            cull_mode,
            blend: material.blend,
            color_update: material.color_update,
            alpha_update: material.alpha_update,
            depth_write: material.depth_write,
            depth_compare: material.depth_compare,
            shader,
        }
    }
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    tex_coord_sets: usize,
    key: &PipelineKey,
) -> wgpu::RenderPipeline {
    const FLOAT_BYTES: u64 = size_of::<f32>() as u64;
    // Position, normal, TEX0, TEX1, COLOR0, then the scene's further sets.
    let mut attributes = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x4,
    ]
    .to_vec();
    for set in BASE_TEX_COORD_SETS..tex_coord_sets {
        attributes.push(wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: tex_coord_offset(set) as u64 * FLOAT_BYTES,
            shader_location: tex_coord_location(set),
        });
    }
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("HSD material TEV"),
        source: wgpu::ShaderSource::Wgsl(key.shader.as_str().into()),
    });
    let blend = blend_state(key.blend);
    let mut write_mask = wgpu::ColorWrites::empty();
    if key.color_update {
        write_mask |= wgpu::ColorWrites::COLOR;
    }
    if key.alpha_update {
        write_mask |= wgpu::ColorWrites::ALPHA;
    }
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("HSD material pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vertexMain"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: floats_per_vertex(tex_coord_sets) as u64 * FLOAT_BYTES,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fragmentMain"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: match key.cull_mode {
                CullMode::None => None,
                CullMode::Front => Some(wgpu::Face::Front),
                CullMode::Back => Some(wgpu::Face::Back),
            },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(key.depth_write),
            depth_compare: Some(compare_function(key.depth_compare)),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

/// GX blends color and alpha with the same equation.
fn blend_state(mode: HsdBlendMode) -> Option<wgpu::BlendState> {
    let component = match mode {
        // Material preparation lowers COPY to `None` and refuses the other
        // logic ops, so a logic blend here writes the source.
        HsdBlendMode::None | HsdBlendMode::Logic(_) => return None,
        HsdBlendMode::Blend {
            source,
            destination,
        } => wgpu::BlendComponent {
            src_factor: blend_factor(source),
            dst_factor: blend_factor(destination),
            operation: wgpu::BlendOperation::Add,
        },
        HsdBlendMode::Subtract => wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::ReverseSubtract,
        },
    };
    Some(wgpu::BlendState {
        color: component,
        alpha: component,
    })
}

fn blend_factor(factor: HsdBlendFactor) -> wgpu::BlendFactor {
    match factor {
        HsdBlendFactor::Zero => wgpu::BlendFactor::Zero,
        HsdBlendFactor::One => wgpu::BlendFactor::One,
        HsdBlendFactor::SourceColor => wgpu::BlendFactor::Src,
        HsdBlendFactor::InverseSourceColor => wgpu::BlendFactor::OneMinusSrc,
        HsdBlendFactor::DestinationColor => wgpu::BlendFactor::Dst,
        HsdBlendFactor::InverseDestinationColor => wgpu::BlendFactor::OneMinusDst,
        HsdBlendFactor::SourceAlpha => wgpu::BlendFactor::SrcAlpha,
        HsdBlendFactor::InverseSourceAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        HsdBlendFactor::DestinationAlpha => wgpu::BlendFactor::DstAlpha,
        HsdBlendFactor::InverseDestinationAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
    }
}

fn compare_function(compare: HsdCompare) -> wgpu::CompareFunction {
    match compare {
        HsdCompare::Never => wgpu::CompareFunction::Never,
        HsdCompare::Less => wgpu::CompareFunction::Less,
        HsdCompare::Equal => wgpu::CompareFunction::Equal,
        HsdCompare::LessEqual => wgpu::CompareFunction::LessEqual,
        HsdCompare::Greater => wgpu::CompareFunction::Greater,
        HsdCompare::NotEqual => wgpu::CompareFunction::NotEqual,
        HsdCompare::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
        HsdCompare::Always => wgpu::CompareFunction::Always,
    }
}

/// A packet's joint position and selection tint (`jointPosition` in WGSL).
fn write_joint_position(
    queue: &wgpu::Queue,
    gpu: &GpuPacket,
    [x, y, z]: [f32; 3],
    highlighted: bool,
) {
    queue.write_buffer(
        &gpu.material_buffer,
        MATERIAL_JOINT_POSITION_OFFSET_BYTES,
        bytemuck::cast_slice(&[x, y, z, if highlighted { 1.0 } else { 0.0 }]),
    );
}

fn global_uniforms(camera: &Camera, lighting: &HsdLightingPreset) -> [f32; GLOBAL_UNIFORM_FLOATS] {
    let mut values = [0.0; GLOBAL_UNIFORM_FLOATS];
    values[0..16].copy_from_slice(&camera.view_projection);
    values[16..32].copy_from_slice(&camera.view);
    for axis in 0..3 {
        values[GLOBAL_CAMERA_POSITION_FLOAT + axis] = camera.position[axis] as f32;
    }
    values[GLOBAL_LIGHTING_FLOAT..].copy_from_slice(&lighting.uniforms(&camera.view));
    values
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    (width, height): (u32, u32),
    rgba: &[u8],
) -> wgpu::Texture {
    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("HSD depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

/// `size` and `bytes` describe the same RGBA8 image, and it is `expected`'s
/// shape.
fn check_texture_update(
    scene_texture: HsdTextureIndex,
    expected: (u32, u32),
    (width, height): (u32, u32),
    bytes: usize,
) -> Result<()> {
    let expected_bytes = width as usize * height as usize * 4;
    if (width, height) != expected || bytes != expected_bytes {
        return Err(HsdRenderError::TextureSizeMismatch {
            scene_texture,
            width: expected.0,
            height: expected.1,
            actual_width: width,
            actual_height: height,
            bytes,
        });
    }
    Ok(())
}

fn validate_dimensions(device: &wgpu::Device, width: u32, height: u32) -> Result<()> {
    let maximum = device.limits().max_texture_dimension_2d;
    if width == 0 || height == 0 || width > maximum || height > maximum {
        return Err(HsdRenderError::RenderSize {
            width,
            height,
            maximum,
        });
    }
    Ok(())
}

fn address_mode(mode: AddressMode) -> wgpu::AddressMode {
    match mode {
        AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
        AddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

fn filter_mode(mode: FilterMode) -> wgpu::FilterMode {
    match mode {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

/// Pop a validation scope. Natively the pop resolves at once and errors are
/// returned. A browser cannot block on a GPU future, so there the pop only
/// ends the scope and errors reach the device's uncaptured-error handler.
pub(crate) fn finish_error_scope(scope: wgpu::ErrorScopeGuard) -> Result<()> {
    #[cfg(not(target_family = "wasm"))]
    if let Some(error) = pollster::block_on(scope.pop()) {
        return Err(crate::error::GpuError::from(error).into());
    }
    #[cfg(target_family = "wasm")]
    drop(scope.pop());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PipelineKey, check_texture_update};
    use crate::error::HsdRenderError;
    use crate::geometry::CullMode;
    use crate::material::test_support::{DIFFUSE, material, stage};
    use crate::material::{HsdAlphaMap, HsdColorMap, StageSource};
    use dat_parser::hsd::pe::HsdBlendMode;
    use dat_parser::hsd::scene::HsdTextureIndex;

    #[test]
    fn a_texture_update_must_have_the_textures_shape() {
        let texture = HsdTextureIndex(3);
        assert!(check_texture_update(texture, (32, 64), (32, 64), 32 * 64 * 4).is_ok());
        // Same byte length, transposed.
        let error = check_texture_update(texture, (32, 64), (64, 32), 32 * 64 * 4).unwrap_err();
        assert!(matches!(
            error,
            HsdRenderError::TextureSizeMismatch {
                scene_texture: HsdTextureIndex(3),
                width: 32,
                height: 64,
                actual_width: 64,
                actual_height: 32,
                ..
            }
        ));
        assert!(
            error.to_string().contains("64x32") && error.to_string().contains("32x64"),
            "{error}"
        );
        // The right size claimed for the wrong number of bytes.
        assert!(check_texture_update(texture, (32, 64), (32, 64), 32 * 64 * 3).is_err());
    }

    #[test]
    fn a_packet_that_writes_no_color_picks_as_a_depth_only_occluder() {
        let texcoord = stage(
            StageSource::TexCoord(0),
            HsdColorMap::Modulate,
            HsdAlphaMap::None,
        );
        let mut hidden = material(vec![texcoord], &[DIFFUSE]);
        hidden.color_update = false;
        hidden.alpha_update = true;
        hidden.blend = HsdBlendMode::SOURCE_ALPHA;
        hidden.depth_write = true;
        let key = PipelineKey::pick(CullMode::Back, &hidden, String::new());
        assert_eq!(key.blend, HsdBlendMode::None);
        assert!(!key.color_update && !key.alpha_update, "writes no id");
        assert!(key.depth_write, "still occludes");

        hidden.color_update = true;
        hidden.alpha_update = false;
        let key = PipelineKey::pick(CullMode::Back, &hidden, String::new());
        assert!(key.color_update && key.alpha_update, "writes its id");
    }
}
