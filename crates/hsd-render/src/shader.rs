//! WGSL generation for a prepared material: vertex texgen and GX lighting,
//! then the fragment lowering of the material's TEV plan.
//!
//! The generated source depends only on the material's program shape (plan,
//! stage sources, ops, custom TEV, vertex color, alpha test), so it doubles as
//! the pipeline cache key. Per-packet values live in uniforms.
//!
//! The same program also lowers to a pick variant (see `crate::pick`): it
//! runs the TEV chain for alpha, discards like the color pass, and writes the
//! packet's pick id instead of a color.

use crate::geometry::{BASE_TEX_COORD_SETS, MAX_TEX_COORD_SETS, tex_coord_location};
use crate::lighting::HSD_MAX_LIGHTS;
use crate::material::{
    HsdAlphaMap, HsdColorMap, MAX_TEXTURE_STAGES, PreparedMaterial, PreparedStage, StageSource,
    TevStep, TevTarget,
};
use dat_parser::hsd::pe::{HsdAlphaCompare, HsdAlphaOp, HsdBlendMode, HsdCompare};
use dat_parser::hsd::tev::{HsdTObjTevAlphaInput, HsdTObjTevColorInput};
use std::fmt::Write;

/// baseColor, ambient, specular, jointPosition, then 4 vec4s per texture stage.
pub const MATERIAL_UNIFORM_FLOATS: usize = 16 + MAX_TEXTURE_STAGES * 16;
pub const MATERIAL_JOINT_POSITION_OFFSET_BYTES: u64 = 12 * 4;
pub const GLOBAL_CAMERA_POSITION_FLOAT: usize = 32;
pub const GLOBAL_LIGHTING_FLOAT: usize = 36;
pub const GLOBAL_UNIFORM_FLOATS: usize =
    GLOBAL_LIGHTING_FLOAT + crate::lighting::HSD_LIGHTING_UNIFORM_FLOATS;

/// An editor's selection tint (raw color) and how far it mixes in.
const HIGHLIGHT_COLOR: &str = "vec3f(1.0, 0.78, 0.2)";
const HIGHLIGHT_STRENGTH: f32 = 0.35;

/// Translucent fragments below this alpha don't take a click, so a faint
/// overlay can't hide the surface under it.
pub const PICK_TRANSLUCENT_ALPHA: f32 = 0.25;

/// What a material program writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShaderOutput {
    /// Raw GX color, for display.
    Color,
    /// The packet's pick id (`u32`), for [`crate::pick`].
    Pick,
}

pub fn material_shader(material: &PreparedMaterial) -> String {
    shader_source(material, ShaderOutput::Color)
}

pub fn pick_shader(material: &PreparedMaterial) -> String {
    shader_source(material, ShaderOutput::Pick)
}

fn shader_source(material: &PreparedMaterial, output: ShaderOutput) -> String {
    let stages = &material.stages;
    let mut stage_bindings = String::new();
    let mut stage_outputs = String::new();
    let mut stage_coordinates = String::new();
    for (index, stage) in stages.iter().enumerate() {
        let _ = writeln!(
            stage_bindings,
            "@group(1) @binding({}) var stageTexture{index}: texture_2d<f32>;\n\
             @group(1) @binding({}) var stageSampler{index}: sampler;",
            1 + index * 2,
            2 + index * 2
        );
        let _ = writeln!(
            stage_outputs,
            "  @location({}) stq{index}: vec3f,",
            3 + index
        );
        let source = match stage.source {
            StageSource::Reflection => {
                "vec4f(normalize((globals.viewMatrix * vec4f(input.normal, 0.0)).xyz), 1.0)"
                    .to_owned()
            }
            StageSource::TexCoord(coordinate) => format!("vec4f(input.uv{coordinate}, 1.0, 1.0)"),
        };
        let _ = writeln!(
            stage_coordinates,
            "  output.stq{index} = stageStq({index}, {source});"
        );
    }
    // TEX0 and TEX1 are in every vertex; the rest only where a scene reads
    // them, declared only by the materials that do.
    let mut extra_tex_coords = String::new();
    for set in BASE_TEX_COORD_SETS..MAX_TEX_COORD_SETS {
        if material.required_tex_coord_mask & (1 << set) != 0 {
            let _ = writeln!(
                extra_tex_coords,
                "  @location({}) uv{set}: vec2f,",
                tex_coord_location(set)
            );
        }
    }
    let fragment = tev_fragment_body(material);
    let discard = material
        .alpha_compare
        .as_ref()
        .map_or_else(String::new, alpha_discard);
    let (fragment_output, epilogue) = match output {
        ShaderOutput::Color => (
            "vec4f",
            format!(
                "{discard}\n  let highlight = material.jointPosition.w * {HIGHLIGHT_STRENGTH:?};\n  \
                 return vec4f(mix(color, {HIGHLIGHT_COLOR}, highlight), alpha);"
            ),
        ),
        ShaderOutput::Pick => {
            // A packet that updates no color is never the pick (its id write
            // is masked); it only occludes, so it keeps the color pass's depth.
            let translucent = if material.blend != HsdBlendMode::None && material.color_update {
                format!("if (alpha < {PICK_TRANSLUCENT_ALPHA:?}) {{ discard; }}")
            } else {
                String::new()
            };
            (
                "u32",
                format!("{discard}\n  {translucent}\n  return u32(material.ambient.w);"),
            )
        }
    };
    format!(
        r#"
struct Light {{
  // rgb color; w = 1 when the light contributes to diffuse COLOR0.
  color: vec4f,
  // World-space unit vector toward the light; w = 1 for specular COLOR1.
  toward: vec4f,
}}
struct Globals {{
  viewProjection: mat4x4f,
  viewMatrix: mat4x4f,
  cameraPosition: vec4f,
  ambientLight: vec4f,
  lights: array<Light, {HSD_MAX_LIGHTS}>,
}}
struct Stage {{
  // Row-major 3x4 texture postmatrix.
  row0: vec4f,
  row1: vec4f,
  row2: vec4f,
  // x = TObj blending factor.
  params: vec4f,
}}
struct Material {{
  baseColor: vec4f,
  // rgb ambient; w = pick id (packet index + 1, 0 is no packet).
  ambient: vec4f,
  // rgb specular color; w = shininess.
  specular: vec4f,
  // xyz = joint world position; w = 1 to tint for an editor selection.
  jointPosition: vec4f,
  stages: array<Stage, {MAX_TEXTURE_STAGES}>,
}}
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> material: Material;
{stage_bindings}
struct VertexInput {{
  @location(0) position: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv0: vec2f,
  @location(3) uv1: vec2f,
  @location(4) color: vec4f,
{extra_tex_coords}}}
struct VertexOutput {{
  @builtin(position) position: vec4f,
  @location(0) color: vec4f,
  @location(1) lit0: vec3f,
  @location(2) lit1: vec3f,
{stage_outputs}}}
fn stageStq(index: u32, source: vec4f) -> vec3f {{
  let stage = material.stages[index];
  return vec3f(dot(stage.row0, source), dot(stage.row1, source), dot(stage.row2, source));
}}
// GX per-vertex lighting for HSD channels (state.c HSD_SetupChannelMode):
// COLOR0 = clamp(material ambient * ambient light + sum(light * max(N.L, 0)));
// COLOR1 = clamp(sum(light * max(N.L, 0) * x^2 / (k0 + (1 - k0) x^2))) with
// x = max(N.H, 0) and k0 = shininess / 2 (lobj.c HSD_LObjSetup), where H is
// the half-vector from the joint toward the light and camera (lobj.c:301).
fn gxLighting(input: VertexInput, output: ptr<function, VertexOutput>) {{
  var lit0 = material.ambient.rgb * globals.ambientLight.rgb;
  var lit1 = vec3f(0.0);
  let toCamera = globals.cameraPosition.xyz - material.jointPosition.xyz;
  let cameraDirection = select(vec3f(0.0), normalize(toCamera), dot(toCamera, toCamera) > 0.0);
  let k0 = 0.5 * material.specular.w;
  for (var index = 0u; index < {HSD_MAX_LIGHTS}u; index++) {{
    let light = globals.lights[index];
    let nDotL = dot(input.normal, light.toward.xyz);
    let diffuse = max(nDotL, 0.0);
    lit0 += light.color.rgb * (diffuse * light.color.w);
    let halfSum = light.toward.xyz + cameraDirection;
    if (light.toward.w > 0.5 && nDotL >= 0.0 && dot(halfSum, halfSum) > 0.0) {{
      let x = max(dot(input.normal, normalize(halfSum)), 0.0);
      let denominator = k0 + (1.0 - k0) * x * x;
      let attenuation = select(0.0, x * x / denominator, denominator > 0.0);
      lit1 += light.color.rgb * (diffuse * attenuation);
    }}
  }}
  (*output).lit0 = clamp(lit0, vec3f(0.0), vec3f(1.0));
  (*output).lit1 = clamp(lit1, vec3f(0.0), vec3f(1.0));
}}
@vertex
fn vertexMain(input: VertexInput) -> VertexOutput {{
  var output: VertexOutput;
  output.position = globals.viewProjection * vec4f(input.position, 1.0);
  output.color = input.color;
{stage_coordinates}  gxLighting(input, &output);
  return output;
}}
@fragment
fn fragmentMain(input: VertexOutput) -> @location(0) {fragment_output} {{
{fragment}
  {epilogue}
}}
"#
    )
}

/// GX's alpha test on the fragment's 8-bit alpha (`GXSetAlphaCompare`).
fn alpha_discard(compare: &HsdAlphaCompare) -> String {
    let term = |function: HsdCompare, reference: u8| {
        let operator = match function {
            HsdCompare::Never => return "false".to_owned(),
            HsdCompare::Always => return "true".to_owned(),
            HsdCompare::Less => "<",
            HsdCompare::Equal => "==",
            HsdCompare::LessEqual => "<=",
            HsdCompare::Greater => ">",
            HsdCompare::NotEqual => "!=",
            HsdCompare::GreaterEqual => ">=",
        };
        format!("alpha8 {operator} {reference}u")
    };
    let first = term(compare.first, compare.first_reference);
    let second = term(compare.second, compare.second_reference);
    let operator = match compare.op {
        HsdAlphaOp::And => "&&",
        HsdAlphaOp::Or => "||",
        HsdAlphaOp::Xor => "!=",
        HsdAlphaOp::Xnor => "==",
    };
    format!(
        "let alpha8 = u32(round(clamp(alpha, 0.0, 1.0) * 255.0));\n  \
         if (!(({first}) {operator} ({second}))) {{ discard; }}"
    )
}

/// Statements that leave the combined result in `color` and `alpha`. Every
/// TEV stage clamps to [0, 1] like GX_ENABLE clamping.
fn tev_fragment_body(material: &PreparedMaterial) -> String {
    let mut lines = Vec::new();
    if material.vertex_color {
        lines.push("var color: vec3f = material.baseColor.rgb * input.color.rgb;".to_owned());
        lines.push("var alpha: f32 = material.baseColor.a * input.color.a;".to_owned());
    } else {
        lines.push("var color: vec3f = material.baseColor.rgb;".to_owned());
        lines.push("var alpha: f32 = material.baseColor.a;".to_owned());
    }
    if material.tev_plan.contains(&TevStep::SpecularLighting) {
        lines.push("var specular: vec3f = material.specular.rgb;".to_owned());
    }
    for (index, stage) in material.stages.iter().enumerate() {
        stage_inputs(&mut lines, index, stage);
    }
    for step in &material.tev_plan {
        match *step {
            TevStep::DiffuseLighting => lines.push("color = color * input.lit0;".to_owned()),
            TevStep::SpecularLighting => {
                lines.push("color = min(color + specular * input.lit1, vec3f(1.0));".to_owned())
            }
            TevStep::Stage {
                stage,
                target,
                alpha,
            } => {
                let target = match target {
                    TevTarget::Color => "color",
                    TevTarget::Specular => "specular",
                };
                let ops = &material.stages[stage];
                lines.push(format!(
                    "{target} = {};",
                    color_op(ops.color_op, target, stage)
                ));
                if alpha {
                    lines.push(format!("alpha = {};", alpha_op(ops.alpha_op, stage)));
                }
            }
        }
    }
    lines
        .iter()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn stage_inputs(lines: &mut Vec<String>, index: usize, stage: &PreparedStage) {
    lines.push(format!(
        "var texel{index}: vec4f = textureSample(stageTexture{index}, stageSampler{index}, \
         input.stq{index}.xy / input.stq{index}.z);"
    ));
    let Some(program) = stage.custom_tev else {
        lines.push(format!("var stageRgb{index}: vec3f = texel{index}.rgb;"));
        lines.push(format!("var stageAlpha{index}: f32 = texel{index}.a;"));
        return;
    };
    // MakeColorGenTExp: an admitted per-TObj TEV replaces the texel inputs.
    let literal = |values: [u8; 4]| {
        let components = values.map(|value| format!("{:.6}", f64::from(value) / 255.0));
        format!("vec4f({})", components.join(", "))
    };
    lines.push(format!(
        "var konst{index}: vec4f = {};",
        literal(program.konst)
    ));
    lines.push(format!(
        "var tev0{index}: vec4f = {};",
        literal(program.tev0)
    ));
    let rgb = match program.color {
        Some(inputs) => {
            let [a, b, c, d] = inputs.map(|input| color_tev_input(input, index));
            format!("clamp(({a} * (vec3f(1.0) - {c}) + {b} * {c}) + {d}, vec3f(0.0), vec3f(1.0))")
        }
        None => format!("texel{index}.rgb"),
    };
    let alpha = match program.alpha {
        Some(inputs) => {
            let [a, b, c, d] = inputs.map(|input| alpha_tev_input(input, index));
            format!("clamp(({a} * (1.0 - {c}) + {b} * {c}) + {d}, 0.0, 1.0)")
        }
        None => format!("texel{index}.a"),
    };
    lines.push(format!("var stageRgb{index}: vec3f = {rgb};"));
    lines.push(format!("var stageAlpha{index}: f32 = {alpha};"));
}

/// TObjMakeTExp color maps, as out = d + ((1 - c) * a + c * b) with clamping.
fn color_op(op: HsdColorMap, current: &str, stage: usize) -> String {
    let source = format!("stageRgb{stage}");
    match op {
        HsdColorMap::AlphaMask => format!("mix({current}, {source}, stageAlpha{stage})"),
        HsdColorMap::RgbMask => format!("mix({current}, {source}, {source})"),
        HsdColorMap::Blend => {
            format!("mix({current}, {source}, material.stages[{stage}].params.x)")
        }
        HsdColorMap::Modulate => format!("{current} * {source}"),
        HsdColorMap::Replace => source,
        HsdColorMap::None | HsdColorMap::Pass => current.to_owned(),
        HsdColorMap::Add => format!("clamp({current} + {source}, vec3f(0.0), vec3f(1.0))"),
        HsdColorMap::Sub => format!("clamp({current} - {source}, vec3f(0.0), vec3f(1.0))"),
    }
}

/// TObjMakeTExp alpha maps.
fn alpha_op(op: HsdAlphaMap, stage: usize) -> String {
    let source = format!("stageAlpha{stage}");
    match op {
        HsdAlphaMap::AlphaMask => format!("mix(alpha, {source}, {source})"),
        HsdAlphaMap::Blend => format!("mix(alpha, {source}, material.stages[{stage}].params.x)"),
        HsdAlphaMap::Modulate => format!("alpha * {source}"),
        HsdAlphaMap::Replace => source,
        HsdAlphaMap::None | HsdAlphaMap::Pass => "alpha".to_owned(),
        HsdAlphaMap::Add => format!("clamp(alpha + {source}, 0.0, 1.0)"),
        HsdAlphaMap::Sub => format!("clamp(alpha - {source}, 0.0, 1.0)"),
    }
}

/// The WGSL swizzle of a KONST component; the parser admits only 0 to 3.
fn component(index: usize) -> char {
    match index {
        0 => 'r',
        1 => 'g',
        2 => 'b',
        _ => 'a',
    }
}

fn color_tev_input(input: HsdTObjTevColorInput, stage: usize) -> String {
    use HsdTObjTevColorInput::*;
    match input {
        Zero => "vec3f(0.0)".to_owned(),
        One => "vec3f(1.0)".to_owned(),
        Half => "vec3f(0.5)".to_owned(),
        TextureRgb => format!("texel{stage}.rgb"),
        TextureAlpha => format!("vec3f(texel{stage}.a)"),
        KonstRgb => format!("konst{stage}.rgb"),
        KonstComponent(index) => format!("vec3f(konst{stage}.{})", component(index)),
        Tev0Rgb => format!("tev0{stage}.rgb"),
        Tev0Alpha => format!("vec3f(tev0{stage}.a)"),
    }
}

fn alpha_tev_input(input: HsdTObjTevAlphaInput, stage: usize) -> String {
    use HsdTObjTevAlphaInput::*;
    match input {
        Zero => "0.0".to_owned(),
        TextureAlpha => format!("texel{stage}.a"),
        KonstComponent(index) => format!("konst{stage}.{}", component(index)),
        Tev0Alpha => format!("tev0{stage}.a"),
    }
}

/// Material uniform block for one packet. `pick_id` is exact in the f32 slot
/// below 2^24, far above any scene's packet count.
pub fn material_uniforms(
    material: &PreparedMaterial,
    joint_position: [f32; 3],
    pick_id: u32,
) -> [f32; MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; MATERIAL_UNIFORM_FLOATS];
    values[0..4].copy_from_slice(&material.base_color);
    values[4..7].copy_from_slice(&material.ambient);
    values[7] = pick_id as f32;
    values[8..11].copy_from_slice(&material.specular);
    values[11] = material.shininess;
    values[12..15].copy_from_slice(&joint_position);
    for (index, stage) in material.stages.iter().enumerate() {
        let offset = 16 + index * 16;
        for (row, values) in stage
            .matrix
            .iter()
            .zip(values[offset..offset + 12].as_chunks_mut::<4>().0)
        {
            *values = *row;
        }
        values[offset + 12] = stage.blending;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::PreparedTev;
    use crate::material::test_support::{DIFFUSE, EXT, SPECULAR, material, stage};

    /// Both the color and the pick program must be valid WGSL.
    fn validate(material: &PreparedMaterial) {
        for source in [material_shader(material), pick_shader(material)] {
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{}\n{source}", error.emit_to_string(&source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{error:?}\n{source}"));
        }
    }

    #[test]
    fn translucent_materials_pick_only_where_mostly_opaque() {
        let mut translucent = material(Vec::new(), &[]);
        translucent.blend = HsdBlendMode::SOURCE_ALPHA;
        assert!(pick_shader(&translucent).contains("if (alpha < 0.25) { discard; }"));
        validate(&translucent);
        assert!(!pick_shader(&material(Vec::new(), &[])).contains("alpha < 0.25"));
        // A depth-only occluder hides exactly what it hides on screen.
        translucent.color_update = false;
        assert!(!pick_shader(&translucent).contains("alpha < 0.25"));
        validate(&translucent);
    }

    #[test]
    fn alpha_compare_discards_on_the_8_bit_alpha() {
        // Battlefield's cut-out PEDesc: alpha in [229, 255].
        let mut cutout = material(Vec::new(), &[]);
        cutout.alpha_compare = Some(HsdAlphaCompare {
            first: HsdCompare::GreaterEqual,
            first_reference: 229,
            op: HsdAlphaOp::And,
            second: HsdCompare::LessEqual,
            second_reference: 255,
        });
        let test = "if (!((alpha8 >= 229u) && (alpha8 <= 255u))) { discard; }";
        assert!(material_shader(&cutout).contains(test));
        assert!(
            pick_shader(&cutout).contains(test),
            "picks match the cut-out"
        );
        validate(&cutout);

        for op in [HsdAlphaOp::Or, HsdAlphaOp::Xor, HsdAlphaOp::Xnor] {
            cutout.alpha_compare = Some(HsdAlphaCompare {
                first: HsdCompare::Never,
                first_reference: 0,
                op,
                second: HsdCompare::NotEqual,
                second_reference: 7,
            });
            validate(&cutout);
        }

        cutout.alpha_compare = None;
        assert!(!material_shader(&cutout).contains("alpha8"));
    }

    #[test]
    fn a_material_declares_only_the_extra_tex_coords_it_reads() {
        use crate::material::test_support::DIFFUSE;
        // Kongo Jungle N64 layers five textures, each on its own set.
        let stages = (0..5)
            .map(|set| {
                stage(
                    StageSource::TexCoord(set),
                    HsdColorMap::Modulate,
                    HsdAlphaMap::None,
                )
            })
            .collect();
        let mut layered = material(stages, &[DIFFUSE; 5]);
        layered.required_tex_coord_mask = 0b1_1111;
        let shader = material_shader(&layered);
        for (set, location) in [(2, 5), (3, 6), (4, 7)] {
            assert!(shader.contains(&format!("@location({location}) uv{set}: vec2f")));
        }
        assert!(!shader.contains("uv5"));
        validate(&layered);
        assert!(!material_shader(&material(Vec::new(), &[])).contains("uv2"));
    }

    #[test]
    fn every_stage_operation_is_valid_wgsl() {
        use HsdAlphaMap as A;
        use HsdColorMap as C;
        let color_ops = [
            C::None,
            C::AlphaMask,
            C::RgbMask,
            C::Blend,
            C::Modulate,
            C::Replace,
            C::Pass,
            C::Add,
            C::Sub,
        ];
        let alpha_ops = [
            A::None,
            A::AlphaMask,
            A::Blend,
            A::Modulate,
            A::Replace,
            A::Pass,
            A::Add,
            A::Sub,
        ];
        for (index, color_op) in color_ops.into_iter().enumerate() {
            let alpha_op = alpha_ops[index % alpha_ops.len()];
            let stages = vec![
                stage(StageSource::TexCoord(0), color_op, alpha_op),
                stage(StageSource::TexCoord(1), color_op, alpha_op),
                stage(StageSource::Reflection, color_op, alpha_op),
                stage(StageSource::TexCoord(0), color_op, alpha_op),
            ];
            validate(&material(stages, &[DIFFUSE, SPECULAR, EXT, DIFFUSE | EXT]));
        }
    }

    #[test]
    fn custom_tev_is_valid_wgsl() {
        use HsdTObjTevAlphaInput as A;
        use HsdTObjTevColorInput as C;
        let mut custom = stage(
            StageSource::TexCoord(0),
            HsdColorMap::Modulate,
            HsdAlphaMap::Modulate,
        );
        custom.custom_tev = Some(PreparedTev {
            color: Some([C::TextureRgb, C::KonstRgb, C::TextureAlpha, C::Tev0Rgb]),
            alpha: Some([A::TextureAlpha, A::KonstComponent(3), A::Zero, A::Tev0Alpha]),
            konst: [255, 128, 0, 255],
            tev0: [0, 0, 0, 64],
        });
        let source = material_shader(&material(vec![custom.clone()], &[DIFFUSE]));
        assert!(
            source.contains("var konst0: vec4f = vec4f(1.000000, 0.501961, 0.000000, 1.000000);")
        );
        validate(&material(vec![custom], &[DIFFUSE]));
    }

    #[test]
    fn material_uniforms_follow_the_shader_layout() {
        let mut blended = stage(
            StageSource::TexCoord(0),
            HsdColorMap::Blend,
            HsdAlphaMap::Blend,
        );
        blended.matrix[1][3] = 7.0;
        let values = material_uniforms(&material(vec![blended], &[DIFFUSE]), [1.0, 2.0, 3.0], 9);
        assert_eq!(values[7], 9.0);
        assert_eq!(&values[8..12], &[1.0, 1.0, 1.0, 50.0]);
        assert_eq!(&values[12..16], &[1.0, 2.0, 3.0, 0.0]);
        assert_eq!(values[16 + 7], 7.0);
        assert_eq!(values[16 + 12], 0.5);
    }
}
