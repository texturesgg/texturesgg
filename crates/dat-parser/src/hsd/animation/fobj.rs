use std::ops::Deref;
use std::sync::Arc;

use thiserror::Error;

/// Generic HSD FObj descriptors serialize `startframe` as `f32`; runtime HSD narrows it to `s16`.
#[derive(Clone, Copy, Debug)]
pub struct FObjStreamF32<'a> {
    pub start_frame: f32,
    pub frac_value: u8,
    pub frac_slope: u8,
    pub packed_data: &'a [u8],
}

#[derive(Debug)]
struct PackedFObjStream<'a> {
    frac_value: u8,
    frac_slope: u8,
    packed_data: PackedData<'a>,
}

#[derive(Debug)]
enum PackedData<'a> {
    Borrowed(&'a [u8]),
    Shared(Arc<[u8]>),
}

impl PackedData<'_> {
    fn into_owned(self) -> PackedData<'static> {
        PackedData::Shared(match self {
            Self::Borrowed(bytes) => Arc::from(bytes),
            Self::Shared(bytes) => bytes,
        })
    }
}

impl Deref for PackedData<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Shared(bytes) => bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum FObjEvaluationError {
    #[error("packed stream does not make progress")]
    NoProgress,
    #[error("invalid interpreter state")]
    InvalidState,
    #[error("pack count overflows")]
    PackCountOverflow,
    #[error("wait overflows")]
    WaitOverflow,
    #[error("packed stream ends unexpectedly")]
    UnexpectedEnd,
    #[error("float value is truncated")]
    TruncatedFloat,
    #[error("signed 16-bit value is truncated")]
    TruncatedSigned16,
    #[error("unsigned 16-bit value is truncated")]
    TruncatedUnsigned16,
    #[error("unknown fractional value encoding")]
    UnknownFractionEncoding,
    #[error("FObj start frame is not finite")]
    NonFiniteStartFrame,
    #[error("FObj start frame cannot be represented by the runtime s16 field")]
    StartFrameOutOfRange,
    #[error("FObj request frame is not finite")]
    NonFiniteRequestFrame,
    #[error("FObj rate is not finite")]
    NonFiniteRate,
    #[error("decoded sample is not finite")]
    NonFiniteSample,
}

/// Stateful scalar interpreter matching HSD's FObj operation ordering.
#[derive(Debug)]
pub(super) struct FObjEvaluator<'a> {
    start_frame: f32,
    stream: PackedFObjStream<'a>,
    pos: usize,
    state: u8,
    flags: u8,
    op: u8,
    interpolation: u8,
    pack_remaining: u16,
    wait: u16,
    time: f32,
    p0: f32,
    p1: f32,
    d0: f32,
    d1: f32,
}

impl<'a> FObjEvaluator<'a> {
    pub(super) fn new_f32(stream: FObjStreamF32<'a>) -> Result<Self, FObjEvaluationError> {
        if !stream.start_frame.is_finite() {
            return Err(FObjEvaluationError::NonFiniteStartFrame);
        }
        let runtime_start_frame = stream.start_frame.trunc();
        if runtime_start_frame < i16::MIN as f32 || runtime_start_frame > i16::MAX as f32 {
            return Err(FObjEvaluationError::StartFrameOutOfRange);
        }
        Ok(Self::with_start_frame(
            (runtime_start_frame as i16) as f32,
            PackedFObjStream {
                frac_value: stream.frac_value,
                frac_slope: stream.frac_slope,
                packed_data: PackedData::Borrowed(stream.packed_data),
            },
        ))
    }

    fn with_start_frame(start_frame: f32, stream: PackedFObjStream<'a>) -> Self {
        Self {
            start_frame,
            time: start_frame,
            stream,
            pos: 0,
            state: 1,
            flags: 0,
            op: 0,
            interpolation: 0,
            pack_remaining: 0,
            wait: 0,
            p0: 0.0,
            p1: 0.0,
            d0: 0.0,
            d1: 0.0,
        }
    }

    /// Retain the complete interpreter state independently of its source buffer.
    /// Already-owned packed bytes are moved without another copy.
    pub fn into_owned(self) -> FObjEvaluator<'static> {
        FObjEvaluator {
            start_frame: self.start_frame,
            stream: PackedFObjStream {
                frac_value: self.stream.frac_value,
                frac_slope: self.stream.frac_slope,
                packed_data: self.stream.packed_data.into_owned(),
            },
            pos: self.pos,
            state: self.state,
            flags: self.flags,
            op: self.op,
            interpolation: self.interpolation,
            pack_remaining: self.pack_remaining,
            wait: self.wait,
            time: self.time,
            p0: self.p0,
            p1: self.p1,
            d0: self.d0,
            d1: self.d1,
        }
    }

    pub fn packed_byte_len(&self) -> usize {
        self.stream.packed_data.len()
    }

    pub(super) fn requested_time(&self, frame: f32) -> Result<f32, FObjEvaluationError> {
        let time = self.start_frame + frame;
        if frame.is_finite() && time.is_finite() {
            Ok(time)
        } else {
            Err(FObjEvaluationError::NonFiniteRequestFrame)
        }
    }

    /// Reset this runtime FObj to the serialized stream at an explicit AObj frame.
    pub fn request(&mut self, frame: f32) -> Result<(), FObjEvaluationError> {
        let time = self.requested_time(frame)?;
        self.pos = 0;
        self.time = time;
        self.op = 0;
        self.interpolation = 0;
        self.flags &= !0x40;
        self.pack_remaining = 0;
        self.wait = 0;
        self.p0 = 0.0;
        self.p1 = 0.0;
        self.d0 = 0.0;
        self.d1 = 0.0;
        self.state = 1;
        Ok(())
    }

    /// Give a pending KEY operation one final interpretation, then stop.
    pub(super) fn stop_with(
        &mut self,
        rate: f32,
        update: &mut impl FnMut(f32),
    ) -> Result<Option<f32>, FObjEvaluationError> {
        if !rate.is_finite() {
            return Err(FObjEvaluationError::NonFiniteRate);
        }
        let result = if self.interpolation == 6 {
            self.advance_with(rate, true, update)
        } else {
            Ok(None)
        };
        self.state = 0;
        result
    }

    /// Advance by an explicit tick delta, handing each emitted value to
    /// `update`, and return the last.
    pub(super) fn advance_with(
        &mut self,
        rate: f32,
        deliver_updates: bool,
        update: &mut impl FnMut(f32),
    ) -> Result<Option<f32>, FObjEvaluationError> {
        if !rate.is_finite() {
            return Err(FObjEvaluationError::NonFiniteRate);
        }
        if self.state == 0 {
            return Ok(None);
        }
        self.time += rate;
        if self.time < 0.0 {
            return Ok(None);
        }
        // EOF and undefined opcodes return per-call states, not stored states.
        // Keep these separate so the next tick resumes the source load state.
        let mut state = self.state;
        let mut last = None;
        let mut terminal_offset = 0.0;
        let mut operations = 0usize;
        loop {
            operations += 1;
            if operations
                > self
                    .stream
                    .packed_data
                    .len()
                    .saturating_mul(4)
                    .saturating_add(64)
            {
                return Err(FObjEvaluationError::NoProgress);
            }
            match state {
                0 => return Ok(last),
                1 | 2 => state = self.load_data()?,
                3 => {
                    if deliver_updates
                        && self.flags & 0x80 != 0
                        && let Some(value) = self.update()?
                    {
                        last = Some(value);
                        update(value);
                    }
                    state = self.load_wait()?;
                }
                4 => {
                    if self.wait as f32 <= self.time {
                        terminal_offset = self.wait as f32;
                        self.time -= self.wait as f32;
                        self.state = 3;
                        state = 3;
                    } else {
                        if deliver_updates && let Some(value) = self.update()? {
                            last = Some(value);
                            update(value);
                        }
                        self.state = 5;
                        return Ok(last);
                    }
                }
                5 => {
                    self.state = 4;
                    state = 4;
                }
                6 => {
                    self.time += terminal_offset;
                    self.launch_key();
                    if deliver_updates && let Some(value) = self.update()? {
                        last = Some(value);
                        update(value);
                    }
                    return Ok(last);
                }
                _ => return Err(FObjEvaluationError::InvalidState),
            }
        }
    }

    fn load_data(&mut self) -> Result<u8, FObjEvaluationError> {
        if self.pos >= self.stream.packed_data.len() {
            return Ok(6);
        }
        self.interpolation = self.op;
        if self.pack_remaining == 0 {
            let first = self.byte()?;
            self.op = first & 0x0f;
            self.pack_remaining = u16::from((first >> 4) & 7) + 1;
            if first & 0x80 != 0 {
                let mut count = u64::from(self.pack_remaining);
                let mut shift = 3u32;
                loop {
                    let value = self.byte()?;
                    if shift >= 16 {
                        return Err(FObjEvaluationError::PackCountOverflow);
                    }
                    let additional = u64::from(value & 0x7f)
                        .checked_shl(shift)
                        .ok_or(FObjEvaluationError::PackCountOverflow)?;
                    count = count
                        .checked_add(additional)
                        .ok_or(FObjEvaluationError::PackCountOverflow)?;
                    if count > u64::from(u16::MAX) {
                        return Err(FObjEvaluationError::PackCountOverflow);
                    }
                    if value & 0x80 == 0 {
                        break;
                    }
                    shift = shift
                        .checked_add(7)
                        .ok_or(FObjEvaluationError::PackCountOverflow)?;
                }
                self.pack_remaining =
                    u16::try_from(count).map_err(|_| FObjEvaluationError::PackCountOverflow)?;
            }
        }
        self.pack_remaining -= 1;
        let initial = self.state == 1;
        match self.op {
            1 | 2 => {
                self.p0 = self.p1;
                self.p1 = self.parse_float(self.stream.frac_value)?;
                if self.interpolation != 5 {
                    self.d0 = self.d1;
                    self.d1 = 0.0;
                }
            }
            3 => {
                self.p0 = self.p1;
                self.d0 = self.d1;
                self.p1 = self.parse_float(self.stream.frac_value)?;
                self.d1 = 0.0;
            }
            4 => {
                self.p0 = self.p1;
                self.p1 = self.parse_float(self.stream.frac_value)?;
                self.d0 = self.d1;
                self.d1 = self.parse_float(self.stream.frac_slope)?;
            }
            5 => {
                self.d0 = self.d1;
                self.d1 = self.parse_float(self.stream.frac_slope)?;
                // SLP changes the incoming tangent for the following value
                // packet and has no wait payload of its own.
                return Ok(self.state);
            }
            6 => {
                self.launch_key();
                self.p1 = self.parse_float(self.stream.frac_value)?;
                self.flags |= 0x40;
            }
            // NONE/reserved opcodes end this call without consuming a guessed
            // payload or changing the stored load state.
            0 | 7..=u8::MAX => return Ok(0),
        }
        self.state = if initial { 3 } else { 4 };
        Ok(self.state)
    }

    fn load_wait(&mut self) -> Result<u8, FObjEvaluationError> {
        if self.pos >= self.stream.packed_data.len() {
            return Ok(6);
        }
        let mut wait = 0u64;
        let mut shift = 0u32;
        loop {
            let value = self.byte()?;
            if shift >= 16 {
                return Err(FObjEvaluationError::WaitOverflow);
            }
            let additional = u64::from(value & 0x7f)
                .checked_shl(shift)
                .ok_or(FObjEvaluationError::WaitOverflow)?;
            wait = wait
                .checked_add(additional)
                .ok_or(FObjEvaluationError::WaitOverflow)?;
            if wait > u64::from(u16::MAX) {
                return Err(FObjEvaluationError::WaitOverflow);
            }
            if value & 0x80 == 0 {
                break;
            }
            shift = shift
                .checked_add(7)
                .ok_or(FObjEvaluationError::WaitOverflow)?;
        }
        self.wait = u16::try_from(wait).map_err(|_| FObjEvaluationError::WaitOverflow)?;
        self.flags |= 0x20;
        self.state = 2;
        Ok(self.state)
    }

    fn update(&mut self) -> Result<Option<f32>, FObjEvaluationError> {
        let value = match self.interpolation {
            6 => {
                if self.flags & 0x80 == 0 {
                    return Ok(None);
                }
                self.flags &= !0x80;
                self.p0
            }
            1 => {
                if self.time >= self.wait as f32 {
                    self.p1
                } else {
                    self.p0
                }
            }
            2 => {
                if self.flags & 0x20 != 0 {
                    self.flags &= !0x20;
                    if self.wait != 0 {
                        self.d0 = (self.p1 - self.p0) / self.wait as f32;
                    } else {
                        self.d0 = 0.0;
                        self.p0 = self.p1;
                    }
                }
                // GALE01 FObjUpdateAnim (0x8036af98) uses fmadds after
                // separately rounded slope subtraction/division and storage.
                self.d0.mul_add(self.time, self.p0)
            }
            3..=5 => {
                if self.wait == 0 {
                    self.p1
                } else {
                    hermite(
                        self.time,
                        self.wait as f32,
                        self.p0,
                        self.p1,
                        self.d0,
                        self.d1,
                    )
                }
            }
            _ => return Ok(None),
        };
        if !value.is_finite() {
            return Err(FObjEvaluationError::NonFiniteSample);
        }
        Ok(Some(value))
    }

    fn launch_key(&mut self) {
        if self.flags & 0x40 != 0 {
            self.interpolation = self.op;
            self.flags &= !0x40;
            self.flags |= 0x80;
            self.p0 = self.p1;
        }
    }

    fn byte(&mut self) -> Result<u8, FObjEvaluationError> {
        let value = *self
            .stream
            .packed_data
            .get(self.pos)
            .ok_or(FObjEvaluationError::UnexpectedEnd)?;
        self.pos += 1;
        Ok(value)
    }

    fn parse_float(&mut self, fraction: u8) -> Result<f32, FObjEvaluationError> {
        if fraction == 0 {
            let bytes: [u8; 4] = self
                .stream
                .packed_data
                .get(self.pos..self.pos + 4)
                .ok_or(FObjEvaluationError::TruncatedFloat)?
                .try_into()
                .map_err(|_| FObjEvaluationError::TruncatedFloat)?;
            self.pos += 4;
            return Ok(f32::from_bits(u32::from_le_bytes(bytes)));
        }
        let denominator = 1i32.wrapping_shl(u32::from(fraction & 0x1f)) as f32;
        let numerator = match fraction & 0xe0 {
            0x20 => {
                let bytes: [u8; 2] = self
                    .stream
                    .packed_data
                    .get(self.pos..self.pos + 2)
                    .ok_or(FObjEvaluationError::TruncatedSigned16)?
                    .try_into()
                    .map_err(|_| FObjEvaluationError::TruncatedSigned16)?;
                self.pos += 2;
                i16::from_le_bytes(bytes) as f32
            }
            0x40 => {
                let bytes: [u8; 2] = self
                    .stream
                    .packed_data
                    .get(self.pos..self.pos + 2)
                    .ok_or(FObjEvaluationError::TruncatedUnsigned16)?
                    .try_into()
                    .map_err(|_| FObjEvaluationError::TruncatedUnsigned16)?;
                self.pos += 2;
                u16::from_le_bytes(bytes) as f32
            }
            0x60 => self.byte()? as i8 as f32,
            0x80 => self.byte()? as f32,
            _ => return Err(FObjEvaluationError::UnknownFractionEncoding),
        };
        Ok(numerator / denominator)
    }
}

fn hermite(time: f32, duration: f32, p0: f32, p1: f32, d0: f32, d1: f32) -> f32 {
    // GALE01 FObjUpdateAnim (0x8036afdc): double reciprocal, then frsp.
    // splGetHelmite (0x80378a34) builds powers before normalization and
    // finishes with three fmadds. The usual cubic basis changes rounding.
    let inverse_duration = (1.0 / f64::from(duration)) as f32;
    let time_squared = time * time;
    let inverse_squared = inverse_duration * inverse_duration;
    let scaled_square = time_squared * inverse_duration;
    let scaled_cube = inverse_squared * (time_squared * time);
    let twice_cube = (2.0 * scaled_cube) * inverse_duration;
    let thrice_square = (3.0 * time_squared) * inverse_squared;
    let end_tangent = scaled_cube - scaled_square;
    let start_tangent = time + (end_tangent - scaled_square);
    let start_position = 1.0 + (twice_cube - thrice_square);
    let end_position = -twice_cube + thrice_square;
    let positions = p0.mul_add(start_position, p1 * end_position);
    let with_start_tangent = d0.mul_add(start_tangent, positions);
    d1.mul_add(end_tangent, with_start_tangent)
}

#[cfg(test)]
mod tests {
    use super::{FObjEvaluationError, FObjEvaluator, FObjStreamF32};

    fn stream(
        packed_data: &[u8],
        start_frame: i16,
        frac_value: u8,
        frac_slope: u8,
    ) -> FObjStreamF32<'_> {
        FObjStreamF32 {
            start_frame: f32::from(start_frame),
            frac_value,
            frac_slope,
            packed_data,
        }
    }

    fn packed_u8(packed_data: &[u8]) -> FObjStreamF32<'_> {
        stream(packed_data, 0, 0x80, 0x80)
    }

    fn evaluator(stream: FObjStreamF32<'_>) -> FObjEvaluator<'_> {
        FObjEvaluator::new_f32(stream).expect("a start frame the runtime field holds")
    }

    /// Advance by a tick delta and return the last emitted value.
    fn advance(
        evaluator: &mut FObjEvaluator<'_>,
        rate: f32,
    ) -> Result<Option<f32>, FObjEvaluationError> {
        evaluator.advance_with(rate, true, &mut |_| {})
    }

    /// Integer ticks `0..frame_count`, holding the last emitted value before
    /// a delayed start and after the stream ends.
    fn sample_integer_frames(
        stream: FObjStreamF32<'_>,
        frame_count: usize,
    ) -> Result<Vec<f32>, FObjEvaluationError> {
        let mut evaluator = evaluator(stream);
        let mut current = 0.0;
        (0..frame_count)
            .map(|frame| {
                if let Some(value) = advance(&mut evaluator, if frame == 0 { 0.0 } else { 1.0 })? {
                    current = value;
                }
                Ok(current)
            })
            .collect()
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
                sample_integer_frames(stream(&packed, 0, fraction, fraction), 1),
                Ok(vec![expected])
            );
        }
    }

    #[test]
    fn rejects_unknown_fraction_categories_and_nonfinite_output() {
        for fraction in [0x01, 0xa0] {
            assert_eq!(
                sample_integer_frames(stream(&[0x06, 0], 0, fraction, fraction), 1),
                Err(FObjEvaluationError::UnknownFractionEncoding)
            );
        }

        let mut packed = vec![0x12];
        packed.extend_from_slice(&0.0f32.to_le_bytes());
        packed.push(1);
        packed.extend_from_slice(&f32::NAN.to_bits().to_le_bytes());
        let mut evaluator = evaluator(stream(&packed, 0, 0, 0));
        assert_eq!(
            advance(&mut evaluator, 0.0),
            Err(FObjEvaluationError::NonFiniteSample)
        );
        assert_eq!(
            sample_integer_frames(stream(&packed, 0, 0, 0), 2),
            Err(FObjEvaluationError::NonFiniteSample)
        );
    }

    #[test]
    fn constant_linear_spline_and_slope_packets_keep_distinct_semantics() {
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x11, 2, 4, 8]), 5),
            Ok(vec![2.0, 2.0, 2.0, 2.0, 8.0])
        );
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x12, 0, 4, 4]), 5),
            Ok(vec![0.0, 1.0, 2.0, 3.0, 4.0])
        );
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x13, 0, 4, 4]), 5),
            Ok(vec![0.0, 0.625, 2.0, 3.375, 4.0])
        );

        let spline = [0x14, 0, 2, 0, 4, 4, 0, 0];
        assert_eq!(
            sample_integer_frames(stream(&spline, 0, 0x80, 0x20), 5),
            Ok(vec![0.0, 1.75, 3.0, 3.75, 4.0])
        );

        // SPL0 point 0, wait 2; SLP tangent 4 without a wait; SPL0 endpoint 4.
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x03, 0, 2, 0x05, 4, 0x03, 4]), 3),
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
            let mut evaluator = evaluator(stream(&packed, 0, 0, 0));
            let actual = advance(&mut evaluator, time)
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
            let mut evaluator = evaluator(stream(&packed, 0, 0, 0));
            let value = advance(&mut evaluator, time).unwrap().unwrap();
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
        let mut evaluator = evaluator(packed_u8(&packed));
        assert_eq!(advance(&mut evaluator, 0.0), Ok(Some(2.0)));
        assert_eq!(advance(&mut evaluator, 1.0), Ok(None));
        assert_eq!(advance(&mut evaluator, 1.0), Ok(Some(8.0)));
        assert_eq!(advance(&mut evaluator, 1.0), Ok(None));

        // CON owns the segment even though the endpoint starts a new LIN pack.
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x01, 0, 4, 0x02, 4]), 5),
            Ok(vec![0.0, 0.0, 0.0, 0.0, 4.0])
        );
    }

    #[test]
    fn zero_waits_and_extended_count_and_wait_encodings_make_progress() {
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x12, 1, 0, 5]), 1),
            Ok(vec![5.0])
        );
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x13, 1, 0, 5]), 1),
            Ok(vec![5.0])
        );
        assert_eq!(
            sample_integer_frames(packed_u8(&[0x22, 1, 0, 5, 0, 9]), 1),
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
            sample_integer_frames(packed_u8(&count_nine), 9),
            Ok((0u8..9).map(f32::from).collect())
        );

        let wait_300 = [0x12, 0, 0xac, 0x02, 10];
        let values = sample_integer_frames(packed_u8(&wait_300), 301).expect("wait 300");
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
            let mut evaluator = evaluator(packed_u8(&packed));
            assert_eq!(advance(&mut evaluator, 0.0), Ok(None));
            assert_eq!(advance(&mut evaluator, 1.0), Ok(None));
            assert_eq!(
                advance(&mut evaluator, 1.0),
                Ok(Some(2.0)),
                "opcode {opcode}"
            );
            assert_eq!(
                advance(&mut evaluator, 1.0),
                Ok(Some(3.0)),
                "opcode {opcode}"
            );
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
            assert_eq!(sample_integer_frames(packed_u8(packed), 2), Err(expected));
        }
    }

    #[test]
    fn signed_initial_time_seeks_or_delays_without_changing_tick_sampling() {
        let packed = [0x22, 0, 5, 5, 5, 10];
        assert_eq!(
            sample_integer_frames(stream(&packed, 7, 0x80, 0), 1),
            Ok(vec![7.0])
        );
        assert_eq!(
            sample_integer_frames(stream(&packed, 12, 0x80, 0), 1),
            Ok(vec![12.0])
        );

        let delayed = [0x12, 0, 10, 10];
        let delayed_stream = stream(&delayed, -2, 0x80, 0);
        let mut evaluator = evaluator(delayed_stream);
        assert_eq!(advance(&mut evaluator, 0.0), Ok(None));
        assert_eq!(advance(&mut evaluator, 1.0), Ok(None));
        assert_eq!(advance(&mut evaluator, 1.0), Ok(Some(0.0)));

        let values =
            sample_integer_frames(stream(&delayed, 0, 0x80, 0), 11).expect("integer ticks");
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
        assert_eq!(advance(&mut evaluator, 0.0), Ok(Some(0.0)));

        let error = FObjEvaluator::new_f32(FObjStreamF32 {
            start_frame: 32_768.0,
            frac_value: 0x80,
            frac_slope: 0,
            packed_data: &packed,
        })
        .expect_err("runtime s16 range");
        assert_eq!(error, FObjEvaluationError::StartFrameOutOfRange);
    }
}
