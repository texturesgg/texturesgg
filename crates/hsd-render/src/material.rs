//! Material preparation for HSD draw packets: TObj texture stages, their TEV
//! plan in MObjMakeTExp order, color channels, lighting colors, and PE state.

use crate::error::{Result, invalid_scene};
use crate::geometry::MAX_TEX_COORD_SETS;
use dat_parser::hsd::channel::HsdChannelBase;
use dat_parser::hsd::pe::{HsdAlphaCompare, HsdBlendMode, HsdCompare, HsdLogicOp};
use dat_parser::hsd::scene::{
    HsdCustomTev, HsdDisplayObject, HsdScene, HsdTextureContentKey, HsdTextureObject,
};
use dat_parser::hsd::tev::{HsdTObjTevAlphaInput, HsdTObjTevColorInput};
pub use dat_parser::hsd::texture::{HsdAlphaMap, HsdColorMap, HsdLightMap};
use dat_parser::hsd::texture::{HsdTextureCoordinates, HsdTextureSource};
use std::collections::HashMap;

/// GX's limit. Stock costumes use at most two; Kongo Jungle N64 uses five.
pub const MAX_TEXTURE_STAGES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterMode {
    Nearest,
    Linear,
}

/// One decoded image, uploaded once and shared by every scene texture with its
/// content key and every stage sampling those; sampler state lives on the stage.
#[derive(Clone, Debug)]
pub struct PreparedTexture {
    pub content: HsdTextureContentKey,
    /// Every `HsdScene::textures` index that decodes to these pixels.
    pub scene_textures: Vec<u32>,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Prepared textures by content, and the prepared texture of each scene
/// texture seen so far.
#[derive(Default)]
pub(crate) struct TextureCache {
    by_content: HashMap<HsdTextureContentKey, usize>,
    by_scene_texture: HashMap<u32, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageSource {
    /// One of the vertex's texture coordinate sets, TEX0 to TEX7.
    TexCoord(u8),
    /// Normalized camera-space normals.
    Reflection,
}

#[derive(Clone, Debug)]
pub struct PreparedStage {
    /// Index into the prepared textures, or `None` when the image did not decode.
    pub texture_index: Option<usize>,
    /// The TObj's sampler state (wrap S/T and magnification filter).
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub mag_filter: FilterMode,
    pub source: StageSource,
    /// Row-major 3x4 texture postmatrix.
    pub matrix: [[f32; 4]; 3],
    pub color_op: HsdColorMap,
    pub alpha_op: HsdAlphaMap,
    /// TObj blending factor for the BLEND color and alpha maps.
    pub blending: f32,
    /// Replaces the stage texel as the color/alpha input (MakeColorGenTExp).
    pub custom_tev: Option<PreparedTev>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TevTarget {
    Color,
    Specular,
}

/// One backend-neutral TEV step; see [`tev_plan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TevStep {
    Stage {
        stage: usize,
        target: TevTarget,
        alpha: bool,
    },
    DiffuseLighting,
    SpecularLighting,
}

#[derive(Clone, Debug)]
pub struct PreparedMaterial {
    /// TEV base: material diffuse/alpha, or white for vertex and white bases.
    pub base_color: [f32; 4],
    /// Multiply the base by rasterized vertex COLOR0.
    pub vertex_color: bool,
    /// Applied TObj stages in source order.
    pub stages: Vec<PreparedStage>,
    /// Source-ordered TEV steps over the stages and lighting.
    pub tev_plan: Vec<TevStep>,
    pub required_tex_coord_mask: u8,
    /// Every applied reflection stage keeps its projective validation boundary.
    pub reflection_matrices: Vec<[[f32; 4]; 3]>,
    /// Material ambient, which scales the preset ambient light in lit COLOR0.
    pub ambient: [f32; 3],
    pub specular: [f32; 3],
    pub shininess: f32,
    /// Lowered canonical PE state, never inferred from material colors or flags.
    /// A destination-alpha override is not lowered, and a logic blend never
    /// reaches here: COPY lowers to `None` and the other ops are refused.
    pub blend: HsdBlendMode,
    pub color_update: bool,
    pub alpha_update: bool,
    /// `None` when every alpha passes. GX ZCompLoc is not selectable on wgpu;
    /// fragment discard does not claim exact before/after-texturing
    /// depth-update parity.
    pub alpha_compare: Option<HsdAlphaCompare>,
    pub depth_write: bool,
    /// ALWAYS when the depth test is off.
    pub depth_compare: HsdCompare,
}

pub(crate) fn prepare_material(
    display_object: &HsdDisplayObject,
    scene: &HsdScene,
    textures: &mut Vec<PreparedTexture>,
    texture_cache: &mut TextureCache,
) -> Result<PreparedMaterial> {
    let channels = display_object.channels();

    // Stages in TObj order. MObjMakeTExp only applies TObjs with a light-map
    // role; BUMP TObjs feed GX emboss texgen instead, which is not implemented.
    let mut stages = Vec::new();
    let mut stage_lightmaps = Vec::new();
    for usage in display_object
        .material
        .iter()
        .flat_map(|material| &material.textures)
    {
        let lightmap = usage.light_map();
        if !lightmap.is_stage() || usage.is_bump() {
            continue;
        }
        if stages.len() == MAX_TEXTURE_STAGES {
            return invalid_scene(format!(
                "materials support at most {MAX_TEXTURE_STAGES} texture stages"
            ));
        }
        let (source, matrix) = texture_coordinates(&usage.coordinates())?;
        if !usage.blending.is_finite() {
            return invalid_scene("texture stage blending is invalid");
        }
        stages.push(PreparedStage {
            texture_index: prepare_texture(scene, usage, textures, texture_cache)?,
            address_u: address_mode(usage.wrap_s)?,
            address_v: address_mode(usage.wrap_t)?,
            mag_filter: if usage.mag_filter == 0 {
                FilterMode::Nearest
            } else {
                FilterMode::Linear
            },
            source,
            matrix,
            color_op: usage
                .color_map()
                .or_else(|error| invalid_scene(error.to_string()))?,
            alpha_op: usage
                .alpha_map()
                .or_else(|error| invalid_scene(error.to_string()))?,
            blending: usage.blending,
            custom_tev: usage.custom_tev.map(PreparedTev::new).transpose()?,
        });
        stage_lightmaps.push(lightmap);
    }
    let mut required_tex_coord_mask = 0;
    let mut reflection_matrices = Vec::new();
    for stage in &stages {
        match stage.source {
            StageSource::Reflection => reflection_matrices.push(stage.matrix),
            StageSource::TexCoord(index) => required_tex_coord_mask |= 1 << index,
        }
    }

    let colors = display_object
        .material
        .as_ref()
        .and_then(|material| material.colors.as_ref());
    let unit = |color: [u8; 4]| [0, 1, 2].map(|axis| (f64::from(color[axis]) / 255.0) as f32);
    let diffuse = colors.map_or([0.8; 3], |colors| unit(colors.diffuse));
    let alpha = colors.map_or(1.0, |colors| colors.alpha);
    if !(0.0..=1.0).contains(&alpha) {
        return invalid_scene("material alpha is invalid");
    }
    let pixel_engine = display_object.pixel_engine();
    if pixel_engine.dither || pixel_engine.destination_alpha.is_some() {
        return invalid_scene("the wgpu backend does not support PE dither or destination alpha");
    }
    let blend = match pixel_engine.blend {
        // COPY writes the source: no blending.
        HsdBlendMode::Logic(HsdLogicOp::Copy) => HsdBlendMode::None,
        HsdBlendMode::Logic(op) => {
            return invalid_scene(format!(
                "the wgpu backend has no framebuffer logic ops ({op:?})"
            ));
        }
        blend => blend,
    };
    let ambient = colors.map_or([0.0; 3], |colors| unit(colors.ambient));
    let specular = colors.map_or([0.0; 3], |colors| unit(colors.specular));
    let shininess = colors.map_or(0.0, |colors| colors.shininess);
    if !shininess.is_finite() {
        return invalid_scene("material lighting colors are invalid");
    }
    // MObjMakeTExp starts from material constants, rasterized vertex color, or
    // white, then texture stages combine onto it.
    let material_base = channels.base == HsdChannelBase::Material;
    let base_rgb = if material_base { diffuse } else { [1.0; 3] };
    let base_alpha = if material_base { alpha } else { 1.0 };
    let depth_test = pixel_engine.depth_test;
    Ok(PreparedMaterial {
        base_color: [base_rgb[0], base_rgb[1], base_rgb[2], base_alpha],
        vertex_color: channels.base == HsdChannelBase::Vertex,
        tev_plan: tev_plan(
            &stage_lightmaps,
            channels.diffuse_lighting,
            channels.specular_lighting,
        ),
        stages,
        required_tex_coord_mask,
        reflection_matrices,
        ambient,
        specular,
        shininess,
        blend,
        color_update: pixel_engine.color_update,
        alpha_update: pixel_engine.alpha_update,
        alpha_compare: (!pixel_engine.alpha_compare.always_passes())
            .then_some(pixel_engine.alpha_compare),
        depth_write: depth_test && pixel_engine.depth_write,
        depth_compare: if depth_test {
            pixel_engine.depth_compare
        } else {
            HsdCompare::Always
        },
    })
}

/// MObjMakeTExp's order (mobj.c:190): diffuse/ambient light-map stages, the
/// lit COLOR0 multiply, then (RENDER_SPECULAR only) specular light-map stages
/// on the specular chain before COLOR1 and the add, then EXT stages. A TObj in
/// several phases applies its alpha op only in the first one it reaches
/// (TObjMakeTExp's `repeat`).
pub fn tev_plan(
    stage_lightmaps: &[HsdLightMap],
    diffuse_lighting: bool,
    specular_lighting: bool,
) -> Vec<TevStep> {
    let mut steps = Vec::new();
    // Stages that already applied their alpha op in an earlier phase.
    let mut reached = vec![false; stage_lightmaps.len()];
    let mut phase =
        |steps: &mut Vec<TevStep>, in_phase: fn(&HsdLightMap) -> bool, target: TevTarget| {
            for (stage, lightmap) in stage_lightmaps.iter().enumerate() {
                if in_phase(lightmap) {
                    steps.push(TevStep::Stage {
                        stage,
                        target,
                        alpha: !reached[stage],
                    });
                    reached[stage] = true;
                }
            }
        };
    phase(
        &mut steps,
        |lightmap| lightmap.diffuse || lightmap.ambient,
        TevTarget::Color,
    );
    if diffuse_lighting {
        steps.push(TevStep::DiffuseLighting);
    }
    if specular_lighting {
        phase(
            &mut steps,
            |lightmap| lightmap.specular,
            TevTarget::Specular,
        );
        steps.push(TevStep::SpecularLighting);
    }
    phase(&mut steps, |lightmap| lightmap.ext, TevTarget::Color);
    steps
}

/// The prepared texture for a stage's scene texture, uploading each distinct
/// decoded image once however many descriptors and TObjs reach it. `None` for
/// a stage whose image did not decode; the renderer binds white.
fn prepare_texture(
    scene: &HsdScene,
    usage: &HsdTextureObject,
    textures: &mut Vec<PreparedTexture>,
    cache: &mut TextureCache,
) -> Result<Option<usize>> {
    let Some(scene_texture) = usage.texture else {
        return Ok(None);
    };
    let Ok(scene_texture_index) = u32::try_from(scene_texture.0) else {
        return invalid_scene("a scene texture index exceeds u32");
    };
    if let Some(&cached) = cache.by_scene_texture.get(&scene_texture_index) {
        return Ok(Some(cached));
    }
    let Some(source) = scene.textures.get(scene_texture.0) else {
        return Ok(None);
    };
    let Ok(rgba) = source.rgba.as_ref() else {
        return Ok(None);
    };
    let content = source.content_key();
    let index = match cache.by_content.get(&content) {
        Some(&index) => {
            textures[index].scene_textures.push(scene_texture_index);
            index
        }
        None => {
            let (width, height) = (u32::from(content.width), u32::from(content.height));
            if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
                return invalid_scene("decoded texture dimensions are invalid");
            }
            textures.push(PreparedTexture {
                content,
                scene_textures: vec![scene_texture_index],
                width,
                height,
                rgba: rgba.to_vec(),
            });
            cache.by_content.insert(content, textures.len() - 1);
            textures.len() - 1
        }
    };
    cache.by_scene_texture.insert(scene_texture_index, index);
    Ok(Some(index))
}

fn texture_coordinates(
    coordinates: &HsdTextureCoordinates,
) -> Result<(StageSource, [[f32; 4]; 3])> {
    let (source, matrix) = match *coordinates {
        HsdTextureCoordinates::Reflection { matrix } => (StageSource::Reflection, matrix),
        HsdTextureCoordinates::Matrix {
            source: HsdTextureSource::TexCoord { index },
            matrix,
        } if usize::from(index) < MAX_TEX_COORD_SETS => (StageSource::TexCoord(index), matrix),
        HsdTextureCoordinates::Matrix { source, .. } => {
            return invalid_scene(format!(
                "the wgpu backend requires texture coordinates from a TEX attribute, not {source:?}"
            ));
        }
        HsdTextureCoordinates::Unsupported { reason } => {
            return invalid_scene(format!(
                "the wgpu backend does not support texture coordinates: {reason:?}"
            ));
        }
    };
    if matrix
        .iter()
        .flatten()
        .any(|component| !component.is_finite())
    {
        return invalid_scene("canonical texture matrix is invalid");
    }
    Ok((source, matrix))
}

/// A TObj's own TEV: `(A * (1 - C) + B * C) + D`, clamped, per active side.
/// The parser admits only that form (add, zero bias, scale one, clamp).
#[derive(Clone, Copy, Debug)]
pub struct PreparedTev {
    pub color: Option<[HsdTObjTevColorInput; 4]>,
    pub alpha: Option<[HsdTObjTevAlphaInput; 4]>,
    pub konst: [u8; 4],
    pub tev0: [u8; 4],
}

impl PreparedTev {
    fn new(custom: HsdCustomTev) -> Result<Self> {
        let prepared = Self {
            color: custom.program.color_inputs(),
            alpha: custom.program.alpha_inputs(),
            konst: custom.registers.konst,
            tev0: custom.registers.tev0,
        };
        if prepared.color.is_none() && prepared.alpha.is_none() {
            return invalid_scene("custom TEV has no active side");
        }
        Ok(prepared)
    }
}

/// `GXTexWrapMode`: clamp, repeat, mirror.
fn address_mode(mode: u32) -> Result<AddressMode> {
    Ok(match mode {
        0 => AddressMode::ClampToEdge,
        1 => AddressMode::Repeat,
        2 => AddressMode::MirrorRepeat,
        _ => return invalid_scene(format!("texture wrap mode {mode} is invalid")),
    })
}

/// Hand-built stages and materials for shader and pick tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub const DIFFUSE: HsdLightMap = HsdLightMap::DIFFUSE;
    pub const SPECULAR: HsdLightMap = HsdLightMap::SPECULAR;
    pub const EXT: HsdLightMap = HsdLightMap::EXT;

    pub fn stage(
        source: StageSource,
        color_op: HsdColorMap,
        alpha_op: HsdAlphaMap,
    ) -> PreparedStage {
        PreparedStage {
            texture_index: Some(0),
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
            mag_filter: FilterMode::Linear,
            source,
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
            ],
            color_op,
            alpha_op,
            blending: 0.5,
            custom_tev: None,
        }
    }

    pub fn material(stages: Vec<PreparedStage>, lightmaps: &[HsdLightMap]) -> PreparedMaterial {
        PreparedMaterial {
            base_color: [0.8, 0.8, 0.8, 1.0],
            vertex_color: true,
            tev_plan: tev_plan(lightmaps, true, true),
            stages,
            required_tex_coord_mask: 0b11,
            reflection_matrices: Vec::new(),
            ambient: [0.5; 3],
            specular: [1.0; 3],
            shininess: 50.0,
            blend: HsdBlendMode::None,
            color_update: true,
            alpha_update: true,
            alpha_compare: Some(HsdAlphaCompare::GREATER_ZERO),
            depth_write: true,
            depth_compare: HsdCompare::LessEqual,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_orders_lighting_between_light_map_phases() {
        let plan = tev_plan(
            &[
                HsdLightMap::DIFFUSE,
                HsdLightMap::EXT | HsdLightMap::SPECULAR,
            ],
            true,
            true,
        );
        assert_eq!(
            plan,
            vec![
                TevStep::Stage {
                    stage: 0,
                    target: TevTarget::Color,
                    alpha: true,
                },
                TevStep::DiffuseLighting,
                TevStep::Stage {
                    stage: 1,
                    target: TevTarget::Specular,
                    alpha: true,
                },
                TevStep::SpecularLighting,
                // Stage 1 already applied its alpha op in the specular phase.
                TevStep::Stage {
                    stage: 1,
                    target: TevTarget::Color,
                    alpha: false,
                },
            ]
        );
    }

    /// A logic blend is the backend's to lower: COPY is a plain write, and
    /// wgpu has no other framebuffer logic op.
    #[test]
    fn a_copy_logic_blend_is_a_plain_write_and_other_logic_ops_are_refused() {
        use dat_parser::hsd::pe::HsdPixelEngineState;
        use dat_parser::hsd::scene::{DObjId, HsdCustomPe, HsdMaterial, MObjId, PeDescId};

        let prepare = |op| {
            let display_object = HsdDisplayObject {
                source_id: DObjId(4),
                material: Some(HsdMaterial {
                    source_id: MObjId(16),
                    render_flags: 0,
                    custom_pe: Some(HsdCustomPe {
                        source_id: PeDescId(64),
                        state: HsdPixelEngineState {
                            blend: HsdBlendMode::Logic(op),
                            ..HsdPixelEngineState::from_render_flags(0)
                        },
                    }),
                    colors: None,
                    textures: Vec::new(),
                }),
                polygons: Vec::new(),
            };
            let scene = HsdScene {
                roots: Vec::new(),
                textures: Vec::new(),
            };
            prepare_material(
                &display_object,
                &scene,
                &mut Vec::new(),
                &mut TextureCache::default(),
            )
        };
        assert_eq!(prepare(HsdLogicOp::Copy).unwrap().blend, HsdBlendMode::None);
        assert!(prepare(HsdLogicOp::Xor).is_err());
    }

    #[test]
    fn specular_stages_are_skipped_without_specular_lighting() {
        let plan = tev_plan(&[HsdLightMap::SPECULAR], true, false);
        assert_eq!(plan, vec![TevStep::DiffuseLighting]);
    }
}
