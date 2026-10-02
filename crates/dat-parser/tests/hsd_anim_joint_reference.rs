use dat_parser::descriptor::animation::{RawFObjTrack, RawFigaTree};
use dat_parser::descriptor::aobj::{RawAObjDesc, RawGenericFObjDesc};
use dat_parser::descriptor::generic_animation::{RawAnimJoint, RawAnimJointGraph};
use dat_parser::hsd::animation::{
    HsdJointPoseEvaluator, HsdJointPoseLimits, aobj_flags, attach_anim_joints,
};
use dat_parser::hsd::scene::{
    HsdJoint, HsdJointIndex, HsdScene, HsdSceneRoot, HsdTransform, JObjId,
};

/// One translation-X segment; the Figa reference tests use the same bytes.
const PACKED: [u8; 4] = [0x12, 0, 8, 8];
const END_FRAME: f32 = 10.0;

/// A root with two children: 0x40 at position [0, 0], 0xC0 at [0, 1].
fn scene() -> HsdScene {
    let joints = [JObjId(0x90), JObjId(0x40), JObjId(0xc0)]
        .into_iter()
        .enumerate()
        .map(|(index, source_id)| HsdJoint {
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
                translation: [index as f32; 3],
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

fn fobj(object_type: u8) -> RawGenericFObjDesc<'static> {
    RawGenericFObjDesc {
        source_offset: 0x300,
        next_offset: None,
        packed_data_offset: Some(0x400),
        length: PACKED.len() as u32,
        start_frame_bits: 0f32.to_bits(),
        object_type,
        frac_value: 0x80,
        frac_slope: 0,
        reserved: 0,
        packed_data: &PACKED,
    }
}

fn anim_joint(position: &[usize], object_types: &[u8]) -> RawAnimJoint<'static> {
    RawAnimJoint {
        source_offset: 0x200,
        position: position.to_vec(),
        child_offset: None,
        next_offset: None,
        aobj: (!object_types.is_empty()).then(|| RawAObjDesc {
            source_offset: 0x280,
            raw_flags: 0,
            end_frame_bits: END_FRAME.to_bits(),
            obj_id: None,
            fobjs: object_types.iter().copied().map(fobj).collect(),
        }),
        raw_flags: 0,
    }
}

/// The same track bound to `joint` as a Figa tree.
fn figa(scene: &HsdScene, joint: JObjId) -> HsdJointPoseEvaluator<'static> {
    let tree = RawFigaTree {
        source_offset: 0x100,
        tree_type: 1,
        flags: 0,
        end_frame: END_FRAME,
        nodes_offset: 0x180,
        tracks_offset: 0x200,
        track_counts: vec![1],
        tracks: vec![RawFObjTrack {
            count_list_ordinal: 0,
            descriptor_offset: 0x200,
            packed_data_offset: 0x400,
            length: PACKED.len() as u16,
            start_frame: 0,
            object_type: 5,
            frac_value: 0x80,
            frac_slope: 0,
            reserved: 0,
            packed_data: &PACKED,
        }],
    };
    HsdJointPoseEvaluator::from_figatree(scene, 0, &tree, &[joint], HsdJointPoseLimits::default())
        .unwrap()
        .into_owned()
}

#[test]
fn anim_joints_pair_with_model_joints_by_tree_position() {
    let scene = scene();
    let graph = RawAnimJointGraph {
        root_offset: 0x200,
        joints: vec![
            // The root and its first child carry no AObj.
            anim_joint(&[0], &[]),
            anim_joint(&[0, 0], &[]),
            // The second child: translation X, and a PATH track (4) that is
            // left out rather than refusing the joint.
            anim_joint(&[0, 1], &[4, 5]),
            // Past the model's children, and beside its root: never visited.
            anim_joint(&[0, 2], &[5]),
            anim_joint(&[1], &[5]),
        ],
    };
    let mut attached =
        attach_anim_joints(&scene, 0, &graph, HsdJointPoseLimits::default()).unwrap();
    assert!(attached.is_animated());
    assert_eq!(attached.end_frame(), END_FRAME);

    let mut reference = figa(&scene, JObjId(0xc0));
    attached.request(0.0).unwrap();
    reference.request(0.0).unwrap();
    let mut moved = false;
    for _ in 0..6 {
        let actual = attached.advance().unwrap();
        let expected = reference.advance().unwrap();
        for (actual, expected) in actual.transforms.iter().zip(expected.transforms) {
            assert_eq!(actual.translation, expected.translation);
        }
        moved |= actual.transforms[2].translation[0] != 2.0;
        // Only the paired joint moves.
        assert_eq!(actual.transforms[1].translation, [1.0; 3]);
    }
    assert!(moved, "the track drives its joint");
}

#[test]
fn a_tree_without_tracks_leaves_the_root_unanimated() {
    let scene = scene();
    let graph = RawAnimJointGraph {
        root_offset: 0x200,
        // Only tracks this layer does not play: PATH and a user byte.
        joints: vec![anim_joint(&[0], &[4, 20])],
    };
    let attached = attach_anim_joints(&scene, 0, &graph, HsdJointPoseLimits::default()).unwrap();
    assert!(!attached.is_animated());
    assert_eq!(attached.end_frame(), 0.0);
}

#[test]
fn looping_keeps_a_finished_animation_running() {
    let scene = scene();
    let graph = RawAnimJointGraph {
        root_offset: 0x200,
        joints: vec![anim_joint(&[0], &[5])],
    };
    let limits = HsdJointPoseLimits::default();
    let mut once = attach_anim_joints(&scene, 0, &graph, limits).unwrap();
    let mut looped = attach_anim_joints(&scene, 0, &graph, limits).unwrap();
    looped.set_looping(true);
    assert_eq!(aobj_flags::LOOP, 1 << 29);
    for pose in [&mut once, &mut looped] {
        pose.request(0.0).unwrap();
        for _ in 0..=(END_FRAME as usize + 2) {
            pose.advance().unwrap();
        }
    }
    assert!(once.is_stopped());
    assert!(!looped.is_stopped());
}
