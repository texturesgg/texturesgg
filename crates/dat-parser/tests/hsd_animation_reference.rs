use dat_parser::DatFile;
use dat_parser::descriptor::animation::{RawFigaTree, RawFigaTreeError};
use dat_parser::hsd::animation::{
    FObjEvaluationError, FObjEvaluator, FObjStream, FObjStreamF32, HsdAObjError, HsdAObjEvaluator,
    HsdAObjFObj, HsdAnimationChannel, aobj_flags, sample_fobj_integer_frames,
};

fn stream<'a>(
    packed_data: &'a [u8],
    start_frame: i16,
    frac_value: u8,
    frac_slope: u8,
) -> FObjStream<'a> {
    FObjStream {
        start_frame,
        frac_value,
        frac_slope,
        packed_data,
    }
}

fn packed_u8(packed_data: &[u8]) -> FObjStream<'_> {
    stream(packed_data, 0, 0x80, 0x80)
}

fn synthetic_figa_raw(
    end_frame: f32,
    node_track_count: u8,
    object_type: u8,
    reserved: u8,
) -> Vec<u8> {
    const DATA_SIZE: usize = 0x34;
    const RELOCATION_COUNT: usize = 3;
    let symbol = b"synthetic_figatree\0";
    let mut raw = vec![0u8; 0x20 + DATA_SIZE + RELOCATION_COUNT * 4 + 8 + symbol.len()];
    let raw_len = raw.len() as u32;
    raw[0..4].copy_from_slice(&raw_len.to_be_bytes());
    raw[4..8].copy_from_slice(&(DATA_SIZE as u32).to_be_bytes());
    raw[8..12].copy_from_slice(&(RELOCATION_COUNT as u32).to_be_bytes());
    raw[12..16].copy_from_slice(&1u32.to_be_bytes());

    let data = 0x20;
    // The FObj packed-data pointer is relocated and deliberately targets data
    // offset zero, where this linear 0 -> 10 stream lives.
    raw[data..data + 4].copy_from_slice(&[0x12, 0, 10, 10]);
    raw[data + 0x10..data + 0x12].copy_from_slice(&4u16.to_be_bytes());
    raw[data + 0x14] = object_type;
    raw[data + 0x15] = 0x80;
    raw[data + 0x16] = 0x20;
    raw[data + 0x17] = reserved;
    raw[data + 0x1c..data + 0x1e].copy_from_slice(&[node_track_count, u8::MAX]);
    raw[data + 0x20..data + 0x24].copy_from_slice(&0x1122_3344u32.to_be_bytes());
    raw[data + 0x24..data + 0x28].copy_from_slice(&0x5566_7788u32.to_be_bytes());
    raw[data + 0x28..data + 0x2c].copy_from_slice(&end_frame.to_bits().to_be_bytes());
    raw[data + 0x2c..data + 0x30].copy_from_slice(&0x1cu32.to_be_bytes());
    raw[data + 0x30..data + 0x34].copy_from_slice(&0x10u32.to_be_bytes());

    let relocations = 0x20 + DATA_SIZE;
    for (index, field) in [0x18u32, 0x2c, 0x30].into_iter().enumerate() {
        let offset = relocations + index * 4;
        raw[offset..offset + 4].copy_from_slice(&field.to_be_bytes());
    }
    let root = relocations + RELOCATION_COUNT * 4;
    raw[root..root + 4].copy_from_slice(&0x20u32.to_be_bytes());
    raw[root + 8..root + 8 + symbol.len()].copy_from_slice(symbol);
    raw
}

#[test]
fn raw_figa_preserves_descriptor_values_offsets_and_relocated_zero() {
    let mut raw = synthetic_figa_raw(10.0, 1, 0x7f, 0xa5);
    raw[0x32..0x34].copy_from_slice(&0xff4cu16.to_be_bytes());
    let dat = DatFile::parse(&raw).expect("synthetic DAT");
    let root = &dat.roots[0];
    let tree = RawFigaTree::parse(&dat, root.data_offset).expect("raw FigaTree");

    assert_eq!(tree.source_offset, 0x20);
    assert_eq!(tree.tree_type, 0x1122_3344);
    assert_eq!(tree.flags, 0x5566_7788);
    assert_eq!(tree.end_frame, 10.0);
    assert_eq!(tree.nodes_offset, 0x1c);
    assert_eq!(tree.tracks_offset, 0x10);
    assert_eq!(tree.track_counts, [1]);
    assert_eq!(tree.tracks.len(), 1);
    let track = tree.tracks[0];
    assert_eq!(track.count_list_ordinal, 0);
    assert_eq!(track.descriptor_offset, 0x10);
    assert_eq!(track.packed_data_offset, 0);
    assert_eq!(track.length, 4);
    assert_eq!(track.start_frame, 0xff4c);
    assert_eq!(track.object_type, 0x7f);
    assert_eq!(track.frac_value, 0x80);
    assert_eq!(track.frac_slope, 0x20);
    assert_eq!(track.reserved, 0xa5);
    assert_eq!(track.packed_data, [0x12, 0, 10, 10]);
    assert_eq!(
        HsdAnimationChannel::from_joint_object_type(track.object_type),
        None
    );
}

#[test]
fn raw_figa_rejects_negative_nonterminator_count_without_losing_value() {
    let dat = DatFile::parse(&synthetic_figa_raw(10.0, 0xfe, 1, 0)).expect("synthetic DAT");
    assert!(matches!(
        RawFigaTree::parse(&dat, dat.roots[0].data_offset),
        Err(RawFigaTreeError::NegativeTrackCount {
            count_list_ordinal: 0,
            value: -2,
        })
    ));
}

#[test]
fn decodes_every_defined_scalar_storage_format_as_little_endian() {
    let cases: &[(u8, &[u8], f32)] = &[
        (0x00, &1.5f32.to_le_bytes(), 1.5),
        (0x21, &(-6i16).to_le_bytes(), -3.0),
        (0x42, &(20u16).to_le_bytes(), 5.0),
        (0x61, &[0xf8], -4.0),
        (0x83, &[40], 5.0),
        (0x9f, &[1], -1.0 / 2_147_483_648.0),
    ];
    for &(fraction, encoded, expected) in cases {
        let mut packed = vec![0x06];
        packed.extend_from_slice(encoded);
        assert_eq!(
            sample_fobj_integer_frames(stream(&packed, 0, fraction, fraction), 1),
            Ok(vec![expected])
        );
    }
}

#[test]
fn rejects_unknown_fraction_categories_and_nonfinite_output() {
    for fraction in [0x01, 0xa0] {
        assert_eq!(
            sample_fobj_integer_frames(stream(&[0x06, 0], 0, fraction, fraction), 1),
            Err(FObjEvaluationError::UnknownFractionEncoding)
        );
    }

    let mut packed = vec![0x12];
    packed.extend_from_slice(&0.0f32.to_le_bytes());
    packed.push(1);
    packed.extend_from_slice(&f32::NAN.to_bits().to_le_bytes());
    let mut evaluator = FObjEvaluator::new(stream(&packed, 0, 0, 0));
    assert_eq!(
        evaluator.advance(0.0),
        Err(FObjEvaluationError::NonFiniteSample)
    );
    assert_eq!(
        sample_fobj_integer_frames(stream(&packed, 0, 0, 0), 2),
        Err(FObjEvaluationError::NonFiniteSample)
    );
}

#[test]
fn constant_linear_spline_and_slope_packets_keep_distinct_semantics() {
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x11, 2, 4, 8]), 5),
        Ok(vec![2.0, 2.0, 2.0, 2.0, 8.0])
    );
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x12, 0, 4, 4]), 5),
        Ok(vec![0.0, 1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x13, 0, 4, 4]), 5),
        Ok(vec![0.0, 0.625, 2.0, 3.375, 4.0])
    );

    let spline = [0x14, 0, 2, 0, 4, 4, 0, 0];
    assert_eq!(
        sample_fobj_integer_frames(stream(&spline, 0, 0x80, 0x20), 5),
        Ok(vec![0.0, 1.75, 3.0, 3.75, 4.0])
    );

    // SPL0 point 0, wait 2; SLP tangent 4 without a wait; SPL0 endpoint 4.
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x03, 0, 2, 0x05, 4, 0x03, 4]), 3),
        Ok(vec![0.0, 3.0, 4.0])
    );
}

#[test]
fn linear_interpolation_preserves_fused_cancellation_and_finite_extrapolation() {
    // Source slope is rounded before fmadds (GALE01 FObjUpdateAnim, 0x8036af98).
    // Halfway from 0.5 to -0.5 over three ticks leaves -2^-26, not zero.
    // Extrapolation can overflow an unfused product while the fused result fits.
    for (p0, p1, duration, time, expected) in [
        (0.5f32, -0.5f32, 3u8, 1.5f32, Ok(0xb280_0000u32)),
        (
            -f32::from_bits(0x7f00_0000),
            -f32::from_bits(0x7e80_0000),
            1,
            4.0,
            Ok(0x7f00_0000),
        ),
        (
            -f32::from_bits(0x7f00_0000),
            -f32::from_bits(0x7e80_0000),
            1,
            8.0,
            Err(FObjEvaluationError::NonFiniteSample),
        ),
    ] {
        let mut packed = vec![0x12];
        packed.extend_from_slice(&p0.to_le_bytes());
        packed.push(duration);
        packed.extend_from_slice(&p1.to_le_bytes());
        let mut evaluator = FObjEvaluator::new(stream(&packed, 0, 0, 0));
        let actual = evaluator
            .advance(time)
            .map(|sample| sample.expect("LIN emits a sample").to_bits());
        assert_eq!(actual, expected, "duration={duration}, time={time}");
    }
}

#[test]
fn fractional_hermite_preserves_compiled_power_order_and_fused_accumulation() {
    // GALE01 splGetHelmite (0x80378a34), reached through packed SPL tracks.
    // The old normalized cubic basis produces 0x3f65a51a / 0x406020cc.
    // Source power order without fmadds still produces 0x3f65a51b / 0x406020cc.
    let cases: [(u8, f32, f32, f32, f32, f32, u32); 2] = [
        (7, 2.3, 0.1, 0.7, 0.5, -0.25, 0x3f65_a51c),
        (13, 9.1, -2.7, 7.3, 0.1, 0.9, 0x4060_20ca),
    ];
    for (duration, time, p0, p1, d0, d1, expected_bits) in cases {
        let mut packed = vec![0x14];
        packed.extend_from_slice(&p0.to_le_bytes());
        packed.extend_from_slice(&d0.to_le_bytes());
        packed.push(duration);
        packed.extend_from_slice(&p1.to_le_bytes());
        packed.extend_from_slice(&d1.to_le_bytes());
        let mut evaluator = FObjEvaluator::new(stream(&packed, 0, 0, 0));
        let value = evaluator.advance(time).unwrap().unwrap();
        assert_eq!(
            value.to_bits(),
            expected_bits,
            "duration={duration}, time={time}"
        );
    }
}

#[test]
fn key_packets_emit_once_and_prior_opcode_owns_the_segment() {
    let packed = [0x16, 2, 2, 8];
    let mut evaluator = FObjEvaluator::new(packed_u8(&packed));
    assert_eq!(evaluator.advance(0.0), Ok(Some(2.0)));
    assert_eq!(evaluator.advance(1.0), Ok(None));
    assert_eq!(evaluator.advance(1.0), Ok(Some(8.0)));
    assert_eq!(evaluator.advance(1.0), Ok(None));

    // CON owns the segment even though the endpoint starts a new LIN pack.
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x01, 0, 4, 0x02, 4]), 5),
        Ok(vec![0.0, 0.0, 0.0, 0.0, 4.0])
    );
}

#[test]
fn zero_waits_and_extended_count_and_wait_encodings_make_progress() {
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x12, 1, 0, 5]), 1),
        Ok(vec![5.0])
    );
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x13, 1, 0, 5]), 1),
        Ok(vec![5.0])
    );
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&[0x22, 1, 0, 5, 0, 9]), 1),
        Ok(vec![9.0])
    );

    // LIN count 9: initial count 1 plus continuation value 1 << 3.
    let mut count_nine = vec![0x82, 0x01];
    for value in 0u8..9 {
        count_nine.push(value);
        if value != 8 {
            count_nine.push(1);
        }
    }
    assert_eq!(
        sample_fobj_integer_frames(packed_u8(&count_nine), 9),
        Ok((0u8..9).map(f32::from).collect())
    );

    let wait_300 = [0x12, 0, 0xac, 0x02, 10];
    let values = sample_fobj_integer_frames(packed_u8(&wait_300), 301).expect("wait 300");
    assert_eq!(values[0], 0.0);
    assert!((values[150] - 5.0).abs() < 1.0e-5);
    assert!((values[300] - 10.0).abs() < 1.0e-5);
}

#[test]
fn fobj_terminal_returns_do_not_stop_reserved_opcode_packs() {
    for opcode in [0u8, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
        // Two NONE/reserved entries have no payload. Each ends one interpreter
        // call, not the stored load state; the following LIN still runs.
        let packed = [0x10 | opcode, 0x12, 0, 4, 4];
        let mut evaluator = FObjEvaluator::new(packed_u8(&packed));
        assert_eq!(evaluator.advance(0.0), Ok(None));
        assert_eq!(evaluator.advance(1.0), Ok(None));
        assert_eq!(evaluator.advance(1.0), Ok(Some(2.0)), "opcode {opcode}");
        assert_eq!(evaluator.advance(1.0), Ok(Some(3.0)), "opcode {opcode}");
    }
}

#[test]
fn malformed_streams_fail_structurally() {
    let cases: &[(&[u8], FObjEvaluationError)] = &[
        (&[0x01], FObjEvaluationError::UnexpectedEnd),
        (&[0x04, 1], FObjEvaluationError::UnexpectedEnd),
        (&[0x81], FObjEvaluationError::UnexpectedEnd),
        (&[0x01, 1, 0x80], FObjEvaluationError::UnexpectedEnd),
        (&[0x00, 0x04, 1], FObjEvaluationError::UnexpectedEnd),
        (
            &[0x91, 0xff, 0xff, 0x04],
            FObjEvaluationError::PackCountOverflow,
        ),
        (
            &[0x11, 0, 0xff, 0xff, 0x04],
            FObjEvaluationError::WaitOverflow,
        ),
    ];
    for &(packed, expected) in cases {
        assert_eq!(
            sample_fobj_integer_frames(packed_u8(packed), 2),
            Err(expected)
        );
    }
}

#[test]
fn signed_initial_time_seeks_or_delays_without_changing_tick_sampling() {
    let packed = [0x22, 0, 5, 5, 5, 10];
    assert_eq!(
        sample_fobj_integer_frames(stream(&packed, 7, 0x80, 0), 1),
        Ok(vec![7.0])
    );
    assert_eq!(
        sample_fobj_integer_frames(stream(&packed, 12, 0x80, 0), 1),
        Ok(vec![12.0])
    );

    let delayed = [0x12, 0, 10, 10];
    let delayed_stream = stream(&delayed, -2, 0x80, 0);
    let mut evaluator = FObjEvaluator::new(delayed_stream);
    assert_eq!(evaluator.advance(0.0), Ok(None));
    assert_eq!(evaluator.advance(1.0), Ok(None));
    assert_eq!(evaluator.advance(1.0), Ok(Some(0.0)));

    let values =
        sample_fobj_integer_frames(stream(&delayed, 0, 0x80, 0), 11).expect("integer ticks");
    assert_eq!(values[0], 0.0);
    assert_eq!(values[5], 5.0);
    assert_eq!(values[10], 10.0);
}

#[test]
fn generic_f32_start_frames_match_runtime_s16_narrowing() {
    let packed = [0x12, 0, 10, 10];
    let mut evaluator = FObjEvaluator::new_f32(FObjStreamF32 {
        start_frame: -0.5,
        frac_value: 0x80,
        frac_slope: 0,
        packed_data: &packed,
    })
    .expect("representable generic start frame");
    assert_eq!(evaluator.advance(0.0), Ok(Some(0.0)));

    let error = FObjEvaluator::new_f32(FObjStreamF32 {
        start_frame: 32_768.0,
        frac_value: 0x80,
        frac_slope: 0,
        packed_data: &packed,
    })
    .expect_err("runtime s16 range");
    assert_eq!(error, FObjEvaluationError::StartFrameOutOfRange);
}

#[test]
fn fobj_terminal_returns_preserve_eof_callback_delivery() {
    // KEY leaves a pending launch when followed by CON. At EOF, launching it
    // leaves bit 0x80 set: subsequent ticks update in stored LOAD_WAIT and again
    // in local state 6. FObj's last-value API alone cannot detect a lost callback.
    let packed = [0x06, 2, 1, 0x01, 4];
    let mut evaluator = HsdAObjEvaluator::new(
        0,
        10.0,
        [HsdAObjFObj {
            metadata: (),
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0x80,
                frac_slope: 0,
                packed_data: &packed,
            },
        }],
        1,
    )
    .unwrap();
    evaluator.request(0.0).unwrap();
    let mut updates = Vec::new();
    for expected in [&[][..], &[4.0][..], &[4.0, 4.0][..]] {
        evaluator.advance(|_, value| updates.push(value)).unwrap();
        assert_eq!(updates, expected);
        updates.clear();
    }

    evaluator.set_updates_suppressed(true);
    evaluator.advance(|_, _| unreachable!()).unwrap();
    evaluator.set_updates_suppressed(false);
    evaluator.advance(|_, value| updates.push(value)).unwrap();
    assert_eq!(updates, [4.0, 4.0]);

    evaluator.stop(|_, _| unreachable!()).unwrap();
    assert!(evaluator.advance(|_, _| unreachable!()).unwrap().stopped);
}

#[test]
fn aobj_evaluator_owns_request_first_play_loop_and_no_update_lifecycle() {
    let packed = [0x12, 0, 4, 4];
    let fobj = || HsdAObjFObj {
        metadata: 7u8,
        stream: FObjStreamF32 {
            start_frame: 0.0,
            frac_value: 0x80,
            frac_slope: 0,
            packed_data: &packed,
        },
    };
    let mut evaluator = HsdAObjEvaluator::new(aobj_flags::LOOP, 2.0, vec![fobj()], 1).unwrap();
    assert!(evaluator.is_stopped());
    assert!(evaluator.advance(|_, _| unreachable!()).unwrap().stopped);
    evaluator.request(0.0).unwrap();

    let mut updates = Vec::new();
    let tick = evaluator
        .advance(|metadata, value| updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(
        (tick.current_frame, tick.rewound, tick.stopped),
        (0.0, false, false)
    );
    assert_eq!(updates, [(7, 0.0)]);
    updates.clear();
    evaluator
        .advance(|metadata, value| updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(updates, [(7, 1.0)]);
    updates.clear();
    let tick = evaluator
        .advance(|metadata, value| updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(
        (tick.current_frame, tick.rewound, tick.stopped),
        (0.0, true, false)
    );
    assert_eq!(updates.last(), Some(&(7, 0.0)));

    let multi_key_packed = [0x26, 1, 1, 2, 1, 3];
    let mut multi_key = HsdAObjEvaluator::new(
        0,
        10.0,
        vec![HsdAObjFObj {
            metadata: 8u8,
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0x80,
                frac_slope: 0,
                packed_data: &multi_key_packed,
            },
        }],
        1,
    )
    .unwrap();
    multi_key.request(0.0).unwrap();
    let mut multi_updates = Vec::new();
    multi_key
        .advance(|metadata, value| multi_updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(multi_updates, [(8, 1.0)]);
    multi_updates.clear();
    multi_key.set_rate(2.0).unwrap();
    multi_key
        .advance(|metadata, value| multi_updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(multi_updates, [(8, 2.0), (8, 3.0)]);

    let mut suppressed =
        HsdAObjEvaluator::new(aobj_flags::NO_UPDATE, 10.0, vec![fobj()], 1).unwrap();
    suppressed.request(0.0).unwrap();
    suppressed.advance(|_, _| unreachable!()).unwrap();
    suppressed.set_updates_suppressed(false);
    let mut update = None;
    suppressed
        .advance(|metadata, value| update = Some((*metadata, value)))
        .unwrap();
    assert_eq!(update, Some((7, 1.0)));

    let mut configured = HsdAObjEvaluator::new(aobj_flags::LOOP, 10.0, vec![fobj()], 1).unwrap();
    assert!(!configured.set_current_frame(2.0).unwrap());
    configured.request(0.0).unwrap();
    configured.set_rewind_frame(1.0).unwrap();
    configured.set_end_frame(3.0).unwrap();
    assert_eq!(
        (configured.rewind_frame(), configured.end_frame()),
        (1.0, 3.0)
    );
    assert!(configured.set_current_frame(2.0).unwrap());
    let mut value = None;
    let tick = configured
        .advance(|_, update| value = Some(update))
        .unwrap();
    assert_eq!((tick.current_frame, value), (2.0, Some(2.0)));
    let tick = configured
        .advance(|_, update| value = Some(update))
        .unwrap();
    assert_eq!(
        (tick.current_frame, tick.rewound, value),
        (1.0, true, Some(1.0))
    );
    assert_eq!(
        configured.set_rewind_frame(f32::NAN),
        Err(HsdAObjError::NonFiniteRewindFrame)
    );

    let key_packed = [0x16, 2, 2, 8];
    let mut stopped = HsdAObjEvaluator::new(
        aobj_flags::NO_UPDATE,
        10.0,
        vec![HsdAObjFObj {
            metadata: 9u8,
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0x80,
                frac_slope: 0,
                packed_data: &key_packed,
            },
        }],
        1,
    )
    .unwrap();
    stopped.request(0.0).unwrap();
    stopped.advance(|_, _| unreachable!()).unwrap();
    stopped.set_rate(2.0).unwrap();
    let mut stopped = stopped.into_owned();
    let mut terminal = Vec::new();
    stopped
        .stop(|metadata, value| terminal.push((*metadata, value)))
        .unwrap();
    assert_eq!(terminal, [(9, 2.0), (9, 8.0)]);
    assert!(stopped.is_stopped());
    assert!(stopped.advance(|_, _| unreachable!()).unwrap().stopped);

    let mut non_finite_key = vec![0x16];
    non_finite_key.extend_from_slice(&1.0_f32.to_le_bytes());
    non_finite_key.push(2);
    non_finite_key.extend_from_slice(&f32::NAN.to_le_bytes());
    let mut failing_stop = HsdAObjEvaluator::new(
        0,
        10.0,
        vec![HsdAObjFObj {
            metadata: 10u8,
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0,
                frac_slope: 0,
                packed_data: &non_finite_key,
            },
        }],
        1,
    )
    .unwrap();
    failing_stop.request(0.0).unwrap();
    failing_stop
        .advance(|_, value| assert_eq!(value, 1.0))
        .unwrap();
    failing_stop.set_rate(2.0).unwrap();
    assert_eq!(
        failing_stop.stop(|_, _| {}),
        Err(HsdAObjError::FObj {
            track: 0,
            source: FObjEvaluationError::NonFiniteSample,
        })
    );
    assert!(failing_stop.is_stopped());

    let mut overflowing =
        HsdAObjEvaluator::<u8>::new(aobj_flags::LOOP, f32::MAX, Vec::new(), 0).unwrap();
    overflowing.set_rewind_frame(-f32::MAX).unwrap();
    overflowing.request(0.0).unwrap();
    overflowing.advance(|_, _| {}).unwrap();
    overflowing.set_rate(f32::MAX).unwrap();
    assert_eq!(
        overflowing.advance(|_, _| {}),
        Err(HsdAObjError::CurrentFrameOverflow)
    );
    assert_eq!(overflowing.current_frame(), 0.0);
    assert!(!overflowing.is_stopped());
    assert_eq!(
        configured.set_end_frame(f32::NAN),
        Err(HsdAObjError::NonFiniteEndFrame)
    );

    assert_eq!(
        HsdAObjEvaluator::new(0, f32::NAN, vec![fobj()], 1).unwrap_err(),
        HsdAObjError::NonFiniteEndFrame
    );
    assert_eq!(
        HsdAObjEvaluator::new(0, 1.0, vec![fobj()], 0).unwrap_err(),
        HsdAObjError::FObjBudget { limit: 0 }
    );
}

#[test]
fn aobj_constructor_checks_budget_and_end_frame_before_consuming_tracks() {
    let tracks = || {
        (0..2).map(|_| -> HsdAObjFObj<'static, ()> {
            panic!("rejected AObj must not consume its tracks")
        })
    };
    assert_eq!(
        HsdAObjEvaluator::new(0, f32::NAN, tracks(), 1).unwrap_err(),
        HsdAObjError::FObjBudget { limit: 1 }
    );
    assert_eq!(
        HsdAObjEvaluator::new(0, f32::NAN, tracks(), 2).unwrap_err(),
        HsdAObjError::NonFiniteEndFrame
    );
    let empty = HsdAObjEvaluator::<()>::new(0, 0.0, std::iter::empty(), 0).unwrap();
    assert!(empty.is_empty());
    assert!(empty.is_stopped());
}

#[test]
fn fractional_loop_preserves_source_remainder_and_callback_order() {
    // Two KEY tracks reach their terminal values at frame 1.
    let packed = [[0x16, 2, 1, 8], [0x16, 3, 1, 9]];
    let tracks = packed
        .iter()
        .enumerate()
        .map(|(metadata, packed_data)| HsdAObjFObj {
            metadata,
            stream: FObjStreamF32 {
                start_frame: 0.0,
                frac_value: 0x80,
                frac_slope: 0,
                packed_data,
            },
        });
    let mut evaluator = HsdAObjEvaluator::new(aobj_flags::LOOP, 0.1, tracks, 2).unwrap();
    evaluator.request(0.0).unwrap();
    let mut updates = Vec::new();
    evaluator
        .advance(|metadata, value| updates.push((*metadata, value)))
        .unwrap();
    assert_eq!(updates, [(0, 2.0), (1, 3.0)]);
    updates.clear();

    let tick = evaluator
        .advance(|metadata, value| updates.push((*metadata, value)))
        .unwrap();
    // Matching GALE01 fmod: fdivs rounds 1/0.1 to 10; fnmsubs retains
    // -(10 * (13421773 / 2^27) - 1) = -2^-26, not zero or a positive remainder.
    // Stop flushes both KEY tracks, then the negative request delays both.
    assert_eq!(
        (
            (tick.current_frame.to_bits(), tick.rewound, tick.stopped),
            updates
        ),
        ((0xb280_0000, true, false), vec![(0, 8.0), (1, 9.0)])
    );
    assert_eq!(evaluator.current_frame().to_bits(), 0xb280_0000);

    evaluator.set_rate(0.05).unwrap();
    let mut resumed = Vec::new();
    let tick = evaluator
        .advance(|metadata, value| resumed.push((*metadata, value)))
        .unwrap();
    assert!(!tick.rewound && !tick.stopped);
    assert_eq!(resumed, [(0, 2.0), (1, 3.0)]);
}

#[test]
fn loop_quotient_conversion_and_rewind_keep_source_float_boundaries() {
    // Expected words follow GALE01 fdivs -> __cvt_dbl_usll ->
    // __cvt_sll_flt -> fnmsubs -> fadds, not a host remainder function.
    for (rewind, end, rate, expected) in [
        (0.0, 0.5, 1.75, 0x3e80_0000), // Truncate quotient 3.5 to 3.
        (1.0, 1.5, 2.75, 0x3fa0_0000), // Add rewind after the fused residual.
        (-0.0, 0.5, 1.0, 0x8000_0000), // fnmsubs negates exact cancellation.
        (0.0, f32::from_bits(0x1f80_0000), 1.0, 0x3f00_0000), // 2^64 saturates to i64::MAX, then rounds to 2^63.
        (0.0, f32::MIN_POSITIVE, f32::MAX, 0x7f7f_ffff), // Infinite quotient has the same source saturation.
    ] {
        let mut evaluator =
            HsdAObjEvaluator::<()>::new(aobj_flags::LOOP, end, Vec::new(), 0).unwrap();
        evaluator.set_rewind_frame(rewind).unwrap();
        evaluator.set_rate(rate).unwrap();
        evaluator.request(0.0).unwrap();
        evaluator.advance(|_, _| unreachable!()).unwrap();
        let tick = evaluator.advance(|_, _| unreachable!()).unwrap();
        assert_eq!(
            (tick.current_frame.to_bits(), tick.rewound, tick.stopped),
            (expected, true, false),
            "rewind={rewind:?}, end={end:?}, rate={rate:?}"
        );
    }
}
