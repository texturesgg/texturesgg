//! Resolved GX color-channel usage for standard HSD materials.
//!
//! `HSD_SetupChannelMode` (state.c:143) configures the lighting channels from
//! the MObj render mode, and `MObjMakeTExp` (mobj.c:190) decides how TEV
//! combines them with the material colors. This module resolves both into the
//! small set of facts a renderer needs; it does not choose lights.

use crate::descriptor::mobj::render_flags;

/// Color that TEV starts from before any light-map texture stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HsdChannelBase {
    /// The material's diffuse color and alpha constants.
    Material,
    /// Rasterized vertex COLOR0 and its alpha, passed through unlit.
    Vertex,
    /// Rasterized COLOR0 from a disabled channel with a white register.
    White,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HsdColorChannelState {
    pub base: HsdChannelBase,
    /// Multiply the base by lit COLOR0: ambient plus clamped diffuse lights.
    /// Lit alpha stays one because no admitted scene enables alpha lights.
    pub diffuse_lighting: bool,
    /// Add the material specular color times lit COLOR1.
    pub specular_lighting: bool,
}

impl HsdColorChannelState {
    /// Resolve `rendermode`'s channel field exactly as the source does.
    ///
    /// `HSD_SetupChannelMode` switches on `rendermode & 7`: VERTEX alone passes
    /// vertex color through COLOR0A0, DIFFUSE alone lights COLOR0 with a white
    /// material register, and every other value disables COLOR0A0 with a white
    /// register. `MObjMakeTExp` starts from rasterized COLOR0A0 when VERTEX is
    /// set and from the material constants otherwise, multiplies by it again
    /// when DIFFUSE is set, and adds specular when SPECULAR is set.
    pub const fn from_render_flags(flags: u32) -> Self {
        let channel_mode =
            flags & (render_flags::CONSTANT | render_flags::VERTEX | render_flags::DIFFUSE);
        Self {
            base: if flags & render_flags::VERTEX == 0 {
                HsdChannelBase::Material
            } else if channel_mode == render_flags::VERTEX {
                HsdChannelBase::Vertex
            } else {
                HsdChannelBase::White
            },
            diffuse_lighting: channel_mode == render_flags::DIFFUSE,
            specular_lighting: flags & render_flags::SPECULAR != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_field_matches_setup_channel_mode_and_make_texp() {
        use HsdChannelBase::{Material, Vertex, White};
        // Every value of the four channel bits, written as source words.
        let cases = [
            (0x0, Material, false, false),
            (0x1, Material, false, false),
            (0x2, Vertex, false, false),
            (0x3, White, false, false),
            (0x4, Material, true, false),
            (0x5, Material, false, false),
            (0x6, White, false, false),
            (0x7, White, false, false),
            (0x8, Material, false, true),
            (0x9, Material, false, true),
            (0xA, Vertex, false, true),
            (0xB, White, false, true),
            (0xC, Material, true, true),
            (0xD, Material, false, true),
            (0xE, White, false, true),
            (0xF, White, false, true),
        ];
        for (flags, base, diffuse_lighting, specular_lighting) in cases {
            assert_eq!(
                HsdColorChannelState::from_render_flags(flags | 0x6000_0000),
                HsdColorChannelState {
                    base,
                    diffuse_lighting,
                    specular_lighting
                },
                "render flags {flags:#x}"
            );
        }
    }
}
