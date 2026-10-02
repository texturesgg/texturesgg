//! Source-reasoned vectors for melee-90f83f6 tobj.c:366–434, 506–509 and
//! mtx.c:325–357. Expected values use simple rotations, not a second SRT engine.
use dat_parser::descriptor::tobj::texture_flags;
use dat_parser::hsd::scene::HsdTransform;
use dat_parser::hsd::texture::{
    HsdTextureCoordinates, HsdTextureSource, HsdTextureUnsupportedReason,
    resolve_texture_coordinates,
};

fn identity() -> HsdTransform {
    HsdTransform {
        scale: [1.0; 3],
        rotation: [0.0; 3],
        translation: [0.0; 3],
    }
}

fn matrix(coordinates: HsdTextureCoordinates) -> [[f32; 4]; 3] {
    let HsdTextureCoordinates::Matrix { matrix, .. } = coordinates else {
        panic!("expected matrix, got {coordinates:?}");
    };
    matrix
}

fn reflection_matrix(coordinates: HsdTextureCoordinates) -> [[f32; 4]; 3] {
    let HsdTextureCoordinates::Reflection { matrix } = coordinates else {
        panic!("expected reflection, got {coordinates:?}");
    };
    matrix
}

fn assert_matrix(actual: [[f32; 4]; 3], expected: [[f32; 4]; 3]) {
    for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
        assert!((actual - expected).abs() < 1.0e-6, "{actual} != {expected}");
    }
}

#[test]
fn source_epsilon_is_strict_and_not_machine_epsilon() {
    let mut transform = identity();
    transform.scale[0] = 1.0e-9;
    let resolved = resolve_texture_coordinates(4, 0, &transform, [1, 1], 0);
    assert_eq!(matrix(resolved)[0][0], 1.0e9);

    // tobj.c locally defines FLT_EPSILON as this f32 value. Equality is admitted;
    // exactly the preceding float is clamped. This catches both wrong constants
    // and a <= comparison, including the negative-scale branch.
    let epsilon = 1.0e-10_f32;
    for (scale, expected) in [
        (epsilon, 1.0e10),
        (-epsilon, -1.0e10),
        (f32::from_bits(epsilon.to_bits() - 1), 0.0),
    ] {
        transform.scale[0] = scale;
        assert_eq!(
            matrix(resolve_texture_coordinates(4, 0, &transform, [1, 1], 0))[0][0],
            expected
        );
    }
}

#[test]
fn mirrored_v_offset_precedes_epsilon_clamp_and_rotation() {
    let mut transform = identity();
    transform.scale[1] = 0.5e-10;
    transform.rotation[2] = std::f32::consts::FRAC_PI_2;
    let actual = matrix(resolve_texture_coordinates(4, 0, &transform, [1, 1], 2));
    // V's reciprocal scale is clamped to zero, but its raw mirror offset still
    // rotates into U: Rz(-pi/2) * (0,-scale.y,0) = (-scale.y,0,0).
    assert_eq!(actual[1], [0.0; 4]);
    assert!((actual[0][3] + transform.scale[1]).abs() < 1.0e-17);
}

#[test]
fn full_xyz_rotation_and_positive_translation_z_survive_s_r_t() {
    let transform = HsdTransform {
        scale: [2.0, 4.0, 3.0],
        rotation: [std::f32::consts::FRAC_PI_2; 3],
        translation: [0.25, 0.5, 2.0],
    };
    // repeat/scale=(1,2,3); mirrored V adds 1/(8/4)=.5 to ty.
    // Rz(-90)*Ry(90)*Rx(90) sends X→-Z, Y→-Y, Z→-X.
    // Therefore T=(-.25,-1,+2), R*T=(-2,+1,+.25), then scale rows.
    let resolved = resolve_texture_coordinates(4, 0, &transform, [2, 8], 2);
    assert!(resolved.has_transform());
    assert_matrix(
        matrix(resolved),
        [
            [0.0, 0.0, -1.0, -2.0],
            [0.0, -2.0, 0.0, 2.0],
            [-3.0, 0.0, 0.0, 0.75],
        ],
    );
}

#[test]
fn off_axis_rotation_changes_homogeneous_uv_and_q() {
    let mut transform = identity();
    transform.rotation[0] = std::f32::consts::FRAC_PI_3;
    let root_three_over_two = 3.0_f32.sqrt() / 2.0;
    // A 2D TEX attribute reaches the ordinary postmatrix as (s,t,1,1),
    // not (s,t,0,1). Rx(60) produces T=.5*t-sqrt(3)/2 and
    // Q=sqrt(3)/2*t+.5; the consumer must retain that Q for sampling.
    assert_matrix(
        matrix(resolve_texture_coordinates(4, 0, &transform, [1, 1], 0)),
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 0.5, -root_three_over_two, 0.0],
            [0.0, root_three_over_two, 0.5, 0.0],
        ],
    );
    transform.rotation = [0.0, std::f32::consts::FRAC_PI_3, 0.0];
    transform.translation[2] = 2.0;
    assert_matrix(
        matrix(resolve_texture_coordinates(4, 0, &transform, [1, 1], 0)),
        [
            [0.5, 0.0, root_three_over_two, 2.0 * root_three_over_two],
            [0.0, 1.0, 0.0, 0.0],
            [-root_three_over_two, 0.0, 0.5, 1.0],
        ],
    );
}

#[test]
fn matrix_texgen_uses_active_gx_dispatch_not_generated_enum_names() {
    let identity = identity();
    // GXAttr.c:475–515 initializes row=5, then TEX1 selects row6. The
    // TEXCOORD1 case does not change row; only BUMP uses src-12 as its source.
    assert_eq!(
        resolve_texture_coordinates(5, 0, &identity, [1, 1], 0).tex_coord_index(),
        Some(1)
    );
    assert_eq!(
        resolve_texture_coordinates(13, 0, &identity, [1, 1], 0).tex_coord_index(),
        Some(0)
    );
    // TEX2 remains TEX2, even though today's vertex decoder cannot supply it.
    assert_eq!(
        resolve_texture_coordinates(6, 0, &identity, [1, 1], 0).tex_coord_index(),
        Some(2)
    );
    assert!(matches!(
        resolve_texture_coordinates(0, 0, &identity, [1, 1], 0),
        HsdTextureCoordinates::Matrix {
            source: HsdTextureSource::Position,
            ..
        }
    ));
}

#[test]
fn reflection_hardwires_normal_and_is_generated_even_with_identity_srt() {
    for source in [0, 1, 5, 13, 21, u32::MAX] {
        let coordinates = resolve_texture_coordinates(
            source,
            texture_flags::COORD_REFLECTION,
            &identity(),
            [1, 1],
            0,
        );
        assert_eq!(coordinates.tex_coord_index(), None);
        assert!(coordinates.has_transform());
        assert_matrix(
            reflection_matrix(coordinates),
            [
                [0.5, 0.0, 0.0, 0.5],
                [0.0, -0.5, 0.0, 0.5],
                [0.0, 0.0, 0.0, 1.0],
            ],
        );
    }
    let ordinary = resolve_texture_coordinates(1, 0, &identity(), [1, 1], 0);
    assert!(matches!(
        ordinary,
        HsdTextureCoordinates::Matrix {
            source: HsdTextureSource::Normal,
            ..
        }
    ));
    assert!(!ordinary.has_transform());
    assert_matrix(
        matrix(ordinary),
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    );
}

#[test]
fn reflection_reuses_repeat_mirror_and_xyz_srt_before_postmatrix_conversion() {
    let transform = HsdTransform {
        scale: [2.0, 4.0, 3.0],
        rotation: [std::f32::consts::FRAC_PI_2; 3],
        translation: [0.25, 0.5, 2.0],
    };
    // MakeTextureMtx is the same [-Z-2, -2Y+2, -3X+.75] as the ordinary
    // vector above. Reflection folds each row into [.5X,-.5Y,0,.5X+.5Y+Z+W].
    assert_matrix(
        reflection_matrix(resolve_texture_coordinates(
            4,
            texture_flags::COORD_REFLECTION,
            &transform,
            [2, 8],
            2,
        )),
        [
            [0.0, 0.0, 0.0, -3.0],
            [0.0, 1.0, 0.0, 1.0],
            [-1.5, 0.0, 0.0, -0.75],
        ],
    );
}

#[test]
fn reflection_preserves_bump_precedence_and_rejects_invalid_arithmetic() {
    use HsdTextureUnsupportedReason as Reason;
    assert_eq!(
        resolve_texture_coordinates(
            u32::MAX,
            texture_flags::COORD_REFLECTION | texture_flags::BUMP,
            &HsdTransform {
                scale: [f32::NAN; 3],
                ..identity()
            },
            [0, 0],
            0,
        ),
        HsdTextureCoordinates::Unsupported {
            reason: Reason::EmbossLight
        },
    );
    for (transform, repeat, wrap_t, reason) in [
        (identity(), [0, 1], 0, Reason::ZeroRepeat),
        (identity(), [1, 0], 0, Reason::ZeroRepeat),
        (
            HsdTransform {
                rotation: [f32::NAN, 0.0, 0.0],
                ..identity()
            },
            [1, 1],
            0,
            Reason::NonFiniteTransform,
        ),
        (
            HsdTransform {
                translation: [0.0, f32::INFINITY, 0.0],
                ..identity()
            },
            [1, 1],
            0,
            Reason::NonFiniteTransform,
        ),
        (
            HsdTransform {
                scale: [1.0e-9, 1.0, 1.0],
                translation: [f32::MAX, 0.0, 0.0],
                ..identity()
            },
            [1, 1],
            0,
            Reason::NonFiniteTransform,
        ),
        // Finite mirror offset plus finite translation overflows before SRT.
        (
            HsdTransform {
                scale: [1.0, f32::MAX, 1.0],
                translation: [0.0, f32::MAX, 0.0],
                ..identity()
            },
            [1, 1],
            2,
            Reason::NonFiniteTransform,
        ),
    ] {
        assert_eq!(
            resolve_texture_coordinates(
                u32::MAX,
                texture_flags::COORD_REFLECTION,
                &transform,
                repeat,
                wrap_t,
            ),
            HsdTextureCoordinates::Unsupported { reason },
        );
    }
    // MakeTextureMtx remains finite; only reflection's Z+W addition overflows.
    let transform = HsdTransform {
        scale: [1.0, 1.0, f32::MAX],
        translation: [0.0, 0.0, 0.5],
        ..identity()
    };
    assert!(
        matrix(resolve_texture_coordinates(4, 0, &transform, [1, 1], 0))
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    );
    assert_eq!(
        resolve_texture_coordinates(4, texture_flags::COORD_REFLECTION, &transform, [1, 1], 0),
        HsdTextureCoordinates::Unsupported {
            reason: Reason::NonFiniteTransform
        },
    );
}

#[test]
fn unsupported_dependencies_and_invalid_arithmetic_never_become_uv0() {
    use HsdTextureUnsupportedReason as Reason;
    let identity = identity();
    for (flags, reason) in [
        (texture_flags::COORD_TOON, Reason::ToonRasterColor),
        (texture_flags::BUMP, Reason::EmbossLight),
    ] {
        let coordinates = resolve_texture_coordinates(4, flags, &identity, [1, 1], 0);
        assert_eq!(coordinates, HsdTextureCoordinates::Unsupported { reason });
        assert_eq!(coordinates.tex_coord_index(), None);
    }
    for (source, repeat, transform, reason) in [
        (21, [1, 1], identity, Reason::TexGenSource),
        (4, [0, 1], identity, Reason::ZeroRepeat),
        (
            4,
            [1, 1],
            HsdTransform {
                scale: [f32::NAN, 1.0, 1.0],
                ..identity
            },
            Reason::NonFiniteTransform,
        ),
        (
            4,
            [1, 1],
            HsdTransform {
                translation: [f32::INFINITY, 0.0, 0.0],
                ..identity
            },
            Reason::NonFiniteTransform,
        ),
        // Finite descriptors can still overflow their final S*R*T matrix.
        (
            4,
            [1, 1],
            HsdTransform {
                scale: [1.0e-9, 1.0, 1.0],
                translation: [f32::MAX, 0.0, 0.0],
                ..identity
            },
            Reason::NonFiniteTransform,
        ),
    ] {
        assert_eq!(
            resolve_texture_coordinates(source, 0, &transform, repeat, 0),
            HsdTextureCoordinates::Unsupported { reason }
        );
    }
}

#[test]
fn texture_attribute_availability_tracks_decoding_not_default_zero_values() {
    use dat_parser::DatFile;
    use dat_parser::descriptor::pobj::GxAttribute;
    use dat_parser::gx::display_list::{PrimitiveGroup, RawVertex};
    use dat_parser::gx::vertex::decode_primitives;
    use dat_parser::gx::{GxAttrName, GxAttrType, GxCompType, GxComponent, GxPrimitiveType};

    let dat = DatFile::from_parts(vec![0; 8], Vec::new(), Vec::new());
    let mut attributes = [
        GxAttribute {
            attr_name: GxAttrName::Tex0,
            attr_type: GxAttrType::Index8,
            comp_count: 1,
            comp_type: GxComponent::Number(GxCompType::Float),
            scale: 0,
            stride: 8,
            buffer_ptr: Some(0),
        },
        GxAttribute {
            attr_name: GxAttrName::Tex1,
            attr_type: GxAttrType::Index8,
            comp_count: 1,
            comp_type: GxComponent::Number(GxCompType::Float),
            scale: 0,
            stride: 8,
            buffer_ptr: Some(4), // Truncated: only one float remains.
        },
    ];
    let groups = [PrimitiveGroup {
        primitive_type: GxPrimitiveType::Triangles,
        vertices: vec![RawVertex {
            indices: vec![0, 0],
            color0: None,
            color1: None,
        }],
    }];
    let decoded = decode_primitives(&dat, &attributes, &groups);
    assert_eq!(decoded.vertices[0].tex_coords, [[0.0; 2]; 8]);
    assert_eq!(decoded.vertices[0].tex_coord_mask, 1);

    // Direct TEX attributes are not implemented by the decoder; a buffer pointer
    // must not make the direct attribute falsely appear available.
    attributes[0].attr_type = GxAttrType::Direct;
    assert_eq!(
        decode_primitives(&dat, &attributes, &groups).vertices[0].tex_coord_mask,
        0
    );
    attributes[0].attr_type = GxAttrType::None;
    assert_eq!(
        decode_primitives(&dat, &attributes, &groups).vertices[0].tex_coord_mask,
        0
    );
}
