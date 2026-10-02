//! Source-defined TObj coordinate generation, independent of renderer policy.
//!
//! Pinned melee-90f83f6: `tobj.c` MakeTextureMtx (366–408),
//! TObjSetupMtx (424–434), setupTextureCoordGen (496–519), and
//! `GXAttr.c` GXSetTexCoordGen2 (465–542).
//! Matrices are row-major 3x4, unlike the scene's joint matrices. The admitted
//! ordinary path first generates (s,t,1) with identity MTX2x4, then applies this
//! postmatrix. All three output rows (including projective Q) are significant.
//! Rust f32 trig is not claimed bit-identical to the SDK's sinf/cosf.

use super::scene::HsdTransform;
use crate::descriptor::tobj::texture_flags;
use crate::math::Mat4;

// tobj.c spells this f32 value 1.00000001335e-10F, NOT machine epsilon.
const TOBJ_SCALE_EPSILON: f32 = 1.0e-10;
/// Actual input-row selection for GX's matrix texgen dispatch, not enum names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HsdTextureSource {
    TexCoord { index: u8 },
    Position,
    Normal,
    Binormal,
    Tangent,
    Color,
}

/// Dependencies not represented by the static texture contract, or invalid data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HsdTextureUnsupportedReason {
    NonFiniteTransform,
    ZeroRepeat,
    Hilight,
    Shadow,
    ToonRasterColor,
    EmbossLight,
    CoordinateMode,
    TexGenSource,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HsdTextureCoordinates {
    /// MTX2x4 identity texgen followed by this full postmatrix. A 2D texture
    /// attribute supplies (s,t,1,1); consumers interpolate S,T,Q before S/Q,T/Q.
    Matrix {
        source: HsdTextureSource,
        matrix: [[f32; 4]; 3],
    },
    /// Normalized camera-space normal texgen followed by this full postmatrix.
    /// Consumers interpolate S,T,Q before dividing S/Q,T/Q in the fragment.
    Reflection {
        matrix: [[f32; 4]; 3],
    },
    Unsupported {
        reason: HsdTextureUnsupportedReason,
    },
}

/// The lighting phases a TObj takes part in (`TEX_LIGHTMAP_*`): where
/// `MObjMakeTExp` applies its stage. A TObj with none of the first four is
/// not a TEV stage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HsdLightMap {
    pub diffuse: bool,
    pub specular: bool,
    pub ambient: bool,
    pub ext: bool,
    pub shadow: bool,
}

impl HsdLightMap {
    pub const NONE: Self = Self {
        diffuse: false,
        specular: false,
        ambient: false,
        ext: false,
        shadow: false,
    };
    pub const DIFFUSE: Self = Self {
        diffuse: true,
        ..Self::NONE
    };
    pub const SPECULAR: Self = Self {
        specular: true,
        ..Self::NONE
    };
    pub const AMBIENT: Self = Self {
        ambient: true,
        ..Self::NONE
    };
    pub const EXT: Self = Self {
        ext: true,
        ..Self::NONE
    };

    pub const fn from_flags(flags: u32) -> Self {
        Self {
            diffuse: flags & texture_flags::LIGHTMAP_DIFFUSE != 0,
            specular: flags & texture_flags::LIGHTMAP_SPECULAR != 0,
            ambient: flags & texture_flags::LIGHTMAP_AMBIENT != 0,
            ext: flags & texture_flags::LIGHTMAP_EXT != 0,
            shadow: flags & texture_flags::LIGHTMAP_SHADOW != 0,
        }
    }

    /// Whether `MObjMakeTExp` applies the TObj as a TEV stage: it is in the
    /// diffuse, specular, ambient or ext phase.
    pub const fn is_stage(self) -> bool {
        self.diffuse || self.specular || self.ambient || self.ext
    }
}

impl std::ops::BitOr for HsdLightMap {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self {
            diffuse: self.diffuse || other.diffuse,
            specular: self.specular || other.specular,
            ambient: self.ambient || other.ambient,
            ext: self.ext || other.ext,
            shadow: self.shadow || other.shadow,
        }
    }
}

/// How a TObj's stage combines with the running color (`TEX_COLORMAP_*`,
/// applied by `TObjMakeTExp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdColorMap {
    None,
    AlphaMask,
    RgbMask,
    Blend,
    Modulate,
    Replace,
    Pass,
    Add,
    Sub,
}

/// How a TObj's stage combines with the running alpha (`TEX_ALPHAMAP_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdAlphaMap {
    None,
    AlphaMask,
    Blend,
    Modulate,
    Replace,
    Pass,
    Add,
    Sub,
}

/// A TObj color or alpha map field holding a value `tobj.h` does not define.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("TObj {map} map {value} is not one HSD defines")]
pub struct HsdUndefinedTextureMap {
    /// `"color"` or `"alpha"`.
    pub map: &'static str,
    pub value: u32,
}

impl HsdColorMap {
    pub const fn from_flags(flags: u32) -> Result<Self, HsdUndefinedTextureMap> {
        Ok(match flags & texture_flags::COLORMAP_MASK {
            texture_flags::COLORMAP_NONE => Self::None,
            texture_flags::COLORMAP_ALPHA_MASK => Self::AlphaMask,
            texture_flags::COLORMAP_RGB_MASK => Self::RgbMask,
            texture_flags::COLORMAP_BLEND => Self::Blend,
            texture_flags::COLORMAP_MODULATE => Self::Modulate,
            texture_flags::COLORMAP_REPLACE => Self::Replace,
            texture_flags::COLORMAP_PASS => Self::Pass,
            texture_flags::COLORMAP_ADD => Self::Add,
            texture_flags::COLORMAP_SUB => Self::Sub,
            field => {
                return Err(HsdUndefinedTextureMap {
                    map: "color",
                    value: field >> 16,
                });
            }
        })
    }
}

impl HsdAlphaMap {
    pub const fn from_flags(flags: u32) -> Result<Self, HsdUndefinedTextureMap> {
        Ok(match flags & texture_flags::ALPHAMAP_MASK {
            texture_flags::ALPHAMAP_NONE => Self::None,
            texture_flags::ALPHAMAP_ALPHA_MASK => Self::AlphaMask,
            texture_flags::ALPHAMAP_BLEND => Self::Blend,
            texture_flags::ALPHAMAP_MODULATE => Self::Modulate,
            texture_flags::ALPHAMAP_REPLACE => Self::Replace,
            texture_flags::ALPHAMAP_PASS => Self::Pass,
            texture_flags::ALPHAMAP_ADD => Self::Add,
            texture_flags::ALPHAMAP_SUB => Self::Sub,
            field => {
                return Err(HsdUndefinedTextureMap {
                    map: "alpha",
                    value: field >> 20,
                });
            }
        })
    }
}

/// Resolve static ordinary and reflection TObj paths without heap allocation.
/// Unsupported dependencies stay explicit rather than becoming invented UV0.
pub fn resolve_texture_coordinates(
    tex_gen_src: u32,
    flags: u32,
    transform: &HsdTransform,
    repeat: [u8; 2],
    wrap_t: u32,
) -> HsdTextureCoordinates {
    use HsdTextureUnsupportedReason as Reason;
    let unsupported = |reason| HsdTextureCoordinates::Unsupported { reason };
    if flags & texture_flags::BUMP != 0 {
        // Emboss consumes an assigned generated coordinate plus an active light
        // and NBT basis, not simply the descriptor's raw TEX attribute index.
        return unsupported(Reason::EmbossLight);
    }
    let reason = match texture_flags::coord(flags) {
        texture_flags::COORD_UV | texture_flags::COORD_REFLECTION => None,
        texture_flags::COORD_HILIGHT => Some(Reason::Hilight),
        texture_flags::COORD_SHADOW => Some(Reason::Shadow),
        texture_flags::COORD_TOON => Some(Reason::ToonRasterColor),
        _ => Some(Reason::CoordinateMode),
    };
    if let Some(reason) = reason {
        return unsupported(reason);
    }
    let reflection = texture_flags::coord(flags) == texture_flags::COORD_REFLECTION;
    // Reflection hardwires GX_TG_NRM; the descriptor selector is ignored even
    // when it would be invalid for ordinary MTX2x4 dispatch (tobj.c:508–509).
    let source = match if reflection { 1 } else { tex_gen_src } {
        0 => HsdTextureSource::Position,
        1 => HsdTextureSource::Normal,
        2 => HsdTextureSource::Binormal,
        3 => HsdTextureSource::Tangent,
        4..=11 => HsdTextureSource::TexCoord {
            index: (tex_gen_src - 4) as u8,
        },
        // GX_TG_TEXCOORD0..6 only select generated output in BUMP dispatch.
        // GXAttr.c leaves row=5/form=0 for these enums in MTX2x4 dispatch.
        // In particular enum13 does NOT select raw TEX1 (enum5).
        12..=18 => HsdTextureSource::TexCoord { index: 0 },
        19 | 20 => HsdTextureSource::Color,
        _ => return unsupported(Reason::TexGenSource),
    };
    if repeat.contains(&0) {
        // MakeTextureMtx asserts repeat_s && repeat_t before doing arithmetic.
        return unsupported(Reason::ZeroRepeat);
    }
    if transform
        .scale
        .iter()
        .chain(&transform.rotation)
        .chain(&transform.translation)
        .any(|value| !value.is_finite())
    {
        return unsupported(Reason::NonFiniteTransform);
    }
    let [sx, sy, sz] = transform.scale;
    let scale = [
        if sx.abs() < TOBJ_SCALE_EPSILON {
            0.0
        } else {
            f32::from(repeat[0]) / sx
        },
        if sy.abs() < TOBJ_SCALE_EPSILON {
            0.0
        } else {
            f32::from(repeat[1]) / sy
        },
        sz,
    ];
    // Mirror offset uses the original scale, before the strict epsilon clamp.
    let mirror_offset = if wrap_t == 2 {
        1.0 / (f32::from(repeat[1]) / sy)
    } else {
        0.0
    };
    let translation = [
        -transform.translation[0],
        -(transform.translation[1] + mirror_offset),
        transform.translation[2],
    ];
    let [rx, ry, rz] = transform.rotation;
    // HSD_MkRotationMtx is Rz*Ry*Rx (mtx.c:325–357), exactly the rotation
    // convention of the existing primitive. TObj negates only rotation.z.
    let rotation = Mat4::from_srt([1.0; 3], [rx, ry, -rz], [0.0; 3]);
    let mut matrix = [[0.0; 4]; 3];
    for (row, matrix_row) in matrix.iter_mut().enumerate() {
        for (element, rotation_column) in matrix_row[..3].iter_mut().zip(&rotation.0) {
            *element = scale[row] * rotation_column[row];
        }
        // Preserve S*(R*T), including translation.z, rather than a 2D SRT.
        matrix_row[3] = scale[row]
            * (rotation.0[0][row] * translation[0]
                + rotation.0[1][row] * translation[1]
                + rotation.0[2][row] * translation[2]);
    }
    if matrix.iter().flatten().any(|value| !value.is_finite()) {
        return unsupported(Reason::NonFiniteTransform);
    }
    if reflection {
        // TObjSetupMtx's reflection postmatrix follows MakeTextureMtx. Keep
        // the source addition order and all three rows, including homogeneous Q.
        for row in &mut matrix {
            *row = [
                0.5 * row[0],
                -0.5 * row[1],
                0.0,
                0.5 * row[0] + 0.5 * row[1] + row[2] + row[3],
            ];
        }
        if matrix.iter().flatten().any(|value| !value.is_finite()) {
            return unsupported(Reason::NonFiniteTransform);
        }
        return HsdTextureCoordinates::Reflection { matrix };
    }
    HsdTextureCoordinates::Matrix { source, matrix }
}

#[cfg(test)]
mod tests {
    use super::{HsdAlphaMap, HsdColorMap, HsdLightMap, HsdUndefinedTextureMap};

    /// Source words from `tobj.h`, not the resolver's own masks.
    #[test]
    fn tobj_flags_resolve_to_their_light_color_and_alpha_maps() {
        // LIGHTMAP_DIFFUSE | LIGHTMAP_EXT, COLORMAP_MODULATE, ALPHAMAP_REPLACE.
        let flags = 0x0044_0090;
        assert_eq!(
            HsdLightMap::from_flags(flags),
            HsdLightMap::DIFFUSE | HsdLightMap::EXT
        );
        assert_eq!(HsdColorMap::from_flags(flags), Ok(HsdColorMap::Modulate));
        assert_eq!(HsdAlphaMap::from_flags(flags), Ok(HsdAlphaMap::Replace));

        // A shadow map alone is not a TEV stage.
        assert!(HsdLightMap::from_flags(0x10).is_stage());
        assert!(!HsdLightMap::from_flags(0x100).is_stage());

        // The fields are wider than the values the header defines.
        assert_eq!(
            HsdColorMap::from_flags(0x0009_0000),
            Err(HsdUndefinedTextureMap {
                map: "color",
                value: 9
            })
        );
        assert_eq!(
            HsdAlphaMap::from_flags(0x0080_0000),
            Err(HsdUndefinedTextureMap {
                map: "alpha",
                value: 8
            })
        );
    }
}
