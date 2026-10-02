//! Scene lighting for HSD previews.
//!
//! A costume DAT carries material colors but no lights: in Melee the current
//! scene supplies them. Presets model HSD's ambient LObj and infinite LObjs. An
//! infinite light is a GX light placed far along its direction
//! (lobj.c setup_infinite_lightobj), so its diffuse term is max(0, N·L) with no
//! distance or spot attenuation.

use crate::error::{HsdRenderError, Result};

/// GX has eight hardware lights; four covers the presets without wasting uniforms.
pub const HSD_MAX_LIGHTS: usize = 4;
/// Ambient vec4, then two vec4s (color + diffuse flag, world direction + specular flag) per light.
pub const HSD_LIGHTING_UNIFORM_FLOATS: usize = 4 + HSD_MAX_LIGHTS * 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsdInfiniteLight {
    /// Raw 8-bit GX light color as 0..1 components.
    pub color: [f64; 3],
    /// Unit vector from the model toward the light, in view space: the
    /// light stays fixed relative to the camera while orbiting.
    pub toward: [f64; 3],
    /// Contributes to lit COLOR0 (RENDER_DIFFUSE materials).
    pub diffuse: bool,
    /// Contributes to lit COLOR1 (RENDER_SPECULAR materials).
    pub specular: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HsdLightingPreset {
    /// Recorded in capture metadata.
    pub id: &'static str,
    /// Ambient LObj color; HSD multiplies it by each material's ambient color.
    pub ambient: [f64; 3],
    pub lights: Vec<HsdInfiniteLight>,
}

/// Neutral preview lighting: not a Melee scene.
///
/// HSD multiplies materials by lit COLOR0 clamped to one, so lighting can only
/// darken the unlit color; the key light therefore brings camera-facing
/// surfaces to full brightness and leaves shading to the sides. It sits
/// slightly above and left of the camera and provides specular highlights; a
/// dim fill from the right keeps the far side readable.
pub fn neutral_preview_lighting() -> HsdLightingPreset {
    HsdLightingPreset {
        id: "neutral-preview-v1",
        ambient: [0.6, 0.6, 0.6],
        lights: vec![
            HsdInfiniteLight {
                color: [1.0, 1.0, 1.0],
                toward: normalize([-0.3, 0.5, 0.8]),
                diffuse: true,
                specular: true,
            },
            HsdInfiniteLight {
                color: [0.35, 0.35, 0.35],
                toward: normalize([0.7, -0.1, 0.7]),
                diffuse: true,
                specular: false,
            },
        ],
    }
}

impl HsdLightingPreset {
    pub fn validate(&self) -> Result<()> {
        let invalid = |message: &str| Err(HsdRenderError::InvalidLighting(message.into()));
        if self.id.is_empty() {
            return invalid("preset id is empty");
        }
        if !is_unit_color(self.ambient) {
            return invalid("ambient color is invalid");
        }
        if self.lights.len() > HSD_MAX_LIGHTS {
            return invalid("presets support at most four lights");
        }
        for light in &self.lights {
            if !is_unit_color(light.color) {
                return invalid("light color is invalid");
            }
            let [x, y, z] = light.toward;
            if !light.toward.iter().all(|value| value.is_finite())
                || ((x * x + y * y + z * z).sqrt() - 1.0).abs() > 1e-4
            {
                return invalid("light direction must be a unit vector");
            }
        }
        Ok(())
    }

    /// Pack the preset for the shaders with every direction in world space, so
    /// shaders can light world-space normals. View-space directions rotate with
    /// the camera: world = transpose(R) * view for the view matrix's rotation R
    /// (column-major).
    pub fn uniforms(&self, view: &[f32; 16]) -> [f32; HSD_LIGHTING_UNIFORM_FLOATS] {
        let mut values = [0.0; HSD_LIGHTING_UNIFORM_FLOATS];
        for (axis, component) in self.ambient.iter().enumerate() {
            values[axis] = *component as f32;
        }
        let view = view.map(f64::from);
        for (index, light) in self.lights.iter().enumerate() {
            let [x, y, z] = light.toward;
            let toward = normalize([
                view[0] * x + view[1] * y + view[2] * z,
                view[4] * x + view[5] * y + view[6] * z,
                view[8] * x + view[9] * y + view[10] * z,
            ]);
            let offset = 4 + index * 8;
            for axis in 0..3 {
                values[offset + axis] = light.color[axis] as f32;
                values[offset + 4 + axis] = toward[axis] as f32;
            }
            values[offset + 3] = if light.diffuse { 1.0 } else { 0.0 };
            values[offset + 7] = if light.specular { 1.0 } else { 0.0 };
        }
        values
    }
}

fn is_unit_color(color: [f64; 3]) -> bool {
    color
        .iter()
        .all(|component| component.is_finite() && (0.0..=1.0).contains(component))
}

fn normalize(vector: [f64; 3]) -> [f64; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    vector.map(|component| component / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_preset_is_valid() {
        neutral_preview_lighting().validate().expect("valid preset");
    }

    #[test]
    fn view_space_lights_follow_the_camera() {
        // A view matrix whose camera sits on -X looking toward +X.
        #[rustfmt::skip]
        let view = [
            0.0, 0.0, -1.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            1.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let preset = HsdLightingPreset {
            id: "test",
            ambient: [0.5; 3],
            lights: vec![HsdInfiniteLight {
                color: [1.0; 3],
                toward: [0.0, 0.0, 1.0],
                diffuse: true,
                specular: false,
            }],
        };
        let values = preset.uniforms(&view);
        // View +Z (toward the camera) is world -X.
        assert_eq!(&values[8..12], &[-1.0, 0.0, 0.0, 0.0]);
        assert_eq!(values[7], 1.0);
    }
}
