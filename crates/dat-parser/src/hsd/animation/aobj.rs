use thiserror::Error;

use super::fobj::{FObjEvaluationError, FObjEvaluator, FObjStreamF32};

pub mod aobj_flags {
    pub(crate) const REWINDED: u32 = 1 << 26;
    pub(crate) const FIRST_PLAY: u32 = 1 << 27;
    pub const NO_UPDATE: u32 = 1 << 28;
    pub const LOOP: u32 = 1 << 29;
    pub(crate) const NO_ANIM: u32 = 1 << 30;
}

#[derive(Clone, Debug)]
pub struct HsdAObjFObj<'a, M> {
    pub metadata: M,
    pub stream: FObjStreamF32<'a>,
}

#[derive(Debug)]
struct HsdAObjTrack<'a, M> {
    metadata: M,
    evaluator: FObjEvaluator<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HsdAObjTick {
    pub current_frame: f32,
    pub rewound: bool,
    pub stopped: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum HsdAObjError {
    #[error("AObj exceeds the FObj budget of {limit}")]
    FObjBudget { limit: usize },
    #[error("AObj end frame is not finite")]
    NonFiniteEndFrame,
    #[error("AObj rate is not finite")]
    NonFiniteRate,
    #[error("AObj rewind frame is not finite")]
    NonFiniteRewindFrame,
    #[error("AObj request frame is not finite")]
    NonFiniteRequestFrame,
    #[error("AObj current frame overflowed")]
    CurrentFrameOverflow,
    #[error("AObj FObj {track} cannot be evaluated: {source}")]
    FObj {
        track: usize,
        source: FObjEvaluationError,
    },
}

/// Bounded runtime AObj lifecycle over source-ordered pointer-free FObj streams.
#[derive(Debug)]
pub struct HsdAObjEvaluator<'a, M> {
    tracks: Vec<HsdAObjTrack<'a, M>>,
    flags: u32,
    current_frame: f32,
    rewind_frame: f32,
    end_frame: f32,
    rate: f32,
}

impl<'a, M> HsdAObjEvaluator<'a, M> {
    pub fn new<I>(
        raw_flags: u32,
        end_frame: f32,
        fobjs: I,
        max_fobjs: usize,
    ) -> Result<Self, HsdAObjError>
    where
        I: IntoIterator<Item = HsdAObjFObj<'a, M>>,
        I::IntoIter: ExactSizeIterator,
    {
        let fobjs = fobjs.into_iter();
        if fobjs.len() > max_fobjs {
            return Err(HsdAObjError::FObjBudget { limit: max_fobjs });
        }
        if !end_frame.is_finite() {
            return Err(HsdAObjError::NonFiniteEndFrame);
        }
        let mut tracks = Vec::with_capacity(fobjs.len());
        for (track, fobj) in fobjs.enumerate() {
            let evaluator = FObjEvaluator::new_f32(fobj.stream)
                .map_err(|source| HsdAObjError::FObj { track, source })?;
            tracks.push(HsdAObjTrack {
                metadata: fobj.metadata,
                evaluator,
            });
        }
        Ok(Self {
            tracks,
            flags: aobj_flags::NO_ANIM | (raw_flags & (aobj_flags::LOOP | aobj_flags::NO_UPDATE)),
            current_frame: 0.0,
            rewind_frame: 0.0,
            end_frame,
            rate: 1.0,
        })
    }

    /// Own packed streams without resetting any track or lifecycle state.
    /// Metadata is moved unchanged and may retain its own independent lifetime.
    pub fn into_owned(self) -> HsdAObjEvaluator<'static, M> {
        HsdAObjEvaluator {
            tracks: self
                .tracks
                .into_iter()
                .map(|track| HsdAObjTrack {
                    metadata: track.metadata,
                    evaluator: track.evaluator.into_owned(),
                })
                .collect(),
            flags: self.flags,
            current_frame: self.current_frame,
            rewind_frame: self.rewind_frame,
            end_frame: self.end_frame,
            rate: self.rate,
        }
    }

    /// Checked aggregate size; overlapping or duplicate byte ranges count once
    /// per track because each borrowed track is copied during ownership conversion.
    pub(super) fn checked_packed_byte_len(&self) -> Option<usize> {
        self.tracks.iter().try_fold(0usize, |total, track| {
            total.checked_add(track.evaluator.packed_byte_len())
        })
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn current_frame(&self) -> f32 {
        self.current_frame
    }

    pub fn end_frame(&self) -> f32 {
        self.end_frame
    }

    pub fn is_stopped(&self) -> bool {
        self.flags & aobj_flags::NO_ANIM != 0
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.set_flag(aobj_flags::LOOP, looping);
    }

    pub fn set_updates_suppressed(&mut self, suppressed: bool) {
        self.set_flag(aobj_flags::NO_UPDATE, suppressed);
    }

    fn set_flag(&mut self, flag: u32, enabled: bool) {
        if enabled {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<(), HsdAObjError> {
        if !rate.is_finite() {
            return Err(HsdAObjError::NonFiniteRate);
        }
        self.rate = rate;
        Ok(())
    }

    pub fn set_rewind_frame(&mut self, frame: f32) -> Result<(), HsdAObjError> {
        if !frame.is_finite() {
            return Err(HsdAObjError::NonFiniteRewindFrame);
        }
        self.rewind_frame = frame;
        Ok(())
    }

    pub fn set_end_frame(&mut self, frame: f32) -> Result<(), HsdAObjError> {
        if !frame.is_finite() {
            return Err(HsdAObjError::NonFiniteEndFrame);
        }
        self.end_frame = frame;
        Ok(())
    }

    fn validate_request(&self, frame: f32) -> Result<(), HsdAObjError> {
        if !frame.is_finite() {
            return Err(HsdAObjError::NonFiniteRequestFrame);
        }
        for (track, fobj) in self.tracks.iter().enumerate() {
            fobj.evaluator
                .requested_time(frame)
                .map_err(|source| HsdAObjError::FObj { track, source })?;
        }
        Ok(())
    }

    fn request_tracks(&mut self, frame: f32) -> Result<(), HsdAObjError> {
        self.validate_request(frame)?;
        for (track, fobj) in self.tracks.iter_mut().enumerate() {
            fobj.evaluator
                .request(frame)
                .map_err(|source| HsdAObjError::FObj { track, source })?;
        }
        Ok(())
    }

    pub fn request(&mut self, frame: f32) -> Result<(), HsdAObjError> {
        self.request_tracks(frame)?;
        self.current_frame = frame;
        self.flags &= !aobj_flags::NO_ANIM;
        self.flags |= aobj_flags::FIRST_PLAY;
        Ok(())
    }

    pub fn set_current_frame(&mut self, frame: f32) -> Result<bool, HsdAObjError> {
        if self.is_stopped() {
            return Ok(false);
        }
        self.request_tracks(frame)?;
        self.current_frame = frame;
        self.flags |= aobj_flags::FIRST_PLAY;
        Ok(true)
    }

    fn stop_tracks(
        &mut self,
        rate: f32,
        update: &mut impl FnMut(&M, f32),
    ) -> Result<(), HsdAObjError> {
        let mut first_error = None;
        for (track, fobj) in self.tracks.iter_mut().enumerate() {
            let metadata = &fobj.metadata;
            if let Err(source) = fobj
                .evaluator
                .stop_with(rate, &mut |value| update(metadata, value))
                && first_error.is_none()
            {
                first_error = Some(HsdAObjError::FObj { track, source });
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub fn stop(&mut self, mut update: impl FnMut(&M, f32)) -> Result<(), HsdAObjError> {
        let result = self.stop_tracks(self.rate, &mut update);
        self.flags |= aobj_flags::NO_ANIM;
        result
    }

    fn interpret_tracks(
        &mut self,
        rate: f32,
        deliver_updates: bool,
        update: &mut impl FnMut(&M, f32),
    ) -> Result<(), HsdAObjError> {
        for (track, fobj) in self.tracks.iter_mut().enumerate() {
            let metadata = &fobj.metadata;
            fobj.evaluator
                .advance_with(rate, deliver_updates, &mut |value| update(metadata, value))
                .map_err(|source| HsdAObjError::FObj { track, source })?;
        }
        Ok(())
    }

    pub fn advance(
        &mut self,
        mut update: impl FnMut(&M, f32),
    ) -> Result<HsdAObjTick, HsdAObjError> {
        if self.is_stopped() {
            return Ok(HsdAObjTick {
                current_frame: self.current_frame,
                rewound: false,
                stopped: true,
            });
        }

        let mut current_frame = self.current_frame;
        let mut rate = if self.flags & aobj_flags::FIRST_PLAY != 0 {
            self.flags &= !aobj_flags::FIRST_PLAY;
            0.0
        } else {
            current_frame += self.rate;
            if !current_frame.is_finite() {
                return Err(HsdAObjError::CurrentFrameOverflow);
            }
            self.rate
        };

        let mut rewound = false;
        if self.flags & aobj_flags::LOOP != 0 && self.end_frame <= current_frame {
            let mut transition_error = None;
            if self.rewind_frame < self.end_frame {
                let span = self.end_frame - self.rewind_frame;
                let offset = current_frame - self.rewind_frame;
                if !span.is_finite() || !offset.is_finite() {
                    return Err(HsdAObjError::CurrentFrameOverflow);
                }
                // GALE01 fmod (0x80364340): fdivs, signed-i64 truncation/
                // saturation, conversion back to f32, then fnmsubs. A host
                // remainder or an unfused subtraction changes fractional wraps.
                // Keep the final negation: exact cancellation produces -0.
                let quotient = (offset / span) as i64 as f32;
                let wrapped = -span.mul_add(quotient, -offset) + self.rewind_frame;
                if !wrapped.is_finite() {
                    return Err(HsdAObjError::CurrentFrameOverflow);
                }
                transition_error = self.stop_tracks(rate, &mut update).err();
                current_frame = wrapped;
                self.current_frame = current_frame;
                if let Err(error) = self.request_tracks(current_frame)
                    && transition_error.is_none()
                {
                    transition_error = Some(error);
                }
            } else {
                current_frame = self.end_frame;
                self.current_frame = current_frame;
            }
            rate = 0.0;
            rewound = true;
            self.flags |= aobj_flags::REWINDED;
            if let Some(error) = transition_error {
                return Err(error);
            }
        } else {
            self.current_frame = current_frame;
            self.flags &= !aobj_flags::REWINDED;
        }

        let interpretation =
            self.interpret_tracks(rate, self.flags & aobj_flags::NO_UPDATE == 0, &mut update);

        if self.flags & aobj_flags::LOOP == 0 && self.end_frame <= self.current_frame {
            let stop = self.stop_tracks(self.rate, &mut update);
            self.flags |= aobj_flags::NO_ANIM;
            interpretation?;
            stop?;
        } else {
            interpretation?;
        }

        Ok(HsdAObjTick {
            current_frame: self.current_frame,
            rewound,
            stopped: self.is_stopped(),
        })
    }
}
