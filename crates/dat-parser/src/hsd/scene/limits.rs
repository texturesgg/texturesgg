use super::HsdSceneError;

#[derive(Clone, Copy, Debug)]
pub struct HsdSceneLimits {
    pub max_roots: usize,
    pub max_joints: usize,
    pub max_display_objects: usize,
    pub max_polygons: usize,
    pub max_texture_objects: usize,
    pub max_gx_attributes: usize,
    pub max_display_list_bytes: usize,
    pub max_primitive_groups: usize,
    pub max_vertex_attribute_decodes: usize,
    pub max_vertices: usize,
    pub max_triangles: usize,
    pub max_textures: usize,
    pub max_decoded_texture_bytes: usize,
    pub max_envelopes: usize,
    pub max_envelope_weights: usize,
}

impl Default for HsdSceneLimits {
    fn default() -> Self {
        Self {
            max_roots: 4_096,
            max_joints: 65_536,
            max_display_objects: 262_144,
            max_polygons: 262_144,
            max_texture_objects: 262_144,
            max_gx_attributes: 1_000_000,
            max_display_list_bytes: 128 * 1024 * 1024,
            max_primitive_groups: 2_000_000,
            max_vertex_attribute_decodes: 32_000_000,
            max_vertices: 2_000_000,
            max_triangles: 4_000_000,
            max_textures: 65_536,
            max_decoded_texture_bytes: 128 * 1024 * 1024,
            max_envelopes: 262_144,
            max_envelope_weights: 1_048_576,
        }
    }
}
/// The largest DAT a host loads for drawing.
pub const HSD_SCENE_MAX_DAT_BYTES: usize = 50 * 1024 * 1024;

/// The budgets a host draws within: tighter than the defaults, which only
/// bound a parse.
pub fn hsd_scene_limits() -> HsdSceneLimits {
    HsdSceneLimits {
        max_roots: 4_096,
        max_joints: 16_384,
        max_display_objects: 32_768,
        max_polygons: 65_536,
        max_texture_objects: 65_536,
        max_gx_attributes: 250_000,
        max_display_list_bytes: 32 * 1024 * 1024,
        max_primitive_groups: 500_000,
        max_vertex_attribute_decodes: 8_000_000,
        max_vertices: 500_000,
        max_triangles: 1_000_000,
        max_textures: 8_192,
        max_decoded_texture_bytes: 64 * 1024 * 1024,
        max_envelopes: 250_000,
        max_envelope_weights: 1_000_000,
    }
}

pub(super) fn checked_budget(
    current: usize,
    additional: usize,
    limit: usize,
    resource: &'static str,
) -> Result<usize, HsdSceneError> {
    current
        .checked_add(additional)
        .filter(|total| *total <= limit)
        .ok_or(HsdSceneError::LimitExceeded { resource, limit })
}
