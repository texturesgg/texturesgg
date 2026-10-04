//! Fighters' move scripts: the commands each action runs as it plays
//! (`Fighter_WaitAnimData.xC`, run by `ftAction_80073240` each frame),
//! read for the frames they act on.
//!
//! A script is 4-byte commands whose top six bits are the opcode. Opcodes
//! below 10 are the generic ones of `Command_Execute` (`lbcommand.c`): 0
//! ends, 1 waits a number of frames, 2 waits until a frame, and 5 and 7
//! carry a pointer in a second word. The rest are the fighter's own, with
//! lengths in words from `ftAction_803C0870`.

use dat_parser::DatFile;
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};

/// Each generic opcode's length in words (`DAT_SCRIPT` on
/// `Fighter_WaitAnimData.xC`).
const GENERIC_LENGTHS: [u32; 10] = [1, 1, 1, 1, 1, 2, 1, 2, 1, 1];

/// `ftAction_803C0870`: each fighter opcode's length in words, from 10.
const FIGHTER_LENGTHS: [u32; 49] = [
    5, 5, 1, 1, 1, 1, 1, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3, 1, 1, 1, 7, 4, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 3, 3, 2, 1, 4,
];

/// `set_cmd_var` (`ftAction_80071820`): sets one of `fp->cmd_vars`.
const SET_CMD_VAR: u32 = 0x13;

/// The most commands a script is read for: stock scripts are far shorter.
const MAX_COMMANDS: usize = 1024;

/// The size of an animation record (`Fighter_WaitAnimData`).
const RECORD_SIZE: u32 = 0x18;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScriptError {
    #[error("missing or ambiguous ftData public root")]
    Root,
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("script opcode {0:#x} is no command")]
    Opcode(u32),
}

/// The frames of animation `animation`'s script, in `fighter` (its data
/// file), at which it sets `cmd_vars[var]` to something other than zero:
/// Fox's neutral special fires a laser at each frame it sets the third.
/// Reading stops at the script's end, or where it loops, jumps or calls,
/// which the frames before already cover.
pub fn cmd_var_frames(
    fighter: &DatFile,
    animation: usize,
    var: u32,
) -> Result<Vec<f32>, ScriptError> {
    let mut roots = fighter
        .roots
        .iter()
        .filter(|root| root.name.starts_with("ftData"));
    let root = roots.next().ok_or(ScriptError::Root)?;
    if roots.next().is_some() {
        return Err(ScriptError::Root);
    }
    let ft_data = DescriptorReader::new(fighter, "ftData", root.data_offset);
    let Some(table) = ft_data.pointer("ftData.xC", 0x0c)? else {
        return Ok(Vec::new());
    };
    let record = DescriptorReader::new(fighter, "Fighter_WaitAnimData", table);
    let Some(script) = record.pointer(
        "Fighter_WaitAnimData.xC",
        animation as u32 * RECORD_SIZE + 0xc,
    )?
    else {
        return Ok(Vec::new());
    };
    let script = DescriptorReader::new(fighter, "move script", script);

    let mut frames = Vec::new();
    let mut frame = 0.0;
    let mut at = 0;
    for _ in 0..MAX_COMMANDS {
        let word = script.u32(at)?;
        let opcode = word >> 26;
        let value = word & 0x03ff_ffff;
        let length = match opcode {
            // End, or control flow the frames so far already cover.
            0 | 3..=7 => break,
            // Wait so many frames, or until a frame.
            1 => {
                frame += value as f32;
                1
            }
            2 => {
                frame = f32::max(frame, value as f32);
                1
            }
            SET_CMD_VAR => {
                if (word >> 24) & 3 == var && word & 0x00ff_ffff != 0 {
                    frames.push(frame);
                }
                1
            }
            opcode if opcode < 10 => GENERIC_LENGTHS[opcode as usize],
            opcode => *FIGHTER_LENGTHS
                .get(opcode as usize - 10)
                .ok_or(ScriptError::Opcode(opcode))?,
        };
        at += length * 4;
    }
    Ok(frames)
}
