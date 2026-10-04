use dat_parser::descriptor::animation::{RawFObjTrack, RawFigaTree};
use dat_parser::gx::vertex::{DecodedPrimitive, DecodedVertex};
use dat_parser::hsd::animation::{
    FObjEvaluationError, FObjStreamF32, HsdAObjError, HsdAObjEvaluator, HsdAObjFObj,
    HsdAnimationChannel, HsdJointChannel, HsdJointPoseError, HsdJointPoseEvaluator,
    HsdJointPoseLimits, aobj_flags,
};
use dat_parser::hsd::draw::{
    HsdDrawEvaluationPolicy, HsdDrawWorkError, HsdDrawWorkEvaluator, HsdDrawWorkLimits, HsdRootPose,
};
use dat_parser::hsd::scene::{
    DObjId, HsdDisplayObject, HsdJoint, HsdJointIndex, HsdPolygon, HsdPolygonBinding, HsdScene,
    HsdSceneRoot, HsdTransform, JObjId, PObjId,
};

fn scalar_aobj(packed_data: &[u8], count: usize) -> HsdAObjEvaluator<'_, HsdJointChannel> {
    HsdAObjEvaluator::new(
        0,
        10.0,
        (0..count).map(|_| HsdAObjFObj {
            metadata: HsdJointChannel::Transform(HsdAnimationChannel::TranslationX),
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0x80,
                frac_slope: 0,
                packed_data,
            },
        }),
        count,
    )
    .unwrap()
}

fn scene() -> HsdScene {
    let joints = [
        (JObjId(0x90), [10.0, 20.0, 30.0]),
        (JObjId(0x40), [1.0, 2.0, 3.0]),
        (JObjId(0xc0), [4.0, 5.0, 6.0]),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (source_id, translation))| HsdJoint {
        source_id,
        parent: (index != 0).then_some(HsdJointIndex(0)),
        children: if index == 0 {
            vec![HsdJointIndex(1), HsdJointIndex(2)]
        } else {
            Vec::new()
        },
        flags: 0,
        local: HsdTransform {
            scale: [1.0; 3],
            rotation: [0.0; 3],
            translation,
        },
        inverse_bind_transform: None,
        display_objects: Vec::new(),
    })
    .collect();
    HsdScene {
        roots: vec![HsdSceneRoot {
            source_id: JObjId(0x90),
            name: None,
            joints,
        }],
        textures: Vec::new(),
    }
}

fn track(ordinal: usize, object_type: u8, packed_data: &[u8]) -> RawFObjTrack<'_> {
    RawFObjTrack {
        count_list_ordinal: ordinal,
        descriptor_offset: 0,
        packed_data_offset: 0,
        length: packed_data.len() as u16,
        start_frame: 0,
        object_type,
        frac_value: 0x80,
        frac_slope: 0,
        reserved: 0,
        packed_data,
    }
}

fn tree<'a>(track_counts: Vec<i8>, mut tracks: Vec<RawFObjTrack<'a>>) -> RawFigaTree<'a> {
    for (index, track) in tracks.iter_mut().enumerate() {
        track.descriptor_offset = 0x200 + index as u32 * 0x0c;
        track.packed_data_offset = 0x400 + index as u32 * 0x20;
    }
    RawFigaTree {
        source_offset: 0x100,
        tree_type: 1,
        flags: 0,
        end_frame: 10.0,
        nodes_offset: 0x180,
        tracks_offset: 0x200,
        track_counts,
        tracks,
    }
}

#[test]
fn owned_pose_outlives_sources_and_preserves_local_state_and_joint_clocks() {
    let scene = scene();
    let mut source = vec![0x12, 0, 8, 8];
    let borrowed_tree = tree(vec![1], vec![track(0, 5, &[0x12, 0, 8, 8])]);
    let owned_tree = tree(vec![1], vec![track(0, 5, &source)]);
    let mut borrowed = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &borrowed_tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    let mut converting = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &owned_tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    fn prepare(evaluator: &mut HsdJointPoseEvaluator<'_>, mut local: HsdTransform) {
        local.translation = [100.0, 200.0, 300.0];
        evaluator
            .set_local_transform(HsdJointIndex(0), local)
            .unwrap();
        evaluator.request(1.0).unwrap();
        evaluator.set_rate(0.5).unwrap();
        evaluator.advance().unwrap();
        evaluator.advance().unwrap();
        evaluator.set_updates_suppressed(true).unwrap();
    }
    prepare(&mut borrowed, scene.roots[0].joints[0].local);
    prepare(&mut converting, scene.roots[0].joints[0].local);
    let mut owned = converting.into_owned();
    drop(owned_tree);
    source.fill(0xff);
    drop(source);
    for tick in 0..6 {
        if tick == 1 {
            borrowed.set_updates_suppressed(false).unwrap();
            owned.set_updates_suppressed(false).unwrap();
        }
        if tick == 3 {
            assert_eq!(owned.request(2.5), borrowed.request(2.5),);
            owned = owned.into_owned();
        }
        let actual = owned.advance().unwrap();
        let expected = borrowed.advance().unwrap();
        assert_eq!(actual.root_index, expected.root_index);
        for (actual, expected) in actual.transforms.iter().zip(expected.transforms) {
            assert_eq!(actual.scale, expected.scale);
            assert_eq!(actual.rotation, expected.rotation);
            assert_eq!(actual.translation, expected.translation);
        }
    }
    assert_eq!(owned.stop(), borrowed.stop());
    let mut owned = owned.into_owned();
    assert!(owned.is_stopped());
    assert_eq!(
        owned.advance().unwrap().transforms[1].translation,
        borrowed.advance().unwrap().transforms[1].translation,
    );
}

#[test]
fn packed_budget_counts_duplicate_ranges_before_constructing_owned_pose() {
    let scene = scene();
    let bytes = [0x06, 2];
    let empty = tree(vec![], vec![]);
    let mut tree = tree(vec![2], vec![track(0, 5, &bytes), track(0, 6, &bytes)]);
    tree.tracks[1].packed_data_offset = tree.tracks[0].packed_data_offset;
    let limits = HsdJointPoseLimits {
        max_packed_bytes: 3,
        ..HsdJointPoseLimits::default()
    };
    assert_eq!(
        HsdJointPoseEvaluator::from_figatree(&scene, 0, &tree, &[JObjId(0x40)], limits)
            .unwrap_err(),
        HsdJointPoseError::ResourceLimit {
            resource: "packed bytes",
            limit: 3
        },
    );
    let mut owned = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits {
            max_packed_bytes: 4,
            ..limits
        },
    )
    .unwrap()
    .into_owned();
    owned.request(0.0).unwrap();
    assert_eq!(
        owned.advance().unwrap().transforms[1].translation,
        [2.0, 2.0, 3.0]
    );
    HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &empty,
        &[],
        HsdJointPoseLimits {
            max_packed_bytes: 0,
            ..limits
        },
    )
    .unwrap();
}

#[test]
fn attaching_over_a_joint_frees_its_packed_bytes_and_a_refusal_changes_nothing() {
    let scene = scene();
    let tree = tree(
        vec![1, 1],
        vec![track(0, 5, &[0x06, 2]), track(1, 5, &[0x06, 3])],
    );
    let mut owned = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40), JObjId(0xc0)],
        HsdJointPoseLimits {
            max_packed_bytes: 4,
            ..HsdJointPoseLimits::default()
        },
    )
    .unwrap()
    .into_owned();
    let replacement_bytes = vec![0x06, 9];
    let replacement = scalar_aobj(&replacement_bytes, 1);
    owned
        .attach_joint_animation(HsdJointIndex(1), replacement.into_owned())
        .unwrap();
    drop(replacement_bytes);
    let error = HsdJointPoseError::ResourceLimit {
        resource: "packed bytes",
        limit: 4,
    };
    assert_eq!(
        owned.attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x12, 0, 8, 8], 1)),
        Err(error),
    );
    assert_eq!(
        owned.attach_joint_animation(HsdJointIndex(0), scalar_aobj(&[0x06, 8], 1)),
        Err(error),
    );
    owned.request(0.0).unwrap();
    let pose = owned.advance().unwrap();
    assert_eq!(pose.transforms[0].translation[0], 10.0);
    assert_eq!(pose.transforms[1].translation[0], 9.0);
    assert_eq!(pose.transforms[2].translation[0], 3.0);
}

#[test]
fn explicit_nonordinal_receivers_preserve_local_state_and_last_callback_wins() {
    let mut scene = scene();
    scene.roots[0].joints[2].local.rotation = [0.25, 0.5, 0.75];
    scene.roots[0].joints[2].local.scale = [2.0, 3.0, 4.0];
    let tree = tree(
        vec![2, 0, 1],
        vec![
            track(0, 5, &[0x06, 2]),
            track(0, 5, &[0x06, 8]),
            track(2, 6, &[0x06, 9]),
        ],
    );
    // Count-list order is sibling, root, child, not source-ID or dense order.
    let receivers = [JObjId(0xc0), JObjId(0x90), JObjId(0x40)];
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &receivers,
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    assert!(evaluator.is_stopped());
    let initial = evaluator.advance().unwrap();
    assert_eq!(initial.root_index, 0);
    assert_eq!(initial.transforms[2].translation, [4.0, 5.0, 6.0]);
    assert_eq!(initial.transforms[2].rotation, [0.25, 0.5, 0.75]);
    assert_eq!(initial.transforms[2].scale, [2.0, 3.0, 4.0]);

    let mut placement = scene.roots[0].joints[0].local;
    placement.translation = [100.0, 200.0, 300.0];
    evaluator
        .set_local_transform(HsdJointIndex(0), placement)
        .unwrap();
    evaluator.request(0.0).unwrap();
    let pose = evaluator.advance().unwrap();
    assert_eq!(pose.transforms[0].translation, placement.translation);
    assert_eq!(pose.transforms[1].translation, [1.0, 9.0, 3.0]);
    assert_eq!(pose.transforms[2].translation, [8.0, 5.0, 6.0]);
    assert_eq!(pose.transforms[2].rotation, [0.25, 0.5, 0.75]);
    assert_eq!(pose.transforms[2].scale, [2.0, 3.0, 4.0]);

    let mut caller_local = pose.transforms[2];
    caller_local.translation = [42.0, 55.0, 66.0];
    evaluator
        .set_local_transform(HsdJointIndex(2), caller_local)
        .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(
        evaluator.pose().unwrap().transforms[2].translation,
        [42.0, 55.0, 66.0]
    );
    let replay = evaluator.advance().unwrap();
    assert_eq!(replay.transforms[0].translation, placement.translation);
    assert_eq!(replay.transforms[2].translation, [8.0, 55.0, 66.0]);
    assert_eq!(replay.transforms[2].rotation, caller_local.rotation);
    assert_eq!(replay.transforms[2].scale, caller_local.scale);
}

#[test]
fn all_scalar_receivers_apply_and_scale_clamping_keeps_source_boundary() {
    let scene = scene();
    for scales in [[0.0_f32, -0.0005, -0.001], [0.001, 0.0005, -2.0]] {
        let values = [
            0.25, 0.5, 0.75, 3.0, 4.0, 5.0, scales[0], scales[1], scales[2],
        ];
        let packed: Vec<Vec<u8>> = values
            .into_iter()
            .map(|value| {
                let mut bytes = vec![0x06];
                bytes.extend_from_slice(&value.to_le_bytes());
                bytes
            })
            .collect();
        let tracks = [1, 2, 3, 5, 6, 7, 8, 9, 10]
            .into_iter()
            .zip(&packed)
            .map(|(channel, bytes)| RawFObjTrack {
                frac_value: 0,
                ..track(0, channel, bytes)
            })
            .collect();
        let tree = tree(vec![9], tracks);
        let mut evaluator = HsdJointPoseEvaluator::from_figatree(
            &scene,
            0,
            &tree,
            &[JObjId(0x40)],
            HsdJointPoseLimits::default(),
        )
        .unwrap();
        evaluator.request(0.0).unwrap();
        let pose = evaluator.advance().unwrap();
        assert_eq!(pose.transforms[1].rotation, [0.25, 0.5, 0.75]);
        assert_eq!(pose.transforms[1].translation, [3.0, 4.0, 5.0]);
        let expected = if scales[0] == 0.0 {
            [0.001, 0.001, -0.001]
        } else {
            [0.001, 0.001, -2.0]
        };
        assert_eq!(pose.transforms[1].scale, expected);
        assert_eq!(pose.transforms[0].scale, [1.0; 3]);
    }
}

#[test]
fn tick_seek_rate_loop_and_suppression_share_the_aobj_lifecycle() {
    let scene = scene();
    let mut tree = tree(vec![1], vec![track(0, 5, &[0x12, 0, 4, 4])]);
    tree.flags = aobj_flags::LOOP;
    tree.end_frame = 2.0;
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    for expected in [0.0, 1.0, 0.0] {
        assert_eq!(
            evaluator.advance().unwrap().transforms[1].translation[0],
            expected
        );
        assert!(!evaluator.is_stopped());
    }
    evaluator.set_rate(0.5).unwrap();
    evaluator.request(0.5).unwrap();
    // FIRST_PLAY samples the requested frame, not requested frame plus rate.
    for expected in [0.5, 1.0] {
        assert_eq!(
            evaluator.advance().unwrap().transforms[1].translation[0],
            expected
        );
    }
    evaluator.set_updates_suppressed(true).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[0],
        1.0
    );
    evaluator.set_updates_suppressed(false).unwrap();
    // The suppressed tick advanced time to 1.5; the next tick rewinds at 2.
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[0],
        0.0
    );
    evaluator.stop().unwrap();
    assert!(evaluator.is_stopped());
    let stopped = evaluator.pose().unwrap().transforms[1].translation;
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation,
        stopped
    );
    evaluator.request(1.0).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[0],
        1.0
    );
}

#[test]
fn signed_start_delay_and_stop_flush_pending_keys_even_when_suppressed() {
    let scene = scene();
    let mut delayed = tree(vec![1], vec![track(0, 6, &[0x06, 9])]);
    delayed.tracks[0].start_frame = (-2_i16) as u16;
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &delayed,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    evaluator.set_rate(100.0).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[1],
        2.0
    );
    evaluator.set_rate(1.0).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[1],
        2.0
    );
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[1],
        9.0
    );

    let mut keys = tree(vec![1], vec![track(0, 5, &[0x16, 2, 2, 8])]);
    keys.flags = aobj_flags::NO_UPDATE;
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &keys,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[0],
        1.0
    );
    evaluator.set_rate(2.0).unwrap();
    evaluator.stop().unwrap();
    assert!(evaluator.is_stopped());
    assert_eq!(evaluator.pose().unwrap().transforms[1].translation[0], 8.0);
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation[0],
        8.0
    );
}

#[test]
fn malformed_later_tracks_poison_every_pose_and_mutation_entry_point() {
    let scene = scene();
    for suppressed in [false, true] {
        let tree = tree(vec![2], vec![track(0, 5, &[0x06, 8]), track(0, 6, &[0x01])]);
        let mut evaluator = HsdJointPoseEvaluator::from_figatree(
            &scene,
            0,
            &tree,
            &[JObjId(0x40)],
            HsdJointPoseLimits::default(),
        )
        .unwrap();
        evaluator.request(0.0).unwrap();
        evaluator.set_updates_suppressed(suppressed).unwrap();
        let expected = HsdJointPoseError::Playback {
            joint_index: HsdJointIndex(1),
            source: HsdAObjError::FObj {
                track: 1,
                source: FObjEvaluationError::UnexpectedEnd,
            },
        };
        assert_eq!(evaluator.advance().unwrap_err(), expected);
        assert_eq!(evaluator.pose().unwrap_err(), expected);
        assert_eq!(evaluator.advance().unwrap_err(), expected);
        assert_eq!(evaluator.request(0.0), Err(expected));
        assert_eq!(
            evaluator.attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x06, 9], 1)),
            Err(expected),
        );
        assert_eq!(evaluator.set_rate(1.0), Err(expected));
        assert_eq!(evaluator.set_updates_suppressed(false), Err(expected));
        assert_eq!(evaluator.stop(), Err(expected));
        assert_eq!(
            evaluator.set_local_transform(HsdJointIndex(0), scene.roots[0].joints[0].local),
            Err(expected),
        );
    }

    let mut nonfinite_key = vec![0x16];
    nonfinite_key.extend_from_slice(&1.0_f32.to_le_bytes());
    nonfinite_key.push(2);
    nonfinite_key.extend_from_slice(&f32::NAN.to_le_bytes());
    let failing_stop = tree(
        vec![2],
        vec![
            track(0, 5, &[0x16, 2, 2, 8]),
            RawFObjTrack {
                frac_value: 0,
                ..track(0, 6, &nonfinite_key)
            },
        ],
    );
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &failing_stop,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(
        evaluator.advance().unwrap().transforms[1].translation,
        [2.0, 1.0, 3.0]
    );
    evaluator.set_rate(2.0).unwrap();
    let error = HsdJointPoseError::Playback {
        joint_index: HsdJointIndex(1),
        source: HsdAObjError::FObj {
            track: 1,
            source: FObjEvaluationError::NonFiniteSample,
        },
    };
    assert_eq!(evaluator.stop(), Err(error));
    assert_eq!(evaluator.pose().unwrap_err(), error);
    assert_eq!(evaluator.advance().unwrap_err(), error);
    assert_eq!(evaluator.request(0.0), Err(error));

    // AObjs visit scene depth-first order, even when attachment order differs.
    let tree = tree(
        vec![1, 1, 1],
        vec![
            track(0, 5, &[0x01]),
            track(1, 5, &[0x06, 8]),
            track(2, 6, &[0x01]),
        ],
    );
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0xc0), JObjId(0x90), JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    let error = evaluator.advance().unwrap_err();
    assert_eq!(
        error,
        HsdJointPoseError::Playback {
            joint_index: HsdJointIndex(1),
            source: HsdAObjError::FObj {
                track: 0,
                source: FObjEvaluationError::UnexpectedEnd
            },
        }
    );
    assert_eq!(evaluator.pose().unwrap_err(), error);
}

#[test]
fn malformed_structure_identity_and_unsupported_channels_fail_before_playback() {
    let scene = scene();
    let cases = [
        (
            tree(vec![1], vec![track(0, 5, &[0x01])]),
            vec![],
            HsdJointPoseError::ReceiverCountMismatch {
                expected: 1,
                actual: 0,
            },
        ),
        (
            tree(vec![0], vec![]),
            vec![JObjId(0xff)],
            HsdJointPoseError::MissingReceiver {
                source_id: JObjId(0xff),
            },
        ),
        (
            tree(vec![0, 0], vec![]),
            vec![JObjId(0x40), JObjId(0x40)],
            HsdJointPoseError::DuplicateReceiver {
                source_id: JObjId(0x40),
            },
        ),
        (
            tree(vec![-2], vec![]),
            vec![JObjId(0x40)],
            HsdJointPoseError::InvalidTrackLayout { ordinal: 0 },
        ),
        (
            tree(vec![2], vec![track(0, 5, &[0x01])]),
            vec![JObjId(0x40)],
            HsdJointPoseError::InvalidTrackLayout { ordinal: 0 },
        ),
        (
            tree(vec![0], vec![track(0, 5, &[0x01])]),
            vec![JObjId(0x40)],
            HsdJointPoseError::InvalidTrackLayout { ordinal: 1 },
        ),
        (
            tree(vec![1], vec![track(1, 5, &[0x01])]),
            vec![JObjId(0x40)],
            HsdJointPoseError::InvalidTrackLayout { ordinal: 0 },
        ),
        (
            tree(
                vec![1],
                vec![RawFObjTrack {
                    length: 2,
                    ..track(0, 5, &[0x01])
                }],
            ),
            vec![JObjId(0x40)],
            HsdJointPoseError::PackedLengthMismatch {
                descriptor: 0x200,
                declared: 2,
                actual: 1,
            },
        ),
    ];
    for (tree, receivers, expected) in cases {
        assert_eq!(
            HsdJointPoseEvaluator::from_figatree(
                &scene,
                0,
                &tree,
                &receivers,
                HsdJointPoseLimits::default()
            )
            .unwrap_err(),
            expected,
        );
    }
    for object_type in [0, 4, 13, 20, 255] {
        let tree = tree(vec![1], vec![track(0, object_type, &[0x01])]);
        assert_eq!(
            HsdJointPoseEvaluator::from_figatree(
                &scene,
                0,
                &tree,
                &[JObjId(0x40)],
                HsdJointPoseLimits::default()
            )
            .unwrap_err(),
            HsdJointPoseError::UnsupportedChannel {
                descriptor: 0x200,
                object_type
            },
        );
    }
    let tree = tree(vec![], vec![]);
    let mut ambiguous = scene;
    ambiguous.roots[0].joints[2].source_id = JObjId(0x40);
    assert_eq!(
        HsdJointPoseEvaluator::from_figatree(
            &ambiguous,
            0,
            &tree,
            &[],
            HsdJointPoseLimits::default()
        )
        .unwrap_err(),
        HsdJointPoseError::AmbiguousJoint {
            source_id: JObjId(0x40)
        },
    );
}

#[test]
fn budgets_and_nonfinite_inputs_reject_without_invalidating_a_valid_attachment() {
    let mut scene = scene();
    let mut tree = tree(vec![1], vec![track(0, 5, &[0x01])]);
    for (limits, expected) in [
        (
            HsdJointPoseLimits {
                max_joints: 2,
                max_tracks: 1,
                ..HsdJointPoseLimits::default()
            },
            HsdJointPoseError::ResourceLimit {
                resource: "joints",
                limit: 2,
            },
        ),
        (
            HsdJointPoseLimits {
                max_joints: 3,
                max_tracks: 0,
                ..HsdJointPoseLimits::default()
            },
            HsdJointPoseError::ResourceLimit {
                resource: "tracks",
                limit: 0,
            },
        ),
    ] {
        assert_eq!(
            HsdJointPoseEvaluator::from_figatree(&scene, 0, &tree, &[JObjId(0x40)], limits)
                .unwrap_err(),
            expected,
        );
    }
    assert_eq!(
        HsdJointPoseEvaluator::from_figatree(
            &scene,
            1,
            &tree,
            &[JObjId(0x40)],
            HsdJointPoseLimits::default()
        )
        .unwrap_err(),
        HsdJointPoseError::RootOutOfRange { root_index: 1 },
    );
    for end_frame in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        tree.end_frame = end_frame;
        assert_eq!(
            HsdJointPoseEvaluator::from_figatree(
                &scene,
                0,
                &tree,
                &[JObjId(0x40)],
                HsdJointPoseLimits::default()
            )
            .unwrap_err(),
            HsdJointPoseError::NonFiniteEndFrame,
        );
    }
    tree.end_frame = 10.0;
    scene.roots[0].joints[2].local.rotation[0] = f32::NAN;
    assert_eq!(
        HsdJointPoseEvaluator::from_figatree(
            &scene,
            0,
            &tree,
            &[JObjId(0x40)],
            HsdJointPoseLimits::default()
        )
        .unwrap_err(),
        HsdJointPoseError::NonFiniteTransform {
            joint_index: HsdJointIndex(2)
        },
    );
    scene.roots[0].joints[2].local.rotation[0] = 0.0;
    tree.tracks[0] = track(0, 5, &[0x12, 0, 4, 4]);
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits {
            max_joints: 3,
            max_tracks: 1,
            ..HsdJointPoseLimits::default()
        },
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    evaluator.advance().unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            evaluator.request(value),
            Err(HsdJointPoseError::NonFiniteRequestFrame)
        );
        assert_eq!(
            evaluator.set_rate(value),
            Err(HsdJointPoseError::NonFiniteRate)
        );
        let mut invalid = scene.roots[0].joints[0].local;
        invalid.translation[0] = value;
        assert_eq!(
            evaluator.set_local_transform(HsdJointIndex(0), invalid),
            Err(HsdJointPoseError::NonFiniteTransform {
                joint_index: HsdJointIndex(0)
            }),
        );
    }
    assert_eq!(
        evaluator.set_local_transform(HsdJointIndex(3), scene.roots[0].joints[0].local),
        Err(HsdJointPoseError::JointOutOfRange {
            joint_index: HsdJointIndex(3)
        }),
    );
    let pose = evaluator.advance().unwrap();
    assert_eq!(pose.transforms[1].translation, [1.0, 2.0, 3.0]);
    assert_eq!(pose.transforms[0].translation, [10.0, 20.0, 30.0]);
}

#[test]
fn a_newly_animated_joint_runs_in_scene_order() {
    let scene = scene();
    let tree = tree(vec![1], vec![track(0, 5, &[0x01])]);
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0xc0)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator
        .attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x01], 1))
        .unwrap();
    evaluator.request(0.0).unwrap();
    let error = HsdJointPoseError::Playback {
        joint_index: HsdJointIndex(1),
        source: HsdAObjError::FObj {
            track: 0,
            source: FObjEvaluationError::UnexpectedEnd,
        },
    };
    // Appending instead of ordered insertion would fail on sibling joint 2.
    assert_eq!(evaluator.advance().unwrap_err(), error);
    assert_eq!(evaluator.pose().unwrap_err(), error);
}

#[test]
fn attaching_over_a_joint_frees_its_tracks_and_a_refusal_changes_nothing() {
    let scene = scene();
    let tree = tree(
        vec![2, 1],
        vec![
            track(0, 5, &[0x06, 2]),
            track(0, 5, &[0x06, 3]),
            track(1, 5, &[0x12, 0, 8, 8]),
        ],
    );
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40), JObjId(0xc0)],
        HsdJointPoseLimits {
            max_joints: 3,
            max_tracks: 3,
            ..HsdJointPoseLimits::default()
        },
    )
    .unwrap();
    // Replacing two tracks with two tracks is legal at the aggregate limit.
    evaluator
        .attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x06, 8], 2))
        .unwrap();
    let budget_error = HsdJointPoseError::ResourceLimit {
        resource: "tracks",
        limit: 3,
    };
    assert_eq!(
        evaluator.attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x06, 9], 3)),
        Err(budget_error),
    );
    assert_eq!(
        evaluator.attach_joint_animation(HsdJointIndex(0), scalar_aobj(&[0x06, 9], 1)),
        Err(budget_error),
    );
    assert_eq!(
        evaluator.attach_joint_animation(HsdJointIndex(3), scalar_aobj(&[0x06, 9], 1)),
        Err(HsdJointPoseError::JointOutOfRange {
            joint_index: HsdJointIndex(3)
        }),
    );
    evaluator.request(0.0).unwrap();
    let pose = evaluator.advance().unwrap();
    assert_eq!(pose.transforms[0].translation[0], 10.0);
    assert_eq!(pose.transforms[1].translation[0], 8.0);
    assert_eq!(pose.transforms[2].translation[0], 0.0);
    assert_eq!(
        evaluator.advance().unwrap().transforms[2].translation[0],
        1.0
    );
    // Reducing an existing controller releases room for a new joint controller.
    evaluator
        .attach_joint_animation(HsdJointIndex(1), scalar_aobj(&[0x06, 7], 1))
        .unwrap();
    evaluator
        .attach_joint_animation(HsdJointIndex(0), scalar_aobj(&[0x06, 6], 1))
        .unwrap();
    evaluator.request(0.0).unwrap();
    let pose = evaluator.advance().unwrap();
    assert_eq!(pose.transforms[0].translation[0], 6.0);
    assert_eq!(pose.transforms[1].translation[0], 7.0);
    assert_eq!(pose.transforms[2].translation[0], 0.0);
}

use dat_parser::descriptor::jobj::flags::{HIDDEN as JOBJ_HIDDEN, INSTANCE as JOBJ_INSTANCE};

fn hidden_after(evaluator: &mut HsdJointPoseEvaluator<'_>) -> Vec<bool> {
    evaluator
        .advance()
        .unwrap()
        .hidden_joints
        .expect("Figa poses carry runtime visibility")
        .to_vec()
}

#[test]
fn node_visibility_thresholds_strictly_above_one_half() {
    let scene = scene();
    // LIN 0 -> 1 over four frames: 0, 0.25, 0.5 hide; 0.75 and 1 show.
    let tree = tree(vec![1], vec![track(0, 11, &[0x12, 0, 4, 1])]);
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    let states = (0..5)
        .map(|_| hidden_after(&mut evaluator)[1])
        .collect::<Vec<_>>();
    assert_eq!(states, [true, true, true, false, false]);
}

#[test]
fn visibility_seeds_from_serialized_flags_and_persists_between_updates() {
    let mut scene = scene();
    scene.roots[0].joints[2].flags = JOBJ_HIDDEN;
    // One CON sample showing 0x40, then the stream ends and the state is kept.
    let tree = tree(vec![1], vec![track(0, 11, &[0x06, 1, 1])]);
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    assert_eq!(
        evaluator.pose().unwrap().hidden_joints,
        Some(&[false, false, true][..])
    );
    evaluator.request(0.0).unwrap();
    for _ in 0..4 {
        assert_eq!(hidden_after(&mut evaluator), [false, false, true]);
    }
}

#[test]
fn branch_recurses_parent_first_and_later_node_updates_override_it() {
    let scene = scene();
    // Root BRANCH hides the subtree; the child's NODE runs afterwards and shows it.
    let tree = tree(
        vec![1, 0, 1],
        vec![track(0, 12, &[0x06, 0]), track(2, 11, &[0x06, 1])],
    );
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x90), JObjId(0x40), JObjId(0xc0)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(hidden_after(&mut evaluator), [true, true, false]);
}

#[test]
fn only_the_first_branch_moves_ahead_of_earlier_descriptors() {
    let scene = scene();
    let tree = tree(
        vec![0, 3, 2],
        vec![
            // 0x40: BRANCH(show), NODE(show), BRANCH(hide) keeps its order, so the
            // second BRANCH writes last. Moving every BRANCH would let NODE win.
            track(1, 12, &[0x06, 1]),
            track(1, 11, &[0x06, 1]),
            track(1, 12, &[0x06, 0]),
            // 0xc0: NODE(show), BRANCH(hide) runs BRANCH first, so NODE wins.
            track(2, 11, &[0x06, 1]),
            track(2, 12, &[0x06, 0]),
        ],
    );
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x90), JObjId(0x40), JObjId(0xc0)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(hidden_after(&mut evaluator), [false, true, false]);
}

#[test]
fn branch_flags_an_instance_without_descending_to_its_target() {
    let mut scene = scene();
    // 0x40 becomes an INSTANCE of 0xc0; the target keeps its owned parent.
    scene.roots[0].joints[1].flags = JOBJ_INSTANCE;
    scene.roots[0].joints[1].children = vec![HsdJointIndex(2)];
    let tree = tree(vec![1], vec![track(0, 12, &[0x06, 0])]);
    let mut evaluator = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    assert_eq!(hidden_after(&mut evaluator), [false, true, false]);
}

fn triangle(display_object: u32, polygon: u32) -> HsdDisplayObject {
    HsdDisplayObject {
        source_id: DObjId(display_object),
        material: None,
        polygons: vec![HsdPolygon {
            source_id: PObjId(polygon),
            flags: 0,
            attributes: Vec::new(),
            primitive_groups: Vec::new(),
            decoded: DecodedPrimitive {
                vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
                    .into_iter()
                    .map(|position| DecodedVertex {
                        position,
                        normal: [0.0, 0.0, 1.0],
                        ..Default::default()
                    })
                    .collect(),
                triangles: vec![[0, 1, 2]],
            },
            binding: HsdPolygonBinding::Rigid { joint: None },
        }],
    }
}

#[test]
fn hidden_joints_keep_packet_ranges_and_report_per_frame_visibility() {
    let mut scene = scene();
    scene.roots[0].joints[1]
        .display_objects
        .push(triangle(0x500, 0x600));
    scene.roots[0].joints[2]
        .display_objects
        .push(triangle(0x510, 0x610));
    scene.roots[0].joints[2].flags = JOBJ_HIDDEN;
    let mut draw = HsdDrawWorkEvaluator::prepare_with_limits(
        &scene,
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        HsdDrawWorkLimits::default(),
    )
    .unwrap();

    // Bind pose honors the serialized flag, as HSD_JObjDispDObj does.
    let work = draw.evaluate_bind_pose(&scene).unwrap();
    let visible = |work: &dat_parser::hsd::draw::HsdEvaluatedDrawWork| {
        work.roots[0]
            .packets
            .iter()
            .map(|packet| (packet.polygon_source_id, packet.visible))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        visible(work),
        [(PObjId(0x600), true), (PObjId(0x610), false)]
    );

    // 0x40 hides for three frames, then shows; ranges and positions stay put.
    let tree = tree(vec![1], vec![track(0, 11, &[0x12, 0, 4, 1])]);
    let mut pose = HsdJointPoseEvaluator::from_figatree(
        &scene,
        0,
        &tree,
        &[JObjId(0x40)],
        HsdJointPoseLimits::default(),
    )
    .unwrap();
    pose.request(0.0).unwrap();
    for shown in [false, false, false, true] {
        let work = draw.evaluate(&scene, &[pose.advance().unwrap()]).unwrap();
        assert_eq!(
            visible(work),
            [(PObjId(0x600), shown), (PObjId(0x610), false)]
        );
        let root = &work.roots[0];
        assert_eq!(root.positions.len(), 6);
        assert_eq!(
            (root.packets[1].first_vertex, root.packets[1].vertex_count),
            (3, 3)
        );
    }
}

#[test]
fn runtime_visibility_must_cover_the_root_and_leave_instances_unchanged() {
    let mut scene = scene();
    scene.roots[0].joints[1].flags = JOBJ_INSTANCE;
    scene.roots[0].joints[1].children = vec![HsdJointIndex(2)];
    let mut draw = HsdDrawWorkEvaluator::prepare_with_limits(
        &scene,
        HsdDrawEvaluationPolicy::GENERIC_HSD,
        HsdDrawWorkLimits::default(),
    )
    .unwrap();
    let transforms = scene.roots[0]
        .joints
        .iter()
        .map(|joint| joint.local)
        .collect::<Vec<_>>();
    let pose = |hidden: &'static [bool]| HsdRootPose {
        root_index: 0,
        transforms: &transforms,
        hidden_joints: Some(hidden),
        constraints: &[],
    };
    assert!(matches!(
        draw.evaluate(&scene, &[pose(&[false, false])]),
        Err(HsdDrawWorkError::PoseVisibilityCountMismatch {
            root_index: 0,
            expected: 3,
            actual: 2,
        })
    ));
    assert!(matches!(
        draw.evaluate(&scene, &[pose(&[false, true, false])]),
        Err(HsdDrawWorkError::RuntimeInstanceVisibility {
            root_index: 0,
            joint_index: HsdJointIndex(1),
        })
    ));
    // A non-instance joint may change freely.
    draw.evaluate(&scene, &[pose(&[true, false, true])])
        .unwrap();
}

/// A BRANCH track walks a joint's owned children, so a caller-built scene
/// whose children loop is refused before any walk.
#[test]
fn a_root_whose_children_cycle_is_refused() {
    let mut scene = scene();
    scene.roots[0].joints[1].children = vec![HsdJointIndex(1)];
    let limits = HsdJointPoseLimits::default();
    assert!(matches!(
        HsdJointPoseEvaluator::unanimated(&scene, 0, limits),
        Err(HsdJointPoseError::InvalidScene { root_index: 0, .. })
    ));
    let tree = tree(vec![1], vec![track(0, 5, &[0x06, 2])]);
    assert!(matches!(
        HsdJointPoseEvaluator::from_figatree(&scene, 0, &tree, &[JObjId(0x40)], limits),
        Err(HsdJointPoseError::InvalidScene { root_index: 0, .. })
    ));
}
