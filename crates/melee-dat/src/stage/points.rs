//! A stage's general points (the joints `map_head` names as spawn points,
//! the camera range, and the blast zone) and HAL's names for its images.
//!
//! `map_head` lists one record per model group that carries points: the
//! group's root joint and `(joint index, kind)` pairs. `Ground_801C3260`
//! (ground.c) walks the root's tree depth-first to each index, without
//! entering an INSTANCE joint's children, and `Ground_801C2D24` reads a
//! kind's position as that joint's world translation.
//!
//! Positions here are in the bind pose and in the DAT's own units, before
//! the scale the game gives each stage at load (Battlefield's is 0.8), so
//! they line up with the scene's geometry. Stages that move their camera
//! range (Rainbow Cruise, Poke Floats) animate these joints.

use dat_parser::DatFile;
use dat_parser::descriptor::jobj::{self, JObj};
use dat_parser::descriptor::map_head::{MapHead, MapHeadError};
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};
use dat_parser::hsd::source::HsdFocus;
use dat_parser::math::Mat4;

/// What a general point marks, as `Ground_801C2D24`'s callers number it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StagePointKind(pub i16);

impl StagePointKind {
    /// Two opposite corners of the camera range.
    pub const CAMERA_CORNERS: [Self; 2] = [Self(0x95), Self(0x96)];
}

const MAX_RECORDS: usize = 64;

const MAX_POINTS: usize = 4096;

const MAX_JOINTS: usize = 8192;

#[derive(Debug, thiserror::Error, PartialEq)]
#[non_exhaustive]
pub enum StagePointsError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error(transparent)]
    MapHead(#[from] MapHeadError),
    #[error("stage general points exceed the {resource} limit")]
    LimitExceeded { resource: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StagePoint {
    pub kind: StagePointKind,
    /// World position in the bind pose.
    pub position: [f32; 3],
}

/// An axis-aligned rectangle on the stage's XY plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageRect {
    pub left: f32,
    pub right: f32,
    pub bottom: f32,
    pub top: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StagePoints {
    pub points: Vec<StagePoint>,
}

impl StagePoints {
    /// Read every general point. A DAT without `map_head` has none.
    pub fn read(dat: &DatFile) -> Result<Self, StagePointsError> {
        let Some(map_head) = MapHead::find(dat) else {
            return Ok(Self::default());
        };
        let record_count = map_head.general_point_count()?;
        if record_count > MAX_RECORDS {
            return Err(StagePointsError::LimitExceeded { resource: "record" });
        }

        let mut points = Vec::new();
        for record in map_head.general_points()? {
            let pair_count = record.pair_count;
            let (Some(root), Some(pairs)) = (record.joint, record.pairs) else {
                continue;
            };
            if points.len().saturating_add(pair_count) > MAX_POINTS {
                return Err(StagePointsError::LimitExceeded { resource: "point" });
            }
            let positions = joint_positions(dat, root)?;
            let pairs = DescriptorReader::new(dat, "MapGeneralPointPairs", pairs);
            for pair in 0..pair_count as u32 {
                // Both fields are s16 in the file.
                let joint = pairs.u16(pair * 4)? as i16;
                let kind = StagePointKind(pairs.u16(pair * 4 + 2)? as i16);
                // The source leaves a kind unset when its index is past the tree.
                let position = usize::try_from(joint)
                    .ok()
                    .and_then(|joint| positions.get(joint));
                if let Some(&position) = position {
                    points.push(StagePoint { kind, position });
                }
            }
        }
        Ok(Self { points })
    }

    /// A kind's position. When records repeat a kind, the last one wins, as
    /// the source overwrites its table in record order.
    pub fn position(&self, kind: StagePointKind) -> Option<[f32; 3]> {
        self.points
            .iter()
            .rev()
            .find(|point| point.kind == kind)
            .map(|point| point.position)
    }

    /// The range the in-game camera keeps fighters within.
    pub fn camera_range(&self) -> Option<StageRect> {
        self.rect(StagePointKind::CAMERA_CORNERS)
    }

    fn rect(&self, corners: [StagePointKind; 2]) -> Option<StageRect> {
        let [a, b] = corners.map(|corner| self.position(corner));
        let (a, b) = (a?, b?);
        Some(StageRect {
            left: a[0].min(b[0]),
            right: a[0].max(b[0]),
            bottom: a[1].min(b[1]),
            top: a[1].max(b[1]),
        })
    }
}

/// World translations of a joint tree in the source's depth-first order.
fn joint_positions(dat: &DatFile, root: u32) -> Result<Vec<[f32; 3]>, StagePointsError> {
    let mut positions = Vec::new();
    // (joint offset, parent world matrix); siblings share their parent's.
    let mut pending = vec![(root, Mat4::identity())];
    while let Some((offset, parent)) = pending.pop() {
        if positions.len() == MAX_JOINTS {
            return Err(StagePointsError::LimitExceeded { resource: "joint" });
        }
        let joint = JObj::parse(dat, offset)?;
        let world = parent.mul(&Mat4::from_srt(
            joint.scale,
            joint.rotation,
            joint.translation,
        ));
        positions.push(world.transform_point([0.0; 3]));
        // The next sibling is visited after this joint's whole subtree.
        if let Some(next) = joint.next_ptr {
            pending.push((next, parent));
        }
        if joint.flags & jobj::flags::INSTANCE == 0
            && let Some(child) = joint.child_ptr
        {
            pending.push((child, world));
        }
    }
    Ok(positions)
}

/// A stage's camera range in the bind pose. A stage whose points do not read
/// is framed whole, like any other model.
pub fn camera_focus(dat: &DatFile) -> Option<HsdFocus> {
    let range = StagePoints::read(dat).ok()?.camera_range()?;
    let half_width = (range.right - range.left) / 2.0;
    let half_height = (range.top - range.bottom) / 2.0;
    (half_width.is_finite() && half_height.is_finite() && half_width > 0.0 && half_height > 0.0)
        .then_some(HsdFocus {
            center: [
                (range.left + range.right) / 2.0,
                (range.bottom + range.top) / 2.0,
                0.0,
            ],
            half_width,
            half_height,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stage::test_support::dat_with_roots_and_relocations;
    use dat_parser::raw::root::RootNode;

    fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_joint(data: &mut [u8], offset: usize, child: u32, next: u32, translation: [f32; 3]) {
        write_u32(data, offset + 0x08, child);
        write_u32(data, offset + 0x0C, next);
        for (axis, value) in translation.into_iter().enumerate() {
            write_u32(data, offset + 0x20 + axis * 4, 1f32.to_bits());
            write_u32(data, offset + 0x2C + axis * 4, value.to_bits());
        }
    }

    /// A map_head with one record over a three-joint tree:
    /// root(10, 0, 0) -> child(1, 2, 0) -> next sibling(-5, 7, 0).
    fn stage(pairs: &[(i16, i16)]) -> DatFile {
        const HEAD: usize = 0x04;
        const RECORD: usize = 0x40;
        const PAIRS: usize = 0x50;
        const ROOT: usize = 0x80;
        const CHILD: usize = 0xC0;
        const SIBLING: usize = 0x100;
        let mut data = vec![0u8; 0x140];
        write_u32(&mut data, HEAD, RECORD as u32);
        write_u32(&mut data, HEAD + 0x04, 1);
        write_u32(&mut data, RECORD, ROOT as u32);
        write_u32(&mut data, RECORD + 0x04, PAIRS as u32);
        write_u32(&mut data, RECORD + 0x08, pairs.len() as u32);
        for (index, (joint, kind)) in pairs.iter().enumerate() {
            data[PAIRS + index * 4..][..2].copy_from_slice(&joint.to_be_bytes());
            data[PAIRS + index * 4 + 2..][..2].copy_from_slice(&kind.to_be_bytes());
        }
        write_joint(&mut data, ROOT, CHILD as u32, 0, [10.0, 0.0, 0.0]);
        write_joint(&mut data, CHILD, 0, SIBLING as u32, [1.0, 2.0, 0.0]);
        write_joint(&mut data, SIBLING, 0, 0, [-5.0, 7.0, 0.0]);
        dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: HEAD as u32,
                name: "map_head".into(),
            }],
            vec![
                HEAD as u32,
                RECORD as u32,
                RECORD as u32 + 0x04,
                ROOT as u32 + 0x08,
                CHILD as u32 + 0x0C,
            ],
        )
    }

    #[test]
    fn points_take_their_joints_world_position_in_depth_first_order() {
        let points = StagePoints::read(&stage(&[(1, 0x95), (2, 0x96), (0, 0x94)])).unwrap();
        let position = |kind| points.position(StagePointKind(kind));
        assert_eq!(position(0x94), Some([10.0, 0.0, 0.0]));
        assert_eq!(position(0x95), Some([11.0, 2.0, 0.0]));
        assert_eq!(position(0x96), Some([5.0, 7.0, 0.0]));
        assert_eq!(
            points.camera_range(),
            Some(StageRect {
                left: 5.0,
                right: 11.0,
                bottom: 2.0,
                top: 7.0,
            })
        );
    }

    #[test]
    fn an_index_past_the_tree_leaves_its_kind_unset() {
        let points = StagePoints::read(&stage(&[(3, 0x95), (-1, 0x96)])).unwrap();
        assert!(points.points.is_empty());
    }
}
