//! Preparation of `HsdScene` plus evaluated draw work into interleaved
//! vertices and source-ordered draw packets; [`crate::material`] prepares each
//! packet's material and textures.

use crate::camera::Focus;
use crate::error::{HsdRenderError, Result, invalid_draw_work, invalid_scene};
use crate::material::{
    PreparedMaterial, PreparedTexture, StageSource, TextureCache, prepare_material,
};
use dat_parser::gx::vertex::DecodedVertex;
use dat_parser::hsd::draw::{HsdEvaluatedDrawPacket, HsdEvaluatedDrawRoot, HsdEvaluatedDrawWork};
use dat_parser::hsd::pe::HsdDrawPass;
use dat_parser::hsd::scene::{HsdDisplayObject, HsdPolygon, HsdScene};
use dat_parser::hsd::source::{HsdFocus, HsdSource};

const CULL_FRONT_FLAG: u16 = 1 << 14;
const CULL_BACK_FLAG: u16 = 1 << 15;
/// Position, normal, TEX0, TEX1, COLOR0: every scene's vertex starts with
/// these. A scene whose materials read TEX2 and up appends those sets.
pub const BASE_FLOATS_PER_VERTEX: usize = 14;
/// The sets every vertex carries (TEX0, TEX1), and GX's limit.
pub const BASE_TEX_COORD_SETS: usize = 2;
pub const MAX_TEX_COORD_SETS: usize = 8;

/// Resource limits for one prepared scene.
pub const MAX_DRAW_CALLS: usize = 1024;
pub const MAX_VERTICES: usize = 250_000;
pub const MAX_TRIANGLES: usize = 500_000;
pub const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;

// HSD clears VtxDesc before installing each PObj's declarations (melee-90f83f6,
// pobj.c setupVtxDesc, lines 455-481). Absent TEX inputs use GX's (0,0,1,1)
// texgen input, not a decoded UV: Dolphin initializes `coord` to that value
// and replaces it only when texcoord_elem_count is nonzero. Enabled but
// undecoded attributes still fail the required-decoded-mask validation.
const ABSENT_GX_TEX_COORD: [f32; 2] = [0.0, 0.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CullMode {
    None,
    Front,
    Back,
}

#[derive(Clone, Debug)]
pub struct PreparedPacket {
    pub polygon_source_id: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    pub cull_mode: CullMode,
    pub pass: HsdDrawPass,
    /// World position of the packet joint. HSD computes each specular
    /// half-vector from the camera direction to the joint, not per vertex
    /// (lobj.c:301).
    pub joint_position: [f32; 3],
    /// Per-frame JOBJ_HIDDEN admission; a hidden packet keeps its buffers.
    pub visible: bool,
    pub material: PreparedMaterial,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub center: [f64; 3],
    pub radius: f64,
}

#[derive(Clone, Debug)]
pub struct PreparedGeometry {
    pub vertices: Vec<f32>,
    pub indices: Vec<u32>,
    /// Source occurrence order; vertex updates rely on it.
    pub packets: Vec<PreparedPacket>,
    /// Packet indices in draw order: source order within each HSD pass.
    pub draw_order: Vec<usize>,
    pub textures: Vec<PreparedTexture>,
    pub bounds: Bounds,
    /// What the camera frames; the whole of `bounds` when `None`.
    pub focus: Option<Focus>,
    /// How many texture-coordinate sets each vertex carries: the highest one
    /// a material reads, and at least TEX0 and TEX1.
    pub tex_coord_sets: usize,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub texture_bytes: usize,
}

struct PacketSource<'a> {
    display_object: &'a HsdDisplayObject,
    polygon: &'a HsdPolygon,
    root: &'a HsdEvaluatedDrawRoot,
    packet: &'a HsdEvaluatedDrawPacket,
    cull_mode: CullMode,
}

impl PreparedGeometry {
    /// Evaluate `source`'s bind pose and prepare it for drawing.
    pub fn bind_pose(source: &mut HsdSource) -> Result<Self> {
        let focus = source.focus;
        let (scene, work) = source.evaluate_bind_pose()?;
        Ok(Self::new(scene, work)?.with_focus(focus))
    }

    pub const fn floats_per_vertex(&self) -> usize {
        floats_per_vertex(self.tex_coord_sets)
    }

    /// Frame `focus` in place of the whole scene.
    pub fn with_focus(mut self, focus: Option<HsdFocus>) -> Self {
        self.focus = focus.map(|focus| Focus {
            center: focus.center.map(f64::from),
            half_width: f64::from(focus.half_width),
            half_height: f64::from(focus.half_height),
        });
        self
    }

    pub fn new(scene: &HsdScene, work: &HsdEvaluatedDrawWork) -> Result<Self> {
        let sources = collect_packet_sources(scene, work)?;
        let mut vertex_count = 0;
        let mut triangle_count = 0;
        for source in &sources {
            vertex_count = checked_count(
                vertex_count,
                source.polygon.decoded.vertices.len(),
                MAX_VERTICES,
                "vertex",
            )?;
            triangle_count = checked_count(
                triangle_count,
                source.polygon.decoded.triangles.len(),
                MAX_TRIANGLES,
                "triangle",
            )?;
        }
        checked_count(0, sources.len(), MAX_DRAW_CALLS, "draw-call")?;
        if vertex_count == 0 || triangle_count == 0 {
            return Err(HsdRenderError::EmptyGeometry);
        }

        // Materials first: the sets they read decide the vertex layout.
        let mut textures = Vec::new();
        let mut texture_cache = TextureCache::default();
        let mut materials = Vec::with_capacity(sources.len());
        for source in &sources {
            materials.push(prepare_material(
                source.display_object,
                scene,
                &mut textures,
                &mut texture_cache,
            )?);
        }
        let tex_coord_sets = materials
            .iter()
            .map(|material| (u8::BITS - material.required_tex_coord_mask.leading_zeros()) as usize)
            .max()
            .unwrap_or(0)
            .max(BASE_TEX_COORD_SETS);
        let floats_per_vertex = floats_per_vertex(tex_coord_sets);

        let mut vertices = vec![0.0; vertex_count * floats_per_vertex];
        let mut indices = Vec::with_capacity(triangle_count * 3);
        let mut packets = Vec::with_capacity(sources.len());
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        let mut vertex_cursor = 0;

        for (source, material) in sources.iter().zip(materials) {
            let PacketSource {
                display_object,
                polygon,
                root,
                packet,
                cull_mode,
            } = *source;
            let required_decoded_mask =
                material.required_tex_coord_mask & polygon.tex_coord_attribute_mask();
            let first_index = indices.len();
            let first_vertex = vertex_cursor;
            for (local, vertex) in polygon.decoded.vertices.iter().enumerate() {
                let position = read_vec3(root.positions[packet.first_vertex + local], "position")?;
                let normal = read_vec3(root.normals[packet.first_vertex + local], "normal")?;
                if vertex
                    .color0
                    .iter()
                    .any(|component| !(0.0..=1.0).contains(component))
                {
                    return invalid_draw_work("source vertex color is invalid");
                }
                if vertex.tex_coord_mask & required_decoded_mask != required_decoded_mask {
                    return invalid_draw_work(
                        "source vertex is missing a required decoded texture coordinate",
                    );
                }
                let output = &mut vertices
                    [vertex_cursor * floats_per_vertex..(vertex_cursor + 1) * floats_per_vertex];
                output[0..3].copy_from_slice(&position);
                output[3..6].copy_from_slice(&normal);
                // Only coordinates a stage reads are validated, so only those are written.
                for index in 0..tex_coord_sets {
                    if material.required_tex_coord_mask & (1 << index) == 0 {
                        continue;
                    }
                    let uv = source_tex_coord(polygon, vertex, index);
                    if uv.iter().any(|component| !component.is_finite()) {
                        return invalid_draw_work("source texture coordinate is invalid");
                    }
                    let offset = tex_coord_offset(index);
                    output[offset..offset + 2].copy_from_slice(&uv);
                }
                output[10..14].copy_from_slice(&vertex.color0);
                for axis in 0..3 {
                    minimum[axis] = minimum[axis].min(f64::from(position[axis]));
                    maximum[axis] = maximum[axis].max(f64::from(position[axis]));
                }
                vertex_cursor += 1;
            }
            for triangle in &polygon.decoded.triangles {
                if triangle
                    .iter()
                    .any(|&index| index >= polygon.decoded.vertices.len())
                {
                    return invalid_draw_work("triangle references an invalid source vertex");
                }
                // GX's clockwise front faces become counter-clockwise here.
                let [a, b, c] = triangle.map(|index| (first_vertex + index) as u32);
                indices.extend([a, c, b]);
            }
            for stage in &material.stages {
                if let StageSource::TexCoord(index) = stage.source {
                    validate_texture_projection(polygon, stage.matrix[2], usize::from(index))?;
                }
            }
            let Some(pass) = display_object.pass() else {
                return invalid_scene("a display object's render mode names no display pass");
            };
            packets.push(PreparedPacket {
                polygon_source_id: polygon.source_id.0,
                first_index: first_index as u32,
                index_count: (indices.len() - first_index) as u32,
                first_vertex: first_vertex as u32,
                vertex_count: polygon.decoded.vertices.len() as u32,
                cull_mode,
                pass,
                joint_position: joint_position(root, packet)?,
                visible: packet.visible,
                material,
            });
        }

        let mut texture_bytes = 0;
        for texture in &textures {
            texture_bytes = checked_count(
                texture_bytes,
                texture.rgba.len(),
                MAX_TEXTURE_BYTES,
                "texture-byte",
            )?;
        }
        let center = [0, 1, 2].map(|axis| (minimum[axis] + maximum[axis]) / 2.0);
        let radius = [0, 1, 2]
            .map(|axis| maximum[axis] - center[axis])
            .iter()
            .map(|extent| extent * extent)
            .sum::<f64>()
            .sqrt()
            .max(0.01);
        if !radius.is_finite() {
            return invalid_draw_work("evaluated bounds are invalid");
        }

        Ok(Self {
            vertices,
            indices,
            draw_order: draw_order(&packets),
            packets,
            textures,
            bounds: Bounds {
                min: minimum,
                max: maximum,
                center,
                radius,
            },
            focus: None,
            tex_coord_sets,
            vertex_count,
            triangle_count,
            texture_bytes,
        })
    }

    /// Copy a new frame's positions, normals, joint positions, and visibility
    /// into the prepared buffers. Topology must match preparation.
    /// The prepared texture that `HsdScene::textures[scene_texture]` shares,
    /// or `None` when no drawn stage samples it or it did not decode.
    pub fn texture_for_scene_texture(&self, scene_texture: u32) -> Option<usize> {
        self.textures
            .iter()
            .position(|texture| texture.scene_textures.contains(&scene_texture))
    }

    pub fn update_vertices(&mut self, scene: &HsdScene, work: &HsdEvaluatedDrawWork) -> Result<()> {
        let sources = collect_packet_sources(scene, work)?;
        if sources.len() != self.packets.len() {
            return invalid_draw_work("animated draw work changed renderable packet count");
        }
        let floats_per_vertex = self.floats_per_vertex();
        for (prepared, source) in self.packets.iter_mut().zip(&sources) {
            if prepared.polygon_source_id != source.polygon.source_id.0
                || prepared.vertex_count as usize != source.polygon.decoded.vertices.len()
            {
                return invalid_draw_work("animated draw work changed static scene topology");
            }
            prepared.joint_position = joint_position(source.root, source.packet)?;
            prepared.visible = source.packet.visible;
            let first = source.packet.first_vertex;
            for local in 0..prepared.vertex_count as usize {
                let position = source.root.positions[first + local];
                let normal = source.root.normals[first + local];
                if position
                    .iter()
                    .chain(&normal)
                    .any(|component| !component.is_finite())
                {
                    return invalid_draw_work(
                        "evaluated position or normal contains a non-finite component",
                    );
                }
                let offset = (prepared.first_vertex as usize + local) * floats_per_vertex;
                self.vertices[offset..offset + 3].copy_from_slice(&position);
                self.vertices[offset + 3..offset + 6].copy_from_slice(&normal);
            }
        }
        Ok(())
    }

    /// Reflection's normalized normal is view-dependent. Validate the same full
    /// Q projection after camera or pose changes. GX's Q-zero sampling is not a
    /// fallback this backend implements.
    pub fn validate_reflections(&self, view: &[f32; 16]) -> Result<()> {
        if view.iter().any(|value| !value.is_finite()) {
            return invalid_draw_work("reflection viewing matrix is invalid");
        }
        for packet in &self.packets {
            for matrix in &packet.material.reflection_matrices {
                let q_row = matrix[2];
                let first = packet.first_vertex as usize;
                let mut positive = false;
                let mut negative = false;
                for vertex in first..first + packet.vertex_count as usize {
                    let q = self.reflection_q(vertex, view, q_row)?;
                    if !q.is_finite() || q == 0.0 {
                        return invalid_draw_work(
                            "reflection texture projection requires finite nonzero Q",
                        );
                    }
                    positive |= q > 0.0;
                    negative |= q < 0.0;
                }
                if !(positive && negative) {
                    continue;
                }
                let first_index = packet.first_index as usize;
                for triangle in self.indices[first_index..first_index + packet.index_count as usize]
                    .as_chunks::<3>()
                    .0
                {
                    let signs = triangle
                        .iter()
                        .map(|&vertex| Ok(self.reflection_q(vertex as usize, view, q_row)? > 0.0))
                        .collect::<Result<Vec<_>>>()?;
                    if signs.iter().any(|&sign| sign != signs[0]) {
                        return invalid_draw_work(
                            "reflection texture projection crosses Q=0 within a triangle",
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn reflection_q(&self, vertex: usize, view: &[f32; 16], q_row: [f32; 4]) -> Result<f32> {
        let offset = vertex * self.floats_per_vertex() + 3;
        let [nx, ny, nz] = [0, 1, 2].map(|axis| f64::from(self.vertices[offset + axis]));
        let view = view.map(f64::from);
        // World-space normals already include native inverse-transpose
        // deformation. The remaining GX_TEXMTX input transform is the viewing
        // rotation (w=0).
        let x = (view[0] * nx + view[4] * ny + view[8] * nz) as f32;
        let y = (view[1] * nx + view[5] * ny + view[9] * nz) as f32;
        let z = (view[2] * nx + view[6] * ny + view[10] * nz) as f32;
        let squared_length = x * x + y * y + z * z;
        if !squared_length.is_finite() || squared_length <= 0.0 {
            return invalid_draw_work("reflection requires a finite nonzero normal");
        }
        let length = f64::from(squared_length).sqrt();
        let [x, y, z] = [x, y, z].map(|component| (f64::from(component) / length) as f32);
        let q = f64::from(q_row[0]) * f64::from(x)
            + f64::from(q_row[1]) * f64::from(y)
            + f64::from(q_row[2]) * f64::from(z)
            + f64::from(q_row[3]);
        Ok(q as f32)
    }
}

// HSD draws every opaque DObj, then every edge-cutout DObj, then every
// translucent DObj (melee gobj.c:31 maps render passes to HSD_TRSP_OPA,
// HSD_TRSP_TEXEDGE, HSD_TRSP_XLU). Source order is kept within each pass.
fn pass_order(pass: HsdDrawPass) -> u8 {
    match pass {
        HsdDrawPass::Opaque => 0,
        HsdDrawPass::TexEdge => 1,
        HsdDrawPass::Translucent => 2,
    }
}

fn draw_order(packets: &[PreparedPacket]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..packets.len()).collect();
    // A stable sort keeps source order within each pass.
    order.sort_by_key(|&index| pass_order(packets[index].pass));
    order
}

fn joint_position(
    root: &HsdEvaluatedDrawRoot,
    packet: &HsdEvaluatedDrawPacket,
) -> Result<[f32; 3]> {
    let Some(matrix) = root.joint_world_matrices.get(packet.joint_index.0) else {
        return invalid_draw_work("evaluated packet joint is out of range");
    };
    // Column-major joint matrices: translation is the fourth column.
    let translation = matrix.0[3];
    read_vec3(
        [translation[0], translation[1], translation[2]],
        "joint translation",
    )
}

fn collect_packet_sources<'a>(
    scene: &'a HsdScene,
    work: &'a HsdEvaluatedDrawWork,
) -> Result<Vec<PacketSource<'a>>> {
    if work.roots.len() != scene.roots.len() {
        return invalid_draw_work("scene and evaluated root counts do not match");
    }
    let mut sources = Vec::new();
    for (root, evaluated) in scene.roots.iter().zip(&work.roots) {
        if evaluated.source_id != root.source_id {
            return invalid_draw_work("evaluated root identity does not match scene");
        }
        let stream_length = evaluated.positions.len();
        if evaluated.joint_world_matrices.len() != root.joints.len()
            || evaluated.normals.len() != stream_length
            || evaluated.binormals.len() != stream_length
            || evaluated.tangents.len() != stream_length
        {
            return invalid_draw_work("evaluated root stream extents are invalid");
        }
        let mut expected_first_vertex = 0;
        for packet in &evaluated.packets {
            let end = packet.first_vertex.checked_add(packet.vertex_count);
            if packet.first_vertex != expected_first_vertex
                || end.is_none_or(|end| end > stream_length)
            {
                return invalid_draw_work("evaluated packet indices or range are invalid");
            }
            let polygon = root
                .joints
                .get(packet.joint_index.0)
                .and_then(|joint| joint.display_objects.get(packet.display_object_index))
                .and_then(|display_object| {
                    Some((
                        display_object,
                        display_object.polygons.get(packet.polygon_index)?,
                    ))
                });
            let Some((display_object, polygon)) = polygon else {
                return invalid_draw_work("evaluated packet references an invalid scene path");
            };
            if packet.polygon_source_id != polygon.source_id
                || packet.vertex_count != polygon.decoded.vertices.len()
            {
                return invalid_draw_work("evaluated packet does not match scene polygon");
            }
            expected_first_vertex = packet.first_vertex + packet.vertex_count;
            if polygon.decoded.vertices.is_empty()
                || polygon.decoded.triangles.is_empty()
                || culls_every_face(polygon)
            {
                continue;
            }
            sources.push(PacketSource {
                display_object,
                polygon,
                root: evaluated,
                packet,
                cull_mode: cull_mode(polygon),
            });
        }
        if expected_first_vertex != stream_length {
            return invalid_draw_work("evaluated packet ranges do not exactly match draw streams");
        }
    }
    Ok(sources)
}

/// The vertex shader location of TEX`index`: 2 and 3 for the base sets, then
/// 5 and up, after the color at 4.
pub const fn tex_coord_location(index: usize) -> u32 {
    if index < BASE_TEX_COORD_SETS {
        2 + index as u32
    } else {
        3 + index as u32
    }
}

/// Floats in one vertex that carries `tex_coord_sets` sets.
pub const fn floats_per_vertex(tex_coord_sets: usize) -> usize {
    BASE_FLOATS_PER_VERTEX + (tex_coord_sets - BASE_TEX_COORD_SETS) * 2
}

/// Where a vertex keeps TEX`index`: TEX0 and TEX1 before the color, the
/// rest after it, so the base layout is the same in every scene.
pub const fn tex_coord_offset(index: usize) -> usize {
    if index < BASE_TEX_COORD_SETS {
        6 + index * 2
    } else {
        BASE_FLOATS_PER_VERTEX + (index - BASE_TEX_COORD_SETS) * 2
    }
}

fn source_tex_coord(polygon: &HsdPolygon, vertex: &DecodedVertex, index: usize) -> [f32; 2] {
    if polygon.tex_coord_attribute_mask() & (1 << index) == 0 {
        ABSENT_GX_TEX_COORD
    } else {
        vertex.tex_coords[index]
    }
}

fn validate_texture_projection(
    polygon: &HsdPolygon,
    q_row: [f32; 4],
    tex_coord: usize,
) -> Result<()> {
    // GX's Q=0 sampling rule is not implemented. Reject singular
    // vertices/primitives rather than clamping or dividing early.
    if q_row[0] == 0.0 && q_row[1] == 0.0 {
        let q = (f64::from(q_row[2]) + f64::from(q_row[3])) as f32;
        if !q.is_finite() || q == 0.0 {
            return invalid_draw_work("texture projection requires finite nonzero Q");
        }
        return Ok(());
    }
    let q_for =
        |vertex: &DecodedVertex| texture_q(q_row, source_tex_coord(polygon, vertex, tex_coord));
    let mut positive = false;
    let mut negative = false;
    for vertex in &polygon.decoded.vertices {
        let q = q_for(vertex);
        if !q.is_finite() || q == 0.0 {
            return invalid_draw_work("texture projection requires finite nonzero Q");
        }
        positive |= q > 0.0;
        negative |= q < 0.0;
    }
    if !(positive && negative) {
        return Ok(());
    }
    for triangle in &polygon.decoded.triangles {
        let signs = triangle.map(|index| q_for(&polygon.decoded.vertices[index]) > 0.0);
        if signs[1] != signs[0] || signs[2] != signs[0] {
            return invalid_draw_work("texture projection crosses Q=0 within a triangle");
        }
    }
    Ok(())
}

fn texture_q(row: [f32; 4], uv: [f32; 2]) -> f32 {
    (f64::from(row[0]) * f64::from(uv[0])
        + f64::from(row[1]) * f64::from(uv[1])
        + f64::from(row[2])
        + f64::from(row[3])) as f32
}

fn checked_count(
    current: usize,
    increment: usize,
    maximum: usize,
    label: &'static str,
) -> Result<usize> {
    let actual = current.saturating_add(increment);
    if actual > maximum {
        return Err(HsdRenderError::ResourceLimit {
            label,
            maximum,
            actual,
        });
    }
    Ok(actual)
}

fn read_vec3(value: [f32; 3], label: &str) -> Result<[f32; 3]> {
    if value.iter().any(|component| !component.is_finite()) {
        return invalid_draw_work(format!("evaluated {label} contains a non-finite component"));
    }
    Ok(value)
}

fn culls_every_face(polygon: &HsdPolygon) -> bool {
    polygon.flags & CULL_BACK_FLAG != 0 && polygon.flags & CULL_FRONT_FLAG != 0
}

fn cull_mode(polygon: &HsdPolygon) -> CullMode {
    if polygon.flags & CULL_BACK_FLAG != 0 {
        CullMode::Back
    } else if polygon.flags & CULL_FRONT_FLAG != 0 {
        CullMode::Front
    } else {
        CullMode::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_tex_coord_sets_follow_the_base_vertex() {
        // A scene that reads only TEX0 and TEX1 keeps the 14-float vertex.
        assert_eq!(floats_per_vertex(BASE_TEX_COORD_SETS), 14);
        assert_eq!((tex_coord_offset(0), tex_coord_location(0)), (6, 2));
        assert_eq!((tex_coord_offset(1), tex_coord_location(1)), (8, 3));
        // COLOR0 stays at floats 10..14 and location 4; TEX2 starts after it.
        assert_eq!((tex_coord_offset(2), tex_coord_location(2)), (14, 5));
        assert_eq!((tex_coord_offset(7), tex_coord_location(7)), (24, 10));
        assert_eq!(floats_per_vertex(MAX_TEX_COORD_SETS), 26);
    }

    #[test]
    fn a_budget_admits_its_maximum_and_refuses_one_more() {
        assert_eq!(
            checked_count(MAX_DRAW_CALLS - 1, 1, MAX_DRAW_CALLS, "draw-call").unwrap(),
            MAX_DRAW_CALLS
        );
        let Err(HsdRenderError::ResourceLimit {
            label,
            maximum,
            actual,
        }) = checked_count(MAX_VERTICES, 1, MAX_VERTICES, "vertex")
        else {
            panic!("one vertex past the budget is refused");
        };
        assert_eq!(
            (label, maximum, actual),
            ("vertex", MAX_VERTICES, MAX_VERTICES + 1)
        );
        // A count too large to add saturates into a refusal, not a wrap.
        assert!(checked_count(usize::MAX, 2, MAX_TRIANGLES, "triangle").is_err());
    }
}
