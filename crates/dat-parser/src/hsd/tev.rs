//! The per-TObj custom TEV expressions a scene admits.
//!
//! HSD builds a color and an alpha expression independently after a shared
//! selector pre-scan, each `(A * (1 - C) + B * C) + D`, clamped. This module
//! validates a descriptor against the subset stock files use (ADD, zero bias,
//! scale one, clamped, no TEV1) and names each side's four inputs. A renderer
//! builds the expression from them; nothing here samples a texture.

use crate::descriptor::tobj::{
    TObjTevDesc, tev_active, tev_alpha_input, tev_bias, tev_color_input, tev_op, tev_scale,
};
use thiserror::Error;

const TEV1_ACTIVE_MASK: u32 =
    tev_active::TEV1_R | tev_active::TEV1_G | tev_active::TEV1_B | tev_active::TEV1_A;

/// The independently gated custom expression side.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HsdTObjTevSide {
    Color,
    Alpha,
}

/// The descriptor's constant and TEV0 registers. TEV1 is absent because the
/// admitted subset never selects it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsdTObjTevRegisters {
    pub konst: [u8; 4],
    pub tev0: [u8; 4],
}

impl HsdTObjTevRegisters {
    pub fn from_descriptor(descriptor: &TObjTevDesc) -> Self {
        Self {
            konst: descriptor.konst,
            tev0: descriptor.tev0,
        }
    }
}

/// Validated expression program for the admitted custom TObj TEV subset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HsdTObjTevProgram {
    color_inputs: Option<[HsdTObjTevColorInput; 4]>,
    alpha_inputs: Option<[HsdTObjTevAlphaInput; 4]>,
}

impl HsdTObjTevProgram {
    /// Validate a parsed descriptor in matching setup order.
    ///
    /// Unknown active bits are malformed and declared TEV1 component bits and
    /// selectors are explicit unsupported boundaries only after the outer custom
    /// expression gate is active. If neither high enable bit is set, matching
    /// setup does not inspect custom fields or lower active bits. Otherwise it
    /// pre-scans both arrays before dispatching and validating only enabled
    /// sides; this distinction is preserved here.
    pub fn validate(descriptor: &TObjTevDesc) -> Result<Self, HsdTObjTevEvaluationError> {
        let color_enabled = descriptor.color_tev_active();
        let alpha_enabled = descriptor.alpha_tev_active();
        if !color_enabled && !alpha_enabled {
            return Ok(Self {
                color_inputs: None,
                alpha_inputs: None,
            });
        }

        let unknown_bits = descriptor.active & !tev_active::DECLARED_MASK;
        if unknown_bits != 0 {
            return Err(HsdTObjTevEvaluationError::MalformedActiveMask {
                active: descriptor.active,
                unknown_bits,
            });
        }

        let tev1_bits = descriptor.active & TEV1_ACTIVE_MASK;
        if tev1_bits != 0 {
            return Err(HsdTObjTevEvaluationError::UnsupportedTev1ActiveMask { tev1_bits });
        }

        pre_scan_color_selectors(descriptor.color_inputs)?;
        pre_scan_alpha_selectors(descriptor.alpha_inputs)?;

        let color_inputs = color_enabled
            .then(|| validate_color_side(descriptor))
            .transpose()?;
        let alpha_inputs = alpha_enabled
            .then(|| validate_alpha_side(descriptor))
            .transpose()?;

        Ok(Self {
            color_inputs,
            alpha_inputs,
        })
    }

    pub fn color_enabled(&self) -> bool {
        self.color_inputs.is_some()
    }

    pub fn alpha_enabled(&self) -> bool {
        self.alpha_inputs.is_some()
    }

    pub fn color_inputs(&self) -> Option<[HsdTObjTevColorInput; 4]> {
        self.color_inputs
    }

    pub fn alpha_inputs(&self) -> Option<[HsdTObjTevAlphaInput; 4]> {
        self.alpha_inputs
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HsdTObjTevColorInput {
    Zero,
    One,
    Half,
    TextureRgb,
    TextureAlpha,
    KonstRgb,
    KonstComponent(usize),
    Tev0Rgb,
    Tev0Alpha,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HsdTObjTevAlphaInput {
    Zero,
    TextureAlpha,
    KonstComponent(usize),
    Tev0Alpha,
}

fn validate_color_side(
    descriptor: &TObjTevDesc,
) -> Result<[HsdTObjTevColorInput; 4], HsdTObjTevEvaluationError> {
    let inputs = [
        parse_color_input(0, descriptor.color_inputs[0])?,
        parse_color_input(1, descriptor.color_inputs[1])?,
        parse_color_input(2, descriptor.color_inputs[2])?,
        parse_color_input(3, descriptor.color_inputs[3])?,
    ];
    validate_operation(
        HsdTObjTevSide::Color,
        descriptor.color_op,
        descriptor.color_bias,
        descriptor.color_scale,
        descriptor.color_clamp,
    )?;
    Ok(inputs)
}

fn validate_alpha_side(
    descriptor: &TObjTevDesc,
) -> Result<[HsdTObjTevAlphaInput; 4], HsdTObjTevEvaluationError> {
    let inputs = [
        parse_alpha_input(0, descriptor.alpha_inputs[0])?,
        parse_alpha_input(1, descriptor.alpha_inputs[1])?,
        parse_alpha_input(2, descriptor.alpha_inputs[2])?,
        parse_alpha_input(3, descriptor.alpha_inputs[3])?,
    ];
    validate_operation(
        HsdTObjTevSide::Alpha,
        descriptor.alpha_op,
        descriptor.alpha_bias,
        descriptor.alpha_scale,
        descriptor.alpha_clamp,
    )?;
    Ok(inputs)
}

fn validate_operation(
    side: HsdTObjTevSide,
    operation: u8,
    bias: u8,
    scale: u8,
    clamp: u8,
) -> Result<(), HsdTObjTevEvaluationError> {
    if operation != tev_op::ADD {
        return Err(HsdTObjTevEvaluationError::UnsupportedOperation { side, operation });
    }
    if bias != tev_bias::ZERO {
        return Err(HsdTObjTevEvaluationError::UnsupportedBias { side, bias });
    }
    if scale != tev_scale::SCALE_1 {
        return Err(HsdTObjTevEvaluationError::UnsupportedScale { side, scale });
    }
    if clamp != 1 {
        return Err(HsdTObjTevEvaluationError::UnsupportedClamp { side, clamp });
    }
    Ok(())
}

fn pre_scan_color_selectors(selectors: [u8; 4]) -> Result<(), HsdTObjTevEvaluationError> {
    for (index, selector) in selectors.into_iter().enumerate() {
        if matches!(
            selector,
            tev_color_input::TEX1_RGB | tev_color_input::TEX1_AAA
        ) {
            return Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Color,
                index,
                selector,
            });
        }
    }
    Ok(())
}

fn pre_scan_alpha_selectors(selectors: [u8; 4]) -> Result<(), HsdTObjTevEvaluationError> {
    for (index, selector) in selectors.into_iter().enumerate() {
        if selector == tev_alpha_input::TEX1_A {
            return Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Alpha,
                index,
                selector,
            });
        }
    }
    Ok(())
}

fn parse_color_input(
    index: usize,
    selector: u8,
) -> Result<HsdTObjTevColorInput, HsdTObjTevEvaluationError> {
    let input = match selector {
        tev_color_input::ZERO => HsdTObjTevColorInput::Zero,
        tev_color_input::ONE => HsdTObjTevColorInput::One,
        tev_color_input::HALF => HsdTObjTevColorInput::Half,
        tev_color_input::TEXC => HsdTObjTevColorInput::TextureRgb,
        tev_color_input::TEXA => HsdTObjTevColorInput::TextureAlpha,
        tev_color_input::KONST_RGB => HsdTObjTevColorInput::KonstRgb,
        tev_color_input::KONST_RRR => HsdTObjTevColorInput::KonstComponent(0),
        tev_color_input::KONST_GGG => HsdTObjTevColorInput::KonstComponent(1),
        tev_color_input::KONST_BBB => HsdTObjTevColorInput::KonstComponent(2),
        tev_color_input::KONST_AAA => HsdTObjTevColorInput::KonstComponent(3),
        tev_color_input::TEX0_RGB => HsdTObjTevColorInput::Tev0Rgb,
        tev_color_input::TEX0_AAA => HsdTObjTevColorInput::Tev0Alpha,
        tev_color_input::TEX1_RGB | tev_color_input::TEX1_AAA => {
            return Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Color,
                index,
                selector,
            });
        }
        _ => {
            return Err(HsdTObjTevEvaluationError::UnsupportedSelector {
                side: HsdTObjTevSide::Color,
                index,
                selector,
            });
        }
    };
    Ok(input)
}

fn parse_alpha_input(
    index: usize,
    selector: u8,
) -> Result<HsdTObjTevAlphaInput, HsdTObjTevEvaluationError> {
    let input = match selector {
        tev_alpha_input::ZERO => HsdTObjTevAlphaInput::Zero,
        tev_alpha_input::TEXA => HsdTObjTevAlphaInput::TextureAlpha,
        tev_alpha_input::KONST_R => HsdTObjTevAlphaInput::KonstComponent(0),
        tev_alpha_input::KONST_G => HsdTObjTevAlphaInput::KonstComponent(1),
        tev_alpha_input::KONST_B => HsdTObjTevAlphaInput::KonstComponent(2),
        tev_alpha_input::KONST_A => HsdTObjTevAlphaInput::KonstComponent(3),
        tev_alpha_input::TEX0_A => HsdTObjTevAlphaInput::Tev0Alpha,
        tev_alpha_input::TEX1_A => {
            return Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Alpha,
                index,
                selector,
            });
        }
        _ => {
            return Err(HsdTObjTevEvaluationError::UnsupportedSelector {
                side: HsdTObjTevSide::Alpha,
                index,
                selector,
            });
        }
    };
    Ok(input)
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum HsdTObjTevEvaluationError {
    #[error("custom TObj TEV active mask {active:#010x} has unknown bits {unknown_bits:#010x}")]
    MalformedActiveMask { active: u32, unknown_bits: u32 },
    #[error("custom TObj TEV uses unsupported TEV1 active bits {tev1_bits:#05x}")]
    UnsupportedTev1ActiveMask { tev1_bits: u32 },
    #[error("custom TObj TEV {side:?} input {index} uses unsupported selector {selector:#04x}")]
    UnsupportedSelector {
        side: HsdTObjTevSide,
        index: usize,
        selector: u8,
    },
    #[error(
        "custom TObj TEV {side:?} input {index} uses unsupported TEV1 selector {selector:#04x}"
    )]
    UnsupportedTev1Selector {
        side: HsdTObjTevSide,
        index: usize,
        selector: u8,
    },
    #[error("custom TObj TEV {side:?} operation {operation} is outside the admitted ADD subset")]
    UnsupportedOperation { side: HsdTObjTevSide, operation: u8 },
    #[error("custom TObj TEV {side:?} bias {bias} is outside the admitted zero-bias subset")]
    UnsupportedBias { side: HsdTObjTevSide, bias: u8 },
    #[error("custom TObj TEV {side:?} scale {scale} is outside the admitted scale-one subset")]
    UnsupportedScale { side: HsdTObjTevSide, scale: u8 },
    #[error("custom TObj TEV {side:?} clamp byte {clamp} is outside the admitted clamped subset")]
    UnsupportedClamp { side: HsdTObjTevSide, clamp: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor(active: u32) -> TObjTevDesc {
        TObjTevDesc {
            color_op: tev_op::ADD,
            alpha_op: tev_op::ADD,
            color_bias: tev_bias::ZERO,
            alpha_bias: tev_bias::ZERO,
            color_scale: tev_scale::SCALE_1,
            alpha_scale: tev_scale::SCALE_1,
            color_clamp: 1,
            alpha_clamp: 1,
            color_inputs: [
                tev_color_input::TEX0_RGB,
                tev_color_input::KONST_RGB,
                tev_color_input::TEXC,
                tev_color_input::ZERO,
            ],
            alpha_inputs: [
                tev_alpha_input::TEX0_A,
                tev_alpha_input::KONST_A,
                tev_alpha_input::TEXA,
                tev_alpha_input::ZERO,
            ],
            konst: [255, 0, 128, 255],
            tev0: [0, 255, 64, 0],
            tev1: [0; 4],
            active,
        }
    }

    /// Each high enable bit turns on one side; a side that is off has no
    /// inputs and the texture passes through it.
    #[test]
    fn each_enable_bit_gates_its_own_side() {
        use HsdTObjTevAlphaInput as A;
        use HsdTObjTevColorInput as C;
        let color = Some([C::Tev0Rgb, C::KonstRgb, C::TextureRgb, C::Zero]);
        let alpha = Some([A::Tev0Alpha, A::KonstComponent(3), A::TextureAlpha, A::Zero]);
        for (active, expected) in [
            (tev_active::COLOR_TEV | 0x77, (color, None)),
            (tev_active::ALPHA_TEV | 0x7F, (None, alpha)),
            (
                tev_active::COLOR_TEV | tev_active::ALPHA_TEV | 0xFF,
                (color, alpha),
            ),
        ] {
            let program = HsdTObjTevProgram::validate(&descriptor(active)).expect("admitted");
            assert_eq!((program.color_inputs(), program.alpha_inputs()), expected);
            assert_eq!(program.color_enabled(), expected.0.is_some());
            assert_eq!(program.alpha_enabled(), expected.1.is_some());
        }
    }

    /// Every selector byte the subset admits, and the input it names.
    #[test]
    fn each_admitted_selector_names_its_input() {
        use HsdTObjTevAlphaInput as A;
        use HsdTObjTevColorInput as C;
        let color = [
            (tev_color_input::ZERO, C::Zero),
            (tev_color_input::ONE, C::One),
            (tev_color_input::HALF, C::Half),
            (tev_color_input::TEXC, C::TextureRgb),
            (tev_color_input::TEXA, C::TextureAlpha),
            (tev_color_input::KONST_RGB, C::KonstRgb),
            (tev_color_input::KONST_RRR, C::KonstComponent(0)),
            (tev_color_input::KONST_GGG, C::KonstComponent(1)),
            (tev_color_input::KONST_BBB, C::KonstComponent(2)),
            (tev_color_input::KONST_AAA, C::KonstComponent(3)),
            (tev_color_input::TEX0_RGB, C::Tev0Rgb),
            (tev_color_input::TEX0_AAA, C::Tev0Alpha),
        ];
        for (selector, input) in color {
            let mut desc = descriptor(tev_active::COLOR_TEV);
            desc.color_inputs = [selector; 4];
            let program = HsdTObjTevProgram::validate(&desc).expect("admitted color selector");
            assert_eq!(program.color_inputs(), Some([input; 4]), "{selector:#04x}");
        }
        let alpha = [
            (tev_alpha_input::ZERO, A::Zero),
            (tev_alpha_input::TEXA, A::TextureAlpha),
            (tev_alpha_input::KONST_R, A::KonstComponent(0)),
            (tev_alpha_input::KONST_G, A::KonstComponent(1)),
            (tev_alpha_input::KONST_B, A::KonstComponent(2)),
            (tev_alpha_input::KONST_A, A::KonstComponent(3)),
            (tev_alpha_input::TEX0_A, A::Tev0Alpha),
        ];
        for (selector, input) in alpha {
            let mut desc = descriptor(tev_active::ALPHA_TEV);
            desc.alpha_inputs = [selector; 4];
            let program = HsdTObjTevProgram::validate(&desc).expect("admitted alpha selector");
            assert_eq!(program.alpha_inputs(), Some([input; 4]), "{selector:#04x}");
        }
    }

    #[test]
    fn outer_gate_ignores_all_custom_fields_when_both_sides_are_disabled() {
        let mut desc = descriptor(tev_active::TEV1_R | (1 << 12));
        desc.color_op = 0xFF;
        desc.alpha_op = 0xFF;
        desc.color_bias = 0xFF;
        desc.alpha_scale = 0xFF;
        desc.color_clamp = 0;
        desc.alpha_clamp = 0;
        desc.color_inputs = [tev_color_input::TEX1_RGB; 4];
        desc.alpha_inputs = [tev_alpha_input::TEX1_A; 4];
        let program = HsdTObjTevProgram::validate(&desc).expect("inactive pass-through");
        assert_eq!(
            (program.color_inputs(), program.alpha_inputs()),
            (None, None)
        );
    }

    #[test]
    fn a_disabled_side_is_not_validated() {
        let mut desc = descriptor(tev_active::COLOR_TEV);
        desc.alpha_op = 0xFF;
        desc.alpha_bias = 0xFF;
        desc.alpha_scale = 0xFF;
        desc.alpha_clamp = 0;
        desc.alpha_inputs = [0xFE; 4];
        let program = HsdTObjTevProgram::validate(&desc).expect("disabled alpha side");
        assert!(program.color_enabled());
        assert_eq!(program.alpha_inputs(), None);
    }

    #[test]
    fn shared_pre_scan_rejects_tev1_on_a_disabled_side() {
        let mut desc = descriptor(tev_active::COLOR_TEV);
        desc.alpha_inputs[2] = tev_alpha_input::TEX1_A;
        assert_eq!(
            HsdTObjTevProgram::validate(&desc),
            Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Alpha,
                index: 2,
                selector: tev_alpha_input::TEX1_A,
            })
        );
    }

    #[test]
    fn rejects_unlisted_enabled_color_and_alpha_selectors() {
        let mut color = descriptor(tev_active::COLOR_TEV);
        color.color_inputs[1] = 0x7E;
        assert_eq!(
            HsdTObjTevProgram::validate(&color),
            Err(HsdTObjTevEvaluationError::UnsupportedSelector {
                side: HsdTObjTevSide::Color,
                index: 1,
                selector: 0x7E,
            })
        );

        let mut alpha = descriptor(tev_active::ALPHA_TEV);
        alpha.alpha_inputs[3] = 0x7E;
        assert_eq!(
            HsdTObjTevProgram::validate(&alpha),
            Err(HsdTObjTevEvaluationError::UnsupportedSelector {
                side: HsdTObjTevSide::Alpha,
                index: 3,
                selector: 0x7E,
            })
        );
    }

    #[test]
    fn rejects_tev1_selectors_and_declared_active_bits() {
        let mut color = descriptor(tev_active::COLOR_TEV);
        color.color_inputs[0] = tev_color_input::TEX1_AAA;
        assert!(matches!(
            HsdTObjTevProgram::validate(&color),
            Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Color,
                ..
            })
        ));

        let mut alpha = descriptor(tev_active::ALPHA_TEV);
        alpha.alpha_inputs[0] = tev_alpha_input::TEX1_A;
        assert!(matches!(
            HsdTObjTevProgram::validate(&alpha),
            Err(HsdTObjTevEvaluationError::UnsupportedTev1Selector {
                side: HsdTObjTevSide::Alpha,
                ..
            })
        ));

        let active = descriptor(tev_active::COLOR_TEV | tev_active::TEV1_R);
        assert_eq!(
            HsdTObjTevProgram::validate(&active),
            Err(HsdTObjTevEvaluationError::UnsupportedTev1ActiveMask {
                tev1_bits: tev_active::TEV1_R,
            })
        );
    }

    #[test]
    fn rejects_unknown_active_mask_bits() {
        let desc = descriptor(tev_active::COLOR_TEV | (1 << 12));
        assert_eq!(
            HsdTObjTevProgram::validate(&desc),
            Err(HsdTObjTevEvaluationError::MalformedActiveMask {
                active: tev_active::COLOR_TEV | (1 << 12),
                unknown_bits: 1 << 12,
            })
        );
    }

    #[test]
    fn rejects_subtraction_and_compare_operations_on_each_side() {
        for operation in [tev_op::SUB, tev_op::R8_GT] {
            let mut color = descriptor(tev_active::COLOR_TEV);
            color.color_op = operation;
            assert_eq!(
                HsdTObjTevProgram::validate(&color),
                Err(HsdTObjTevEvaluationError::UnsupportedOperation {
                    side: HsdTObjTevSide::Color,
                    operation,
                })
            );

            let mut alpha = descriptor(tev_active::ALPHA_TEV);
            alpha.alpha_op = operation;
            assert_eq!(
                HsdTObjTevProgram::validate(&alpha),
                Err(HsdTObjTevEvaluationError::UnsupportedOperation {
                    side: HsdTObjTevSide::Alpha,
                    operation,
                })
            );
        }
    }

    #[test]
    fn rejects_each_nonzero_bias_on_each_side() {
        for bias in [tev_bias::ADD_HALF, tev_bias::SUB_HALF] {
            let mut color = descriptor(tev_active::COLOR_TEV);
            color.color_bias = bias;
            assert_eq!(
                HsdTObjTevProgram::validate(&color),
                Err(HsdTObjTevEvaluationError::UnsupportedBias {
                    side: HsdTObjTevSide::Color,
                    bias,
                })
            );

            let mut alpha = descriptor(tev_active::ALPHA_TEV);
            alpha.alpha_bias = bias;
            assert_eq!(
                HsdTObjTevProgram::validate(&alpha),
                Err(HsdTObjTevEvaluationError::UnsupportedBias {
                    side: HsdTObjTevSide::Alpha,
                    bias,
                })
            );
        }
    }

    #[test]
    fn rejects_each_alternate_scale_on_each_side() {
        for scale in [tev_scale::SCALE_2, tev_scale::SCALE_4, tev_scale::DIVIDE_2] {
            let mut color = descriptor(tev_active::COLOR_TEV);
            color.color_scale = scale;
            assert_eq!(
                HsdTObjTevProgram::validate(&color),
                Err(HsdTObjTevEvaluationError::UnsupportedScale {
                    side: HsdTObjTevSide::Color,
                    scale,
                })
            );

            let mut alpha = descriptor(tev_active::ALPHA_TEV);
            alpha.alpha_scale = scale;
            assert_eq!(
                HsdTObjTevProgram::validate(&alpha),
                Err(HsdTObjTevEvaluationError::UnsupportedScale {
                    side: HsdTObjTevSide::Alpha,
                    scale,
                })
            );
        }
    }

    #[test]
    fn rejects_unclamped_color_and_alpha_sides() {
        let mut color = descriptor(tev_active::COLOR_TEV);
        color.color_clamp = 0;
        assert_eq!(
            HsdTObjTevProgram::validate(&color),
            Err(HsdTObjTevEvaluationError::UnsupportedClamp {
                side: HsdTObjTevSide::Color,
                clamp: 0,
            })
        );

        let mut alpha = descriptor(tev_active::ALPHA_TEV);
        alpha.alpha_clamp = 0;
        assert_eq!(
            HsdTObjTevProgram::validate(&alpha),
            Err(HsdTObjTevEvaluationError::UnsupportedClamp {
                side: HsdTObjTevSide::Alpha,
                clamp: 0,
            })
        );
    }
}
