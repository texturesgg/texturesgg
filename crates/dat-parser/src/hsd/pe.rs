//! Resolved pixel-engine semantics for standard HSD materials and custom PEDescs.

use super::scene::HsdMaterial;
use crate::descriptor::mobj::{PEDesc, render_flags};

/// GX blend factor, already resolved for the side it appears on: GX numbers a
/// source factor's color terms as the destination color and a destination
/// factor's as the source color (`GX_BL_DSTCLR` and `GX_BL_SRCCLR` share 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdBlendFactor {
    Zero,
    One,
    SourceColor,
    InverseSourceColor,
    DestinationColor,
    InverseDestinationColor,
    SourceAlpha,
    InverseSourceAlpha,
    DestinationAlpha,
    InverseDestinationAlpha,
}

impl HsdBlendFactor {
    const fn source(value: u8) -> Option<Self> {
        Some(match value {
            2 => Self::DestinationColor,
            3 => Self::InverseDestinationColor,
            _ => return Self::alpha_or_constant(value),
        })
    }

    const fn destination(value: u8) -> Option<Self> {
        Some(match value {
            2 => Self::SourceColor,
            3 => Self::InverseSourceColor,
            _ => return Self::alpha_or_constant(value),
        })
    }

    const fn alpha_or_constant(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Zero,
            1 => Self::One,
            4 => Self::SourceAlpha,
            5 => Self::InverseSourceAlpha,
            6 => Self::DestinationAlpha,
            7 => Self::InverseDestinationAlpha,
            _ => return None,
        })
    }
}

/// GX blend equation (`GXSetBlendMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdBlendMode {
    None,
    /// `source * src + destination * dst`.
    Blend {
        source: HsdBlendFactor,
        destination: HsdBlendFactor,
    },
    /// `destination - source`; GX ignores the factors.
    Subtract,
}

impl HsdBlendMode {
    /// The standard translucent blend every null-PEDesc XLU material uses.
    pub const SOURCE_ALPHA: Self = Self::Blend {
        source: HsdBlendFactor::SourceAlpha,
        destination: HsdBlendFactor::InverseSourceAlpha,
    };
}

/// GX comparison function, in `GXCompare` order, for depth and alpha tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdCompare {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

impl HsdCompare {
    const fn from_gx(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Never,
            1 => Self::Less,
            2 => Self::Equal,
            3 => Self::LessEqual,
            4 => Self::Greater,
            5 => Self::NotEqual,
            6 => Self::GreaterEqual,
            7 => Self::Always,
            _ => return None,
        })
    }
}

/// How GX combines the two alpha comparisons (`GXAlphaOp`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HsdAlphaOp {
    And,
    Or,
    Xor,
    Xnor,
}

/// GX alpha test (`GXSetAlphaCompare`): each comparison tests the fragment's
/// 8-bit alpha against its reference, and the op combines the two results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HsdAlphaCompare {
    pub first: HsdCompare,
    pub first_reference: u8,
    pub op: HsdAlphaOp,
    pub second: HsdCompare,
    pub second_reference: u8,
}

impl HsdAlphaCompare {
    /// Both comparisons ALWAYS, combined with AND.
    pub const ALWAYS: Self = Self::both(HsdCompare::Always);
    /// Both comparisons GREATER than reference zero, combined with AND.
    pub const GREATER_ZERO: Self = Self::both(HsdCompare::Greater);

    const fn both(compare: HsdCompare) -> Self {
        Self {
            first: compare,
            first_reference: 0,
            op: HsdAlphaOp::And,
            second: compare,
            second_reference: 0,
        }
    }

    /// Whether an 8-bit alpha passes the test, exactly as GX evaluates it.
    pub const fn passes(&self, alpha: u8) -> bool {
        let first = compare(self.first, alpha, self.first_reference);
        let second = compare(self.second, alpha, self.second_reference);
        match self.op {
            HsdAlphaOp::And => first && second,
            HsdAlphaOp::Or => first || second,
            HsdAlphaOp::Xor => first != second,
            HsdAlphaOp::Xnor => first == second,
        }
    }

    /// Whether every alpha passes, so the test can be skipped.
    pub fn always_passes(&self) -> bool {
        (0..=u8::MAX).all(|alpha| self.passes(alpha))
    }
}

const fn compare(function: HsdCompare, value: u8, reference: u8) -> bool {
    match function {
        HsdCompare::Never => false,
        HsdCompare::Less => value < reference,
        HsdCompare::Equal => value == reference,
        HsdCompare::LessEqual => value <= reference,
        HsdCompare::Greater => value > reference,
        HsdCompare::NotEqual => value != reference,
        HsdCompare::GreaterEqual => value >= reference,
        HsdCompare::Always => true,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HsdDepthCompareLocation {
    BeforeTexturing,
    AfterTexturing,
}

/// Complete PE state for a standard or custom-PEDesc HSD material.
///
/// This is source state, not a backend pipeline or a claim that a backend can
/// reproduce every ordering effect of the source depth-compare location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HsdPixelEngineState {
    pub color_update: bool,
    pub alpha_update: bool,
    pub destination_alpha: Option<u8>,
    pub blend: HsdBlendMode,
    /// GX disables the depth test and the depth write together.
    pub depth_test: bool,
    pub depth_compare: HsdCompare,
    pub depth_write: bool,
    pub depth_compare_location: HsdDepthCompareLocation,
    pub alpha_compare: HsdAlphaCompare,
    pub dither: bool,
}

impl HsdPixelEngineState {
    /// Resolve the null-PEDesc branch of `HSD_SetupPEMode`.
    ///
    /// Material alpha and non-PE render flags do not select PE state.
    pub const fn from_render_flags(flags: u32) -> Self {
        let translucent = flags & render_flags::XLU != 0;
        let depth_write = flags & render_flags::NO_ZUPDATE == 0;
        let compare_alpha = translucent && depth_write;
        Self {
            color_update: true,
            alpha_update: false,
            destination_alpha: None,
            blend: if translucent {
                HsdBlendMode::SOURCE_ALPHA
            } else {
                HsdBlendMode::None
            },
            depth_test: true,
            depth_compare: if flags & render_flags::ZMODE_ALWAYS != 0 {
                HsdCompare::Always
            } else {
                HsdCompare::LessEqual
            },
            depth_write,
            depth_compare_location: if compare_alpha {
                HsdDepthCompareLocation::AfterTexturing
            } else {
                HsdDepthCompareLocation::BeforeTexturing
            },
            alpha_compare: if compare_alpha {
                HsdAlphaCompare::GREATER_ZERO
            } else {
                HsdAlphaCompare::ALWAYS
            },
            dither: false,
        }
    }

    /// Resolve the custom-PEDesc branch of `HSD_SetupPEMode` (state.c:205).
    ///
    /// `None` when the descriptor holds a value outside its GX enum, or a
    /// logic op other than COPY: wgpu has no framebuffer logic ops.
    pub const fn from_descriptor(descriptor: &PEDesc) -> Option<Self> {
        const COLOR_UPDATE: u8 = 0x01;
        const ALPHA_UPDATE: u8 = 0x02;
        const DESTINATION_ALPHA: u8 = 0x04;
        const COMPARE_BEFORE_TEXTURING: u8 = 0x08;
        const DEPTH_TEST: u8 = 0x10;
        const DEPTH_WRITE: u8 = 0x20;
        const DITHER: u8 = 0x40;
        const LOGIC_COPY: u8 = 3;
        let flags = descriptor.flags;
        let blend = match descriptor.blend_mode {
            0 => HsdBlendMode::None,
            1 => {
                let (Some(source), Some(destination)) = (
                    HsdBlendFactor::source(descriptor.src_factor),
                    HsdBlendFactor::destination(descriptor.dst_factor),
                ) else {
                    return None;
                };
                HsdBlendMode::Blend {
                    source,
                    destination,
                }
            }
            2 if descriptor.logic_op == LOGIC_COPY => HsdBlendMode::None,
            3 => HsdBlendMode::Subtract,
            _ => return None,
        };
        let op = match descriptor.alpha_op {
            0 => HsdAlphaOp::And,
            1 => HsdAlphaOp::Or,
            2 => HsdAlphaOp::Xor,
            3 => HsdAlphaOp::Xnor,
            _ => return None,
        };
        let (Some(depth_compare), Some(first), Some(second)) = (
            HsdCompare::from_gx(descriptor.z_compare),
            HsdCompare::from_gx(descriptor.alpha_compare0),
            HsdCompare::from_gx(descriptor.alpha_compare1),
        ) else {
            return None;
        };
        Some(Self {
            color_update: flags & COLOR_UPDATE != 0,
            alpha_update: flags & ALPHA_UPDATE != 0,
            destination_alpha: if flags & DESTINATION_ALPHA != 0 {
                Some(descriptor.dst_alpha)
            } else {
                None
            },
            blend,
            depth_test: flags & DEPTH_TEST != 0,
            depth_compare,
            depth_write: flags & DEPTH_WRITE != 0,
            depth_compare_location: if flags & COMPARE_BEFORE_TEXTURING != 0 {
                HsdDepthCompareLocation::BeforeTexturing
            } else {
                HsdDepthCompareLocation::AfterTexturing
            },
            alpha_compare: HsdAlphaCompare {
                first,
                first_reference: descriptor.ref0,
                op,
                second,
                second_reference: descriptor.ref1,
            },
            dither: flags & DITHER != 0,
        })
    }
}

/// Display pass that draws a DObj, in the source's pass order: every opaque
/// DObj, then every edge-cutout DObj, then every translucent DObj.
///
/// `HSD_GObj_804085F0` (gobj.c:31) maps the render passes to `HSD_TRSP_OPA`,
/// `HSD_TRSP_TEXEDGE`, then `HSD_TRSP_XLU`, and each pass visits the whole
/// scene before the next one starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HsdDrawPass {
    Opaque,
    TexEdge,
    Translucent,
}

impl HsdDrawPass {
    /// `DObjLoad` (dobj.c:182-197) classifies the MObj render mode. XLU alone
    /// selects TEXEDGE; only XLU with NO_ZUPDATE selects XLU. NO_ZUPDATE alone
    /// makes the source panic, so it has no pass.
    pub const fn from_render_flags(flags: u32) -> Option<Self> {
        match flags & render_flags::BLENDING {
            0 => Some(Self::Opaque),
            render_flags::XLU => Some(Self::TexEdge),
            render_flags::BLENDING => Some(Self::Translucent),
            _ => None,
        }
    }
}

impl HsdMaterial {
    /// Resolve current material PE state without caching or changing provenance.
    pub fn pixel_engine_state(&self) -> HsdPixelEngineState {
        match self.custom_pe {
            None => HsdPixelEngineState::from_render_flags(self.render_flags),
            Some(custom_pe) => custom_pe.state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsd::scene::{HsdCustomPe, MObjId, PeDescId};

    #[test]
    fn draw_pass_follows_dobj_load_classification() {
        // Source words from DObjLoad's switch on rendermode & 0x60000000.
        assert_eq!(
            HsdDrawPass::from_render_flags(0x0000_0000),
            Some(HsdDrawPass::Opaque)
        );
        assert_eq!(
            HsdDrawPass::from_render_flags(0x4000_0000),
            Some(HsdDrawPass::TexEdge)
        );
        assert_eq!(
            HsdDrawPass::from_render_flags(0x6000_0000),
            Some(HsdDrawPass::Translucent)
        );
        assert_eq!(HsdDrawPass::from_render_flags(0x2000_0000), None);
        // Unrelated render-mode bits do not select a pass.
        assert_eq!(
            HsdDrawPass::from_render_flags(0x9FFF_FFFF),
            Some(HsdDrawPass::Opaque)
        );
        assert!(HsdDrawPass::Opaque < HsdDrawPass::TexEdge);
        assert!(HsdDrawPass::TexEdge < HsdDrawPass::Translucent);
    }

    #[test]
    fn standard_pe_resolves_all_render_flag_combinations() {
        // Independent source words and expected states from HSD_SetupPEMode;
        // do not construct expectations with the resolver's flag masks.
        let cases = [
            (
                0x0000_0000,
                HsdBlendMode::None,
                HsdCompare::LessEqual,
                true,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
            (
                0x0800_0000,
                HsdBlendMode::None,
                HsdCompare::Always,
                true,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
            (
                0x2000_0000,
                HsdBlendMode::None,
                HsdCompare::LessEqual,
                false,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
            (
                0x2800_0000,
                HsdBlendMode::None,
                HsdCompare::Always,
                false,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
            (
                0x4000_0000,
                HsdBlendMode::SOURCE_ALPHA,
                HsdCompare::LessEqual,
                true,
                HsdDepthCompareLocation::AfterTexturing,
                HsdAlphaCompare::GREATER_ZERO,
            ),
            (
                0x4800_0000,
                HsdBlendMode::SOURCE_ALPHA,
                HsdCompare::Always,
                true,
                HsdDepthCompareLocation::AfterTexturing,
                HsdAlphaCompare::GREATER_ZERO,
            ),
            (
                0x6000_0000,
                HsdBlendMode::SOURCE_ALPHA,
                HsdCompare::LessEqual,
                false,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
            (
                0x6800_0000,
                HsdBlendMode::SOURCE_ALPHA,
                HsdCompare::Always,
                false,
                HsdDepthCompareLocation::BeforeTexturing,
                HsdAlphaCompare::ALWAYS,
            ),
        ];
        for (flags, blend, depth_compare, depth_write, depth_compare_location, alpha_compare) in
            cases
        {
            assert_eq!(
                HsdPixelEngineState::from_render_flags(flags),
                HsdPixelEngineState {
                    color_update: true,
                    alpha_update: false,
                    destination_alpha: None,
                    blend,
                    depth_test: true,
                    depth_compare,
                    depth_write,
                    depth_compare_location,
                    alpha_compare,
                    dither: false,
                },
                "render flags {flags:#010x}",
            );
        }
    }

    #[test]
    fn admitted_custom_pe_overrides_conflicting_standard_state() {
        let material = HsdMaterial {
            source_id: MObjId(16),
            // Standard state would write depth with ALWAYS comparison and
            // reject zero alpha after texturing; the custom mode overrides all.
            render_flags: render_flags::XLU | render_flags::ZMODE_ALWAYS,
            custom_pe: Some(HsdCustomPe {
                source_id: PeDescId(64),
                state: crate::hsd::pe::HsdPixelEngineState::from_descriptor(
                    &crate::hsd::scene::ADMITTED_CUSTOM_PE,
                )
                .unwrap(),
            }),
            colors: None,
            textures: Vec::new(),
        };
        assert_eq!(
            material.pixel_engine_state(),
            HsdPixelEngineState {
                color_update: true,
                alpha_update: false,
                destination_alpha: None,
                blend: HsdBlendMode::SOURCE_ALPHA,
                depth_test: true,
                depth_compare: HsdCompare::LessEqual,
                depth_write: false,
                depth_compare_location: HsdDepthCompareLocation::BeforeTexturing,
                alpha_compare: HsdAlphaCompare::ALWAYS,
                dither: false,
            },
        );
    }
}
