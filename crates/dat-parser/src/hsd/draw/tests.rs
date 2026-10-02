use super::*;
use crate::gx::vertex::{DecodedPrimitive, DecodedVertex};
use crate::hsd::scene::{DObjId, HsdDisplayObject, HsdEnvelope, HsdJoint, HsdSceneRoot, HsdWeight};

fn vertex(pn_mtx_idx: u16) -> DecodedVertex {
    DecodedVertex {
        position: [1.0, 0.0, 0.0],
        normal: [1.0, 1.0, 0.0],
        binormal: [2.0, 0.0, 0.0],
        tangent: [0.0, 3.0, 0.0],
        has_nbt: true,
        pn_mtx_idx,
        ..Default::default()
    }
}

fn polygon(source_id: u32, binding: HsdPolygonBinding) -> super::super::scene::HsdPolygon {
    super::super::scene::HsdPolygon {
        source_id: PObjId(source_id),
        flags: 0,
        attributes: Vec::new(),
        primitive_groups: Vec::new(),
        decoded: DecodedPrimitive {
            vertices: vec![vertex(0)],
            triangles: Vec::new(),
        },
        binding,
    }
}

fn scene() -> HsdScene {
    let rigid = polygon(
        100,
        HsdPolygonBinding::Rigid {
            joint: Some(JObjId(20)),
        },
    );
    let envelope = polygon(
        104,
        HsdPolygonBinding::Envelope {
            source_offset: Some(200),
            entries: vec![HsdEnvelope {
                source_offset: 204,
                weights: vec![
                    HsdWeight {
                        joint: JObjId(10),
                        weight: 0.25,
                    },
                    HsdWeight {
                        joint: JObjId(20),
                        weight: 0.75,
                    },
                ],
            }],
        },
    );
    HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(10),
            name: Some("draw_root".into()),
            joints: vec![
                HsdJoint {
                    source_id: JObjId(10),
                    parent: None,
                    children: vec![HsdJointIndex(1)],
                    flags: jobj::flags::SKELETON_ROOT,
                    local: HsdTransform {
                        scale: [1.0, 1.0, 1.0],
                        rotation: [0.0, 0.0, 0.0],
                        translation: [5.0, 0.0, 0.0],
                    },
                    inverse_bind_transform: Some(Mat4::identity()),
                    display_objects: vec![HsdDisplayObject {
                        source_id: DObjId(40),
                        material: None,
                        polygons: vec![rigid, envelope],
                    }],
                },
                HsdJoint {
                    source_id: JObjId(20),
                    parent: Some(HsdJointIndex(0)),
                    children: Vec::new(),
                    flags: 0,
                    local: HsdTransform {
                        scale: [1.0, 1.0, 1.0],
                        rotation: [0.0, 0.0, 0.0],
                        translation: [0.0, 2.0, 0.0],
                    },
                    inverse_bind_transform: Some(Mat4::identity()),
                    display_objects: Vec::new(),
                },
            ],
        }],
        textures: Vec::new(),
    }
}

fn assert_vec3(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
    }
}

#[test]
fn bind_pose_emits_stable_source_ordered_packets() {
    let scene = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER)
            .expect("prepare");
    let work = evaluator.evaluate_bind_pose(&scene).expect("evaluate");
    let root = &work.roots[0];

    assert_eq!(root.source_id, JObjId(10));
    assert_eq!(root.packets.len(), 2);
    assert_eq!(
        root.packets[0],
        HsdEvaluatedDrawPacket {
            joint_index: HsdJointIndex(0),
            display_object_index: 0,
            polygon_index: 0,
            polygon_source_id: PObjId(100),
            first_vertex: 0,
            vertex_count: 1,
            visible: true,
        }
    );
    assert_eq!(root.packets[1].polygon_source_id, PObjId(104));
    assert_eq!(root.packets[1].first_vertex, 1);
    assert_vec3(root.positions[0], [6.0, 2.0, 0.0]);
    assert_vec3(root.positions[1], [6.0, 1.5, 0.0]);
    assert_vec3(
        root.normals[0],
        [
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        ],
    );
    assert_vec3(root.binormals[0], [2.0, 0.0, 0.0]);
    assert_vec3(root.tangents[0], [0.0, 3.0, 0.0]);
}

#[test]
fn runtime_display_object_visibility_compacts_restores_and_rejects_atomically() {
    let mut scene = scene();
    scene.roots[0].joints[1]
        .display_objects
        .push(HsdDisplayObject {
            source_id: DObjId(44),
            material: None,
            polygons: vec![polygon(108, HsdPolygonBinding::Rigid { joint: None })],
        });
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER).unwrap();
    let mut baseline_evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER).unwrap();
    let baseline = baseline_evaluator.evaluate_bind_pose(&scene).unwrap();
    evaluator
        .set_hidden_display_objects(0, &[DObjId(40)])
        .unwrap();
    let prepared = evaluator
        .prepared_packets()
        .map(|(_, packet)| packet)
        .collect::<Vec<_>>();
    let hidden = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(hidden.roots[0].packets, prepared);
    assert_eq!(hidden.roots[0].packets.len(), 1);
    assert_eq!(hidden.roots[0].packets[0].polygon_source_id, PObjId(108));
    assert_eq!(hidden.roots[0].packets[0].first_vertex, 0);
    assert_eq!(hidden.roots[0].positions, baseline.roots[0].positions[2..]);
    assert_eq!(hidden.roots[0].normals, baseline.roots[0].normals[2..]);
    assert_eq!(
        hidden.roots[0]
            .joint_world_matrices
            .iter()
            .map(|matrix| matrix.0)
            .collect::<Vec<_>>(),
        baseline.roots[0]
            .joint_world_matrices
            .iter()
            .map(|matrix| matrix.0)
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        evaluator.set_hidden_display_objects(0, &[DObjId(44), DObjId(999)]),
        Err(HsdDrawWorkError::MissingDisplayObject {
            source_id: DObjId(999),
            ..
        })
    ));
    assert!(matches!(
        evaluator.set_hidden_display_objects(1, &[]),
        Err(HsdDrawWorkError::VisibilityRootOutOfRange { .. })
    ));
    assert_eq!(
        evaluator.evaluate_bind_pose(&scene).unwrap().roots[0].packets,
        prepared
    );
    evaluator
        .set_hidden_display_objects(0, &[DObjId(40), DObjId(44), DObjId(44)])
        .unwrap();
    let hidden = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert!(hidden.roots[0].packets.is_empty());
    assert!(hidden.roots[0].positions.is_empty());
    assert_eq!(hidden.roots[0].joint_world_matrices.len(), 2);
    evaluator.set_hidden_display_objects(0, &[]).unwrap();
    let restored = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(restored.roots[0].packets, baseline.roots[0].packets);
    assert_eq!(restored.roots[0].positions, baseline.roots[0].positions);
    evaluator
        .set_hidden_display_objects(0, &[DObjId(40)])
        .unwrap();
    scene.roots[0].joints[0].display_objects[0].source_id = DObjId(999);
    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));
}

#[test]
fn runtime_display_object_visibility_suppresses_every_instance_occurrence() {
    let scene = instance_scene();
    let source_id = scene.roots[0].joints[1].display_objects[0].source_id;
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let mut baseline_evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let baseline = baseline_evaluator.evaluate_bind_pose(&scene).unwrap();
    evaluator
        .set_hidden_display_objects(0, &[source_id])
        .unwrap();
    let prepared = evaluator
        .prepared_packets()
        .map(|(_, packet)| packet)
        .collect::<Vec<_>>();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    let packets = &work.roots[0].packets;
    assert_eq!(*packets, prepared);
    assert!(packets.len() < baseline.roots[0].packets.len());
    assert!(packets.iter().all(|packet| {
        scene.roots[0].joints[packet.joint_index.0].display_objects[packet.display_object_index]
            .source_id
            != source_id
    }));
    let mut end = 0;
    for packet in packets {
        assert_eq!(packet.first_vertex, end);
        end += packet.vertex_count;
    }
    assert_eq!(end, work.roots[0].positions.len());
    assert_eq!(
        work.roots[0]
            .joint_world_matrices
            .iter()
            .map(|matrix| matrix.0)
            .collect::<Vec<_>>(),
        baseline.roots[0]
            .joint_world_matrices
            .iter()
            .map(|matrix| matrix.0)
            .collect::<Vec<_>>()
    );
}

#[test]
fn animated_pose_updates_rigid_and_weighted_vertices_and_normals() {
    let scene = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER)
            .expect("prepare");
    let mut pose = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    pose[1].scale = [2.0, 1.0, 1.0];
    let work = evaluator
        .evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                transforms: &pose,
                hidden_joints: None,
            }],
        )
        .expect("evaluate animated pose");
    let root = &work.roots[0];

    assert_vec3(root.positions[0], [7.0, 2.0, 0.0]);
    assert_vec3(root.positions[1], [6.75, 1.5, 0.0]);
    assert_vec3(root.normals[0], [0.447_213_6, 0.894_427_2, 0.0]);
    assert_vec3(root.normals[1], [0.496_138_93, 0.868_243_16, 0.0]);
}

#[test]
fn repeated_frames_reuse_streams_and_recover_after_partial_vertex_failure() {
    let mut scene = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER).unwrap();
    let frame = evaluator.evaluate_bind_pose(&scene).unwrap();
    let root = &frame.roots[0];
    let storage = (
        root.joint_world_matrices.as_ptr(),
        root.positions.as_ptr(),
        root.normals.as_ptr(),
        root.binormals.as_ptr(),
        root.tangents.as_ptr(),
        root.packets.as_ptr(),
    );
    assert_vec3(root.positions[1], [6.0, 1.5, 0.0]);

    // Fail after the rigid packet has already emitted a vertex and both
    // frame-local rigid and envelope caches have been populated.
    scene.roots[0].joints[0].display_objects[0].polygons[1]
        .decoded
        .vertices[0]
        .normal[0] = f32::NAN;
    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::Evaluation {
            polygon_source_id: PObjId(104),
            source: HsdEnvelopeEvaluationError::NonFiniteVertexInput,
            ..
        })
    ));
    scene.roots[0].joints[0].display_objects[0].polygons[1]
        .decoded
        .vertices[0]
        .normal = [1.0, 1.0, 0.0];
    scene.roots[0].joints[1].inverse_bind_transform =
        Some(Mat4::from_srt([1.0; 3], [0.0; 3], [2.0, 0.0, 0.0]));
    let mut pose = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    pose[1].scale = [2.0, 1.0, 1.0];
    let frame = evaluator
        .evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                transforms: &pose,
                hidden_joints: None,
            }],
        )
        .unwrap();
    assert_vec3(frame.roots[0].positions[0], [7.0, 2.0, 0.0]);
    assert_vec3(frame.roots[0].positions[1], [9.75, 1.5, 0.0]);
    assert_vec3(frame.roots[0].normals[1], [0.496_138_93, 0.868_243_16, 0.0]);
    assert_eq!(frame.roots[0].packets.len(), 2);
    assert_eq!(frame.roots[0].positions.len(), 2);

    // No pose in this call means bind pose, not the previous animated input.
    let frame = evaluator.evaluate_bind_pose(&scene).unwrap();
    let root = &frame.roots[0];
    assert_vec3(root.positions[0], [6.0, 2.0, 0.0]);
    assert_vec3(root.positions[1], [7.5, 1.5, 0.0]);
    assert_vec3(root.binormals[1], [2.0, 0.0, 0.0]);
    assert_vec3(root.tangents[1], [0.0, 3.0, 0.0]);
    assert_eq!(
        storage,
        (
            root.joint_world_matrices.as_ptr(),
            root.positions.as_ptr(),
            root.normals.as_ptr(),
            root.binormals.as_ptr(),
            root.tangents.as_ptr(),
            root.packets.as_ptr(),
        )
    );
}

#[test]
fn repeated_frames_clear_pose_assignments_after_early_and_late_root_errors() {
    let mut scene = scene();
    scene.roots.extend(instance_scene().roots);
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER).unwrap();
    let mut pose = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    pose[0].translation[0] += 10.0;
    let root_pose = HsdRootPose {
        root_index: 0,
        transforms: &pose,
        hidden_joints: None,
    };
    assert!(matches!(
        evaluator.evaluate(&scene, &[root_pose, root_pose]),
        Err(HsdDrawWorkError::DuplicateRootPose { root_index: 0 })
    ));
    assert!(matches!(
        evaluator.evaluate(
            &scene,
            &[
                root_pose,
                HsdRootPose {
                    root_index: 2,
                    hidden_joints: None,
                    transforms: &pose
                }
            ]
        ),
        Err(HsdDrawWorkError::PoseRootOutOfRange {
            root_index: 2,
            root_count: 2
        })
    ));
    scene.roots[1].joints[4].children[0] = HsdJointIndex(3);
    assert!(matches!(
        evaluator.evaluate(&scene, &[root_pose]),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));
    scene.roots[1].joints[4].children[0] = HsdJointIndex(1);
    scene.roots[1].joints[1].local.scale[0] = 0.0;
    assert!(matches!(
        evaluator.evaluate(&scene, &[root_pose]),
        Err(HsdDrawWorkError::SingularInstanceTarget { root_index: 1, .. })
    ));
    scene.roots[1].joints[1].local.scale[0] = 1.0;
    let frame = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_vec3(frame.roots[0].positions[0], [6.0, 2.0, 0.0]);
    assert_vec3(frame.roots[1].positions[4], [16.0, 3.0, 0.0]);
    assert_vec3(frame.roots[1].positions[5], [17.5, 2.25, 0.0]);
}

#[test]
fn envelope_model_node_uses_skeleton_relative_correction() {
    let mut scene = scene();
    let envelope = scene.roots[0].joints[0].display_objects[0]
        .polygons
        .pop()
        .expect("envelope polygon");
    scene.roots[0].joints[1]
        .display_objects
        .push(HsdDisplayObject {
            source_id: DObjId(44),
            material: None,
            polygons: vec![envelope],
        });
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");

    let work = evaluator.evaluate_bind_pose(&scene).expect("evaluate");
    assert_vec3(work.roots[0].positions[1], [6.0, 3.5, 0.0]);
}

#[test]
fn envelope_model_node_that_is_the_skeleton_uses_inverse_envelope_matrix() {
    let mut scene = scene();
    scene.roots[0].joints[0].flags = jobj::flags::SKELETON;
    let near_singular = Mat4::from_srt([1.0e-12, 1.0, 1.0], [0.0; 3], [0.0; 3]);
    scene.roots[0].joints[0].inverse_bind_transform = Some(near_singular);
    scene.roots[0].joints[1].inverse_bind_transform = Some(near_singular);
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");

    let work = evaluator.evaluate_bind_pose(&scene).expect("evaluate");
    assert_vec3(work.roots[0].positions[1], [6.0, 1.5, 0.0]);
}

#[test]
fn self_skeleton_with_exactly_singular_envelope_matrix_fails_closed() {
    let mut scene = scene();
    scene.roots[0].joints[0].flags = jobj::flags::SKELETON;
    scene.roots[0].joints[0].inverse_bind_transform =
        Some(Mat4::from_srt([0.0, 1.0, 1.0], [0.0; 3], [0.0; 3]));
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");

    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::SingularEnvelopeMatrix {
            root_index: 0,
            polygon_source_id: PObjId(104),
            joint_index: HsdJointIndex(0),
        })
    ));
}

#[test]
fn envelope_model_node_uses_non_root_skeleton_envelope_matrix() {
    let mut scene = scene();
    let envelope = scene.roots[0].joints[0].display_objects[0]
        .polygons
        .pop()
        .expect("envelope polygon");
    scene.roots[0].joints[0].flags = 0;
    scene.roots[0].joints[1].flags = jobj::flags::SKELETON;
    scene.roots[0].joints[1].children = vec![HsdJointIndex(2)];
    scene.roots[0].joints[1].inverse_bind_transform =
        Some(Mat4::from_srt([1.0; 3], [0.0; 3], [0.0, -2.0, 0.0]));
    scene.roots[0].joints.push(HsdJoint {
        source_id: JObjId(30),
        parent: Some(HsdJointIndex(1)),
        children: Vec::new(),
        flags: 0,
        local: HsdTransform {
            scale: [1.0; 3],
            rotation: [0.0; 3],
            translation: [0.0, 3.0, 0.0],
        },
        inverse_bind_transform: None,
        display_objects: vec![HsdDisplayObject {
            source_id: DObjId(44),
            material: None,
            polygons: vec![envelope],
        }],
    });
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");

    let work = evaluator.evaluate_bind_pose(&scene).expect("evaluate");
    assert_vec3(work.roots[0].positions[1], [6.0, 5.0, 0.0]);
}

#[test]
fn envelope_model_node_without_a_skeleton_fails_closed() {
    let mut scene = scene();
    scene.roots[0].joints[0].flags = 0;
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");

    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::MissingEnvelopeSkeleton {
            root_index: 0,
            polygon_source_id: PObjId(104),
            joint_index: HsdJointIndex(0),
        })
    ));
}

#[test]
fn hsd_affine_inverse_matches_nonsingular_and_singular_source_paths() {
    let matrix = Mat4::from_srt([2.0, 0.5, 1.25], [0.3, -0.2, 0.4], [7.0, -3.0, 5.0]);
    let identity = hsd_inverse_affine_or_identity(matrix).mul(&matrix);
    for (column_index, column) in identity.0.into_iter().enumerate() {
        for (row_index, value) in column.into_iter().enumerate() {
            let expected = f32::from(column_index == row_index);
            assert!((value - expected).abs() < 1.0e-5, "{value} != {expected}");
        }
    }

    let singular = Mat4::from_srt([0.0, 1.0, 1.0], [0.0; 3], [2.0, 3.0, 4.0]);
    let fallback = hsd_inverse_affine_or_identity(singular);
    for (column_index, column) in fallback.0.into_iter().enumerate() {
        for (row_index, value) in column.into_iter().enumerate() {
            assert_eq!(value, f32::from(column_index == row_index));
        }
    }
    assert!(psmtx_inverse_affine(singular).is_none());
}

#[test]
fn pose_shape_and_prepared_scene_mismatches_fail_closed() {
    let mut scene = scene();
    let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD)
        .expect("prepare");
    assert!(matches!(
        evaluator.evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                transforms: &[],
                hidden_joints: None,
            }]
        ),
        Err(HsdDrawWorkError::PoseJointCountMismatch { .. })
    ));

    scene.roots[0].joints[0].source_id = JObjId(999);
    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));
}

#[test]
fn prepared_topology_rejects_changed_hierarchy_packet_paths_and_vertex_counts() {
    let mut changed_parent = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&changed_parent, HsdDrawEvaluationPolicy::GENERIC_HSD)
            .expect("prepare parent case");
    changed_parent.roots[0].joints[1].parent = None;
    assert!(matches!(
        evaluator.evaluate_bind_pose(&changed_parent),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));

    let mut moved_packet = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&moved_packet, HsdDrawEvaluationPolicy::GENERIC_HSD)
            .expect("prepare packet case");
    let envelope = moved_packet.roots[0].joints[0].display_objects[0]
        .polygons
        .pop()
        .expect("envelope packet");
    moved_packet.roots[0].joints[0]
        .display_objects
        .push(HsdDisplayObject {
            source_id: DObjId(44),
            material: None,
            polygons: vec![envelope],
        });
    assert!(matches!(
        evaluator.evaluate_bind_pose(&moved_packet),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));

    let mut changed_vertices = scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&changed_vertices, HsdDrawEvaluationPolicy::GENERIC_HSD)
            .expect("prepare vertex case");
    changed_vertices.roots[0].joints[0].display_objects[0].polygons[0]
        .decoded
        .vertices
        .push(vertex(0));
    assert!(matches!(
        evaluator.evaluate_bind_pose(&changed_vertices),
        Err(HsdDrawWorkError::SceneTopologyMismatch)
    ));
}

fn instance_joint(
    source_id: u32,
    parent: Option<usize>,
    children: &[usize],
    flags: u32,
    translation: [f32; 3],
    polygons: Vec<super::super::scene::HsdPolygon>,
) -> HsdJoint {
    HsdJoint {
        source_id: JObjId(source_id),
        parent: parent.map(HsdJointIndex),
        children: children.iter().copied().map(HsdJointIndex).collect(),
        flags,
        local: HsdTransform {
            scale: [1.0; 3],
            rotation: [0.0; 3],
            translation,
        },
        inverse_bind_transform: Some(Mat4::identity()),
        display_objects: if polygons.is_empty() {
            Vec::new()
        } else {
            vec![HsdDisplayObject {
                source_id: DObjId(source_id + 200),
                material: None,
                polygons,
            }]
        },
    }
}

fn instance_scene() -> HsdScene {
    HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(10),
            name: None,
            joints: vec![
                instance_joint(
                    10,
                    None,
                    &[1, 3, 4, 5],
                    jobj::flags::SKELETON_ROOT,
                    [5.0, 0.0, 0.0],
                    vec![],
                ),
                instance_joint(
                    20,
                    Some(0),
                    &[2],
                    0,
                    [2.0, 0.0, 0.0],
                    vec![
                        polygon(
                            100,
                            HsdPolygonBinding::Rigid {
                                joint: Some(JObjId(30)),
                            },
                        ),
                        polygon(
                            104,
                            HsdPolygonBinding::Envelope {
                                source_offset: Some(300),
                                entries: vec![HsdEnvelope {
                                    source_offset: 304,
                                    weights: vec![
                                        HsdWeight {
                                            joint: JObjId(10),
                                            weight: 0.25,
                                        },
                                        HsdWeight {
                                            joint: JObjId(30),
                                            weight: 0.75,
                                        },
                                    ],
                                }],
                            },
                        ),
                    ],
                ),
                instance_joint(
                    30,
                    Some(1),
                    &[],
                    0,
                    [0.0, 3.0, 0.0],
                    vec![polygon(108, HsdPolygonBinding::Rigid { joint: None })],
                ),
                instance_joint(
                    40,
                    Some(0),
                    &[],
                    0,
                    [0.0, 8.0, 0.0],
                    vec![polygon(112, HsdPolygonBinding::Rigid { joint: None })],
                ),
                // These DObjs are retained as source metadata but never drawn.
                instance_joint(
                    50,
                    Some(0),
                    &[1],
                    INSTANCE,
                    [10.0, 0.0, 0.0],
                    vec![polygon(116, HsdPolygonBinding::Rigid { joint: None })],
                ),
                instance_joint(60, Some(0), &[1], INSTANCE, [20.0, 0.0, 0.0], vec![]),
            ],
        }],
        textures: vec![],
    }
}

#[test]
fn instances_keep_original_owner_source_identity_and_subtree_order() {
    let scene = instance_scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    let root = &work.roots[0];
    assert_eq!(root.joint_world_matrices.len(), 6);
    assert_vec3(
        root.joint_world_matrices[1].transform_point([0.0; 3]),
        [7.0, 0.0, 0.0],
    );
    assert_vec3(
        root.joint_world_matrices[2].transform_point([0.0; 3]),
        [7.0, 3.0, 0.0],
    );
    assert_eq!(scene.roots[0].joints[1].parent, Some(HsdJointIndex(0)));
    assert_eq!(
        root.packets
            .iter()
            .map(|packet| packet.polygon_source_id.0)
            .collect::<Vec<_>>(),
        [100, 104, 108, 112, 100, 104, 108, 100, 104, 108]
    );
    for (index, packet) in root.packets.iter().enumerate() {
        assert_eq!(packet.first_vertex, index);
        assert_eq!(packet.vertex_count, 1);
    }
    for (original, first, second) in [(0, 4, 7), (1, 5, 8), (2, 6, 9)] {
        let mut packet = root.packets[original];
        packet.first_vertex = first;
        assert_eq!(root.packets[first], packet);
        packet.first_vertex = second;
        assert_eq!(root.packets[second], packet);
    }
    for (actual, expected) in root.positions.iter().zip([
        [8.0, 3.0, 0.0],
        [9.5, 2.25, 0.0],
        [8.0, 3.0, 0.0],
        [6.0, 8.0, 0.0],
        [16.0, 3.0, 0.0],
        [17.5, 2.25, 0.0],
        [16.0, 3.0, 0.0],
        [26.0, 3.0, 0.0],
        [27.5, 2.25, 0.0],
        [26.0, 3.0, 0.0],
    ]) {
        assert_vec3(*actual, expected);
    }
}

#[test]
fn animated_instances_recompute_correction_using_owned_target_matrices() {
    let scene = instance_scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::MELEE_FIGHTER).unwrap();
    let mut pose = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    pose[1].translation[0] = 4.0;
    pose[2].scale = [2.0, 1.0, 1.0];
    pose[4].translation[0] = 30.0;
    let work = evaluator
        .evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                transforms: &pose,
                hidden_joints: None,
            }],
        )
        .unwrap();
    assert_vec3(work.roots[0].positions[0], [11.0, 3.0, 0.0]);
    assert_vec3(work.roots[0].positions[1], [16.75, 2.25, 0.0]);
    assert_vec3(work.roots[0].positions[4], [37.0, 3.0, 0.0]);
    assert_vec3(work.roots[0].positions[5], [42.75, 2.25, 0.0]);
    assert_vec3(work.roots[0].positions[7], [27.0, 3.0, 0.0]);
    // A later bind-pose frame must not retain any animated correction/cache.
    assert_vec3(
        evaluator.evaluate_bind_pose(&scene).unwrap().roots[0].positions[4],
        [16.0, 3.0, 0.0],
    );
}

#[test]
fn instance_weighted_skinning_and_nbt_keep_inverse_transpose_policy() {
    let mut scene = instance_scene();
    scene.roots[0].joints[2].local.scale = [2.0, 1.0, 1.0];
    scene.roots[0].joints[4].local.scale = [3.0, 1.0, 1.0];
    for policy in [
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        HsdDrawEvaluationPolicy::MELEE_FIGHTER,
    ] {
        let mut evaluator = HsdDrawWorkEvaluator::prepare(&scene, policy).unwrap();
        let work = evaluator.evaluate_bind_pose(&scene).unwrap();
        let root = &work.roots[0];
        assert_vec3(root.positions[4], [21.0, 3.0, 0.0]);
        assert_vec3(root.positions[5], [29.25, 2.25, 0.0]);
        let normalize = |x: f32| {
            let length = (x * x + 1.0).sqrt();
            [x / length, 1.0 / length, 0.0]
        };
        assert_vec3(root.normals[4], normalize(1.0 / 6.0));
        assert_vec3(root.normals[5], normalize(1.0 / 5.25));
        assert_vec3(root.binormals[4], [2.0 / 6.0, 0.0, 0.0]);
        assert_vec3(root.binormals[5], [2.0 / 5.25, 0.0, 0.0]);
        assert_vec3(root.tangents[5], [0.0, 3.0, 0.0]);
    }
}

fn nested_instance_scene() -> HsdScene {
    HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(10),
            name: None,
            joints: vec![
                instance_joint(10, None, &[1, 3, 4], 0, [5.0, 0.0, 0.0], vec![]),
                instance_joint(20, Some(0), &[2], 0, [2.0, 0.0, 0.0], vec![]),
                instance_joint(30, Some(1), &[3], INSTANCE, [4.0, 0.0, 0.0], vec![]),
                instance_joint(
                    40,
                    Some(0),
                    &[],
                    0,
                    [20.0, 0.0, 0.0],
                    vec![polygon(100, HsdPolygonBinding::Rigid { joint: None })],
                ),
                instance_joint(50, Some(0), &[1], INSTANCE, [40.0, 0.0, 0.0], vec![]),
            ],
        }],
        textures: vec![],
    }
}

#[test]
fn nested_instances_replace_inherited_correction_and_hidden_instances_skip_branches() {
    let mut scene = nested_instance_scene();
    let evaluate = |scene: &HsdScene| {
        let mut evaluator =
            HsdDrawWorkEvaluator::prepare(scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
        evaluator.evaluate_bind_pose(scene).unwrap().roots[0]
            .positions
            .clone()
    };
    assert_eq!(
        evaluate(&scene),
        [[12.0, 0.0, 0.0], [26.0, 0.0, 0.0], [12.0, 0.0, 0.0]]
    );
    scene.roots[0].joints[4].flags |= HIDDEN;
    assert_eq!(evaluate(&scene), [[12.0, 0.0, 0.0], [26.0, 0.0, 0.0]]);
    scene.roots[0].joints[4].flags &= !HIDDEN;
    scene.roots[0].joints[2].flags |= HIDDEN;
    assert_eq!(evaluate(&scene), [[26.0, 0.0, 0.0]]);
}

#[test]
fn instance_cycle_and_invalid_root_local_reference_contracts_fail_closed() {
    let mut cycle = instance_scene();
    cycle.roots[0].joints[4].children = vec![HsdJointIndex(0)];
    assert!(matches!(
        HsdDrawWorkEvaluator::prepare(&cycle, HsdDrawEvaluationPolicy::GENERIC_HSD),
        Err(HsdDrawWorkError::InstanceCycle { .. })
    ));
    for targets in [
        vec![],
        vec![HsdJointIndex(1), HsdJointIndex(3)],
        vec![HsdJointIndex(99)],
    ] {
        let mut invalid = instance_scene();
        invalid.roots[0].joints[4].children = targets;
        assert!(
            HsdDrawWorkEvaluator::prepare(&invalid, HsdDrawEvaluationPolicy::GENERIC_HSD).is_err()
        );
    }
    let mut unowned = instance_scene();
    unowned.roots[0].joints[0].children.remove(0);
    assert!(HsdDrawWorkEvaluator::prepare(&unowned, HsdDrawEvaluationPolicy::GENERIC_HSD).is_err());
    // A reference cannot become an owning parent even if its target exists.
    let mut reparented = instance_scene();
    reparented.roots[0].joints[1].parent = Some(HsdJointIndex(4));
    assert!(
        HsdDrawWorkEvaluator::prepare(&reparented, HsdDrawEvaluationPolicy::GENERIC_HSD).is_err()
    );
}

#[test]
fn instance_expansion_budgets_are_inclusive_and_aggregate_across_roots() {
    let scene = instance_scene();
    let limits = HsdDrawWorkLimits {
        max_joint_occurrences: 10,
        max_packets: 10,
        max_vertices: 10,
    };
    let prepare = |scene: &HsdScene, limits| {
        HsdDrawWorkEvaluator::prepare_with_limits(
            scene,
            HsdDrawEvaluationPolicy::GENERIC_HSD,
            limits,
        )
    };
    let mut evaluator = prepare(&scene, limits).unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(work.roots[0].packets.len(), 10);
    for (limits, resource) in [
        (
            HsdDrawWorkLimits {
                max_joint_occurrences: 9,
                ..limits
            },
            "joint occurrences",
        ),
        (
            HsdDrawWorkLimits {
                max_packets: 9,
                ..limits
            },
            "packets",
        ),
        (
            HsdDrawWorkLimits {
                max_vertices: 9,
                ..limits
            },
            "vertices",
        ),
    ] {
        assert!(matches!(prepare(&scene, limits),
            Err(HsdDrawWorkError::LimitExceeded { resource: actual, limit: 9 }) if actual == resource));
    }
    let mut aggregate = instance_scene();
    aggregate.roots.extend(instance_scene().roots);
    let double = HsdDrawWorkLimits {
        max_joint_occurrences: 20,
        max_packets: 20,
        max_vertices: 20,
    };
    assert!(prepare(&aggregate, double).is_ok());
    for limited in [
        HsdDrawWorkLimits {
            max_joint_occurrences: 19,
            ..double
        },
        HsdDrawWorkLimits {
            max_packets: 19,
            ..double
        },
        HsdDrawWorkLimits {
            max_vertices: 19,
            ..double
        },
    ] {
        assert!(matches!(
            prepare(&aggregate, limited),
            Err(HsdDrawWorkError::LimitExceeded { .. })
        ));
    }
}

#[test]
fn zero_vertex_packets_and_empty_exponential_instances_still_consume_work() {
    let mut scene = instance_scene();
    for joint in &mut scene.roots[0].joints {
        for object in &mut joint.display_objects {
            for polygon in &mut object.polygons {
                polygon.decoded.vertices.clear();
            }
        }
    }
    let limits = HsdDrawWorkLimits {
        max_joint_occurrences: 10,
        max_packets: 10,
        max_vertices: 0,
    };
    let mut evaluator = HsdDrawWorkEvaluator::prepare_with_limits(
        &scene,
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        limits,
    )
    .unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(work.roots[0].packets.len(), 10);
    assert!(work.roots[0].positions.is_empty());
    assert!(matches!(
        HsdDrawWorkEvaluator::prepare_with_limits(
            &scene,
            HsdDrawEvaluationPolicy::GENERIC_HSD,
            HsdDrawWorkLimits {
                max_packets: 9,
                ..limits
            }
        ),
        Err(HsdDrawWorkError::LimitExceeded {
            resource: "packets",
            ..
        })
    ));

    let mut root = HsdSceneRoot {
        source_id: JObjId(10),
        name: None,
        joints: vec![instance_joint(10, None, &[], 0, [0.0; 3], vec![])],
    };
    for level in 0..12 {
        let target = root.joints.len();
        root.joints[0].children.push(HsdJointIndex(target));
        let children = if level == 11 {
            vec![]
        } else {
            vec![target + 1, target + 2]
        };
        root.joints.push(instance_joint(
            100 + target as u32,
            Some(0),
            &children,
            0,
            [0.0; 3],
            vec![],
        ));
        if level != 11 {
            for _ in 0..2 {
                let index = root.joints.len();
                root.joints.push(instance_joint(
                    100 + index as u32,
                    Some(target),
                    &[target + 3],
                    INSTANCE,
                    [0.0; 3],
                    vec![],
                ));
            }
        }
    }
    let empty = HsdScene {
        roots: vec![root],
        textures: vec![],
    };
    assert!(matches!(
        HsdDrawWorkEvaluator::prepare_with_limits(
            &empty,
            HsdDrawEvaluationPolicy::GENERIC_HSD,
            HsdDrawWorkLimits {
                max_joint_occurrences: 64,
                max_packets: 0,
                max_vertices: 0
            }
        ),
        Err(HsdDrawWorkError::LimitExceeded {
            resource: "joint occurrences",
            limit: 64
        })
    ));
}

#[test]
fn instance_target_inverse_preserves_exact_singular_and_finite_error_boundaries() {
    let scene = instance_scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let mut pose = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    pose[1].scale[0] = 0.0;
    assert!(matches!(
        evaluator.evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                hidden_joints: None,
                transforms: &pose
            }]
        ),
        Err(HsdDrawWorkError::SingularInstanceTarget {
            joint_index: HsdJointIndex(4),
            target: HsdJointIndex(1),
            ..
        })
    ));
    pose[1].scale[0] = 1.0e-12;
    assert!(
        evaluator
            .evaluate(
                &scene,
                &[HsdRootPose {
                    root_index: 0,
                    hidden_joints: None,
                    transforms: &pose
                }]
            )
            .is_ok()
    );
    pose[1].scale[0] = 1.0e-40;
    assert!(matches!(
        evaluator.evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                hidden_joints: None,
                transforms: &pose
            }]
        ),
        Err(HsdDrawWorkError::NonFiniteInstanceCorrection { .. })
    ));
    pose[1].scale[0] = 1.0;
    pose[4].translation[0] = f32::INFINITY;
    assert!(matches!(
        evaluator.evaluate(
            &scene,
            &[HsdRootPose {
                root_index: 0,
                hidden_joints: None,
                transforms: &pose
            }]
        ),
        Err(HsdDrawWorkError::NonFinitePoseTransform { .. })
    ));
}

#[test]
fn prepared_instances_reject_stale_edges_flags_and_all_source_packet_metadata() {
    let mutations: &[fn(&mut HsdScene)] = &[
        |scene| scene.roots[0].joints[4].children[0] = HsdJointIndex(3),
        |scene| scene.roots[0].joints[4].children.push(HsdJointIndex(1)),
        |scene| scene.roots[0].joints[0].children.swap(0, 1),
        |scene| scene.roots[0].joints[4].flags |= HIDDEN,
        |scene| scene.roots[0].joints[4].flags &= !INSTANCE,
        |scene| scene.roots[0].joints[1].display_objects[0].source_id = DObjId(999),
        |scene| scene.roots[0].joints[1].display_objects[0].polygons[0].source_id = PObjId(999),
        |scene| {
            scene.roots[0].joints[1].display_objects[0].polygons[0].binding =
                HsdPolygonBinding::Rigid { joint: None }
        },
        // Suppressed source packets must not disappear from verification.
        |scene| {
            scene.roots[0].joints[4].display_objects[0].polygons[0]
                .decoded
                .vertices
                .push(vertex(0))
        },
        |scene| scene.roots[0].joints[4].display_objects[0].polygons[0].source_id = PObjId(999),
        |scene| scene.roots[0].joints[4].display_objects[0].polygons.clear(),
    ];
    for mutation in mutations {
        let mut scene = instance_scene();
        let mut evaluator =
            HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
        mutation(&mut scene);
        assert!(matches!(
            evaluator.evaluate_bind_pose(&scene),
            Err(HsdDrawWorkError::SceneTopologyMismatch)
        ));
    }
}

#[test]
fn prepared_instances_keep_envelope_contents_and_unrelated_flags_frame_local() {
    let mut scene = instance_scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let before = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_vec3(before.roots[0].positions[5], [17.5, 2.25, 0.0]);
    let before_packets = before.roots[0].packets.clone();

    let polygon = &mut scene.roots[0].joints[1].display_objects[0].polygons[1];
    let HsdPolygonBinding::Envelope {
        source_offset,
        entries,
    } = &mut polygon.binding
    else {
        unreachable!()
    };
    *source_offset = Some(400);
    entries[0].source_offset = 404;
    entries[0].weights[1].weight = 0.5;
    polygon.flags ^= 1;
    let changed_weights = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_vec3(changed_weights.roots[0].positions[1], [7.0, 1.5, 0.0]);
    assert_vec3(changed_weights.roots[0].positions[5], [15.0, 1.5, 0.0]);
    assert_vec3(changed_weights.roots[0].positions[8], [25.0, 1.5, 0.0]);

    // This changes frame-local model-node evaluation, not occurrence topology.
    scene.roots[0].joints[1].flags |= jobj::flags::SKELETON_ROOT;
    let changed_flags = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_vec3(changed_flags.roots[0].positions[1], [5.5, 1.5, 0.0]);
    assert_vec3(changed_flags.roots[0].positions[5], [13.5, 1.5, 0.0]);
    assert_vec3(changed_flags.roots[0].positions[8], [23.5, 1.5, 0.0]);
    assert_eq!(changed_flags.roots[0].packets, before_packets);
}

#[test]
fn repeated_instance_palettes_track_current_entry_count_and_selector_errors() {
    let mut scene = instance_scene();
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    assert_vec3(
        evaluator.evaluate_bind_pose(&scene).unwrap().roots[0].positions[5],
        [17.5, 2.25, 0.0],
    );
    let polygon = &mut scene.roots[0].joints[1].display_objects[0].polygons[1];
    let HsdPolygonBinding::Envelope { entries, .. } = &mut polygon.binding else {
        unreachable!()
    };
    entries.push(HsdEnvelope {
        source_offset: 308,
        weights: vec![HsdWeight {
            joint: JObjId(30),
            weight: 1.0,
        }],
    });
    polygon.decoded.vertices[0].pn_mtx_idx = 3;
    assert_vec3(
        evaluator.evaluate_bind_pose(&scene).unwrap().roots[0].positions[5],
        [18.0, 3.0, 0.0],
    );

    // A slot retained in the fixed-capacity scratch is not an admitted slot
    // after the current source palette shrinks.
    let HsdPolygonBinding::Envelope { entries, .. } =
        &mut scene.roots[0].joints[1].display_objects[0].polygons[1].binding
    else {
        unreachable!()
    };
    entries.pop();
    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::Evaluation {
            polygon_source_id: PObjId(104),
            source: HsdEnvelopeEvaluationError::PaletteSelectorOutOfRange {
                pn_mtx_idx: 3,
                palette_len: 1,
                ..
            },
            ..
        })
    ));
    scene.roots[0].joints[1].display_objects[0].polygons[1]
        .decoded
        .vertices[0]
        .pn_mtx_idx = 0;
    let frame = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(frame.roots[0].packets.len(), 10);
    assert_eq!(frame.roots[0].positions.len(), 10);
    assert_vec3(frame.roots[0].positions[5], [17.5, 2.25, 0.0]);
    assert_vec3(frame.roots[0].positions[8], [27.5, 2.25, 0.0]);
}

#[test]
fn instance_correction_cancels_owned_rotation_before_applying_instance_rotation() {
    let mut scene = instance_scene();
    scene.roots[0].joints[1].local.rotation[2] = std::f32::consts::FRAC_PI_2;
    scene.roots[0].joints[4].local.rotation[2] = std::f32::consts::PI;
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_vec3(work.roots[0].positions[0], [4.0, 1.0, 0.0]);
    assert_vec3(work.roots[0].positions[4], [14.0, -3.0, 0.0]);
    assert_vec3(work.roots[0].positions[7], [26.0, 3.0, 0.0]);
    let n = std::f32::consts::FRAC_1_SQRT_2;
    assert_vec3(work.roots[0].normals[0], [-n, n, 0.0]);
    assert_vec3(work.roots[0].normals[4], [-n, -n, 0.0]);
    assert_vec3(work.roots[0].normals[7], [n, n, 0.0]);
}

#[test]
fn instance_correction_is_left_of_unnormalized_weighted_envelope() {
    let mut scene = instance_scene();
    let HsdPolygonBinding::Envelope { entries, .. } =
        &mut scene.roots[0].joints[1].display_objects[0].polygons[1].binding
    else {
        unreachable!()
    };
    entries[0].weights[1].weight = 0.5;
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    // Weight sum is .75, but HSD's 3x4 Mtx has an implicit affine row.
    // Both instance translations contribute in full (+8 and +18); the
    // weighted linear terms, translation, and normal policy stay unchanged.
    assert_vec3(work.roots[0].positions[1], [7.0, 1.5, 0.0]);
    assert_vec3(work.roots[0].positions[5], [15.0, 1.5, 0.0]);
    assert_vec3(work.roots[0].positions[8], [25.0, 1.5, 0.0]);
    for index in [1, 5, 8] {
        assert_vec3(work.roots[0].binormals[index], [8.0 / 3.0, 0.0, 0.0]);
        assert_vec3(work.roots[0].tangents[index], [0.0, 4.0, 0.0]);
    }
}

#[test]
fn hidden_empty_instance_does_not_invert_its_singular_target() {
    let mut scene = HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(10),
            name: None,
            joints: vec![
                instance_joint(10, None, &[1, 2], 0, [0.0; 3], vec![]),
                instance_joint(20, Some(0), &[2], INSTANCE | HIDDEN, [0.0; 3], vec![]),
                instance_joint(30, Some(0), &[], 0, [0.0; 3], vec![]),
            ],
        }],
        textures: vec![],
    };
    scene.roots[0].joints[2].local.scale[0] = 0.0;
    let limits = HsdDrawWorkLimits {
        max_joint_occurrences: 3,
        max_packets: 0,
        max_vertices: 0,
    };
    let mut evaluator = HsdDrawWorkEvaluator::prepare_with_limits(
        &scene,
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        limits,
    )
    .unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert!(work.roots[0].packets.is_empty());
    scene.roots[0].joints[1].flags &= !HIDDEN;
    let mut evaluator = HsdDrawWorkEvaluator::prepare_with_limits(
        &scene,
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        HsdDrawWorkLimits {
            max_joint_occurrences: 4,
            ..limits
        },
    )
    .unwrap();
    assert!(matches!(
        evaluator.evaluate_bind_pose(&scene),
        Err(HsdDrawWorkError::SingularInstanceTarget { .. })
    ));
}

#[test]
fn instance_left_composition_does_not_overflow_unweighted_source_influences() {
    let mut scene = HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(10),
            name: None,
            joints: vec![
                instance_joint(
                    10,
                    None,
                    &[1, 2, 3],
                    jobj::flags::SKELETON_ROOT,
                    [0.0; 3],
                    vec![],
                ),
                instance_joint(
                    20,
                    Some(0),
                    &[],
                    0,
                    [0.0; 3],
                    vec![polygon(
                        100,
                        HsdPolygonBinding::Envelope {
                            source_offset: Some(300),
                            entries: vec![HsdEnvelope {
                                source_offset: 304,
                                weights: vec![
                                    HsdWeight {
                                        joint: JObjId(30),
                                        weight: 0.125,
                                    },
                                    HsdWeight {
                                        joint: JObjId(10),
                                        weight: 0.125,
                                    },
                                ],
                            }],
                        },
                    )],
                ),
                instance_joint(30, Some(0), &[], 0, [f32::MAX / 4.0, 0.0, 0.0], vec![]),
                instance_joint(40, Some(0), &[1], INSTANCE, [0.0; 3], vec![]),
            ],
        }],
        textures: vec![],
    };
    scene.roots[0].joints[3].local.scale = [8.0; 3];
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    assert_eq!(work.roots[0].positions[0][0], f32::MAX / 32.0);
    assert_eq!(work.roots[0].positions[1][0], f32::MAX / 4.0);
    assert!(
        work.roots[0]
            .normals
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    );
}

#[test]
fn instance_own_dobjs_do_not_require_resolvable_rigid_bindings() {
    for hidden in [0, HIDDEN] {
        let mut scene = instance_scene();
        scene.roots[0].joints[4].flags |= hidden;
        let mut prepared =
            HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
        let expected = prepared.evaluate_bind_pose(&scene).unwrap();
        let expected_packets = expected.roots[0].packets.clone();
        let expected_positions = expected.roots[0].positions.clone();

        scene.roots[0].joints[4].display_objects[0].polygons[0].binding =
            HsdPolygonBinding::Rigid {
                joint: Some(JObjId(999)),
            };
        // Source metadata is still checked, even though it is not dispatched.
        assert!(matches!(
            prepared.evaluate_bind_pose(&scene),
            Err(HsdDrawWorkError::SceneTopologyMismatch)
        ));
        let mut evaluator =
            HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
        let work = evaluator.evaluate_bind_pose(&scene).unwrap();
        assert_eq!(work.roots[0].packets, expected_packets);
        assert_eq!(work.roots[0].positions, expected_positions);

        // A dispatched source packet must still resolve its rigid target.
        scene.roots[0].joints[1].display_objects[0].polygons[0].binding =
            HsdPolygonBinding::Rigid {
                joint: Some(JObjId(999)),
            };
        assert!(matches!(
            HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD),
            Err(HsdDrawWorkError::MissingRigidJoint {
                polygon_source_id: PObjId(100),
                source_id: JObjId(999),
                ..
            })
        ));
    }
}

#[test]
fn prepared_packets_match_evaluated_shared_nested_and_suppressed_occurrences() {
    let mut scene = instance_scene();
    scene.roots[0].joints[5].flags |= HIDDEN;
    scene.roots[0].joints[2].display_objects[0].polygons[0]
        .decoded
        .vertices
        .clear();
    scene.roots.extend(nested_instance_scene().roots);
    let mut evaluator =
        HsdDrawWorkEvaluator::prepare(&scene, HsdDrawEvaluationPolicy::GENERIC_HSD).unwrap();
    let prepared = evaluator.prepared_packets().collect::<Vec<_>>();
    let work = evaluator.evaluate_bind_pose(&scene).unwrap();
    let evaluated = work
        .roots
        .iter()
        .enumerate()
        .flat_map(|(root_index, root)| {
            root.packets
                .iter()
                .copied()
                .map(move |packet| (root_index, packet))
        })
        .collect::<Vec<_>>();

    assert_eq!(prepared, evaluated);
    assert_eq!(
        prepared
            .iter()
            .map(|(root, packet)| (*root, packet.polygon_source_id.0))
            .collect::<Vec<_>>(),
        [
            (0, 100),
            (0, 104),
            (0, 108),
            (0, 112),
            (0, 100),
            (0, 104),
            (0, 108),
            (1, 100),
            (1, 100),
            (1, 100),
        ]
    );
    assert_eq!(prepared[2].1.vertex_count, 0);
    assert_eq!(prepared[6].1.vertex_count, 0);
    assert_eq!(prepared[7].1.first_vertex, 0);
}
