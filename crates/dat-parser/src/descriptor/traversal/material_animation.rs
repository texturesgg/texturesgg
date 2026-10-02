//! Bounded material-animation graph traversal with valid-prefix outcomes.
//!
//! Each linked-list prefix is validated and budgeted before its nested animation references.
//! Joint-level structural issues stop the preorder prefix because audit consumers pair it by
//! position with the model joint tree.

use std::collections::HashSet;

use super::{
    DescriptorKind, LinkedDescriptor, TraversalBudget, TraversalIssue, TraversalOutcome,
    read_linked_list_with_budget,
};
use crate::DatFile;
use crate::descriptor::DescriptorParseError;
use crate::descriptor::material_animation::{AObj, FObj, MatAnim, MatAnimJoint, TexAnim};

impl LinkedDescriptor for MatAnim {
    const KIND: DescriptorKind = DescriptorKind::MatAnim;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        Self::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

impl LinkedDescriptor for TexAnim {
    const KIND: DescriptorKind = DescriptorKind::TexAnim;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        Self::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

impl LinkedDescriptor for FObj {
    const KIND: DescriptorKind = DescriptorKind::FObj;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        Self::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

#[derive(Clone, Debug)]
pub struct SourceMatAnimJoint {
    /// Index of the parent node in the walk's preorder list, so consumers can
    /// check that this tree mirrors the JObj tree it animates.
    pub parent_index: Option<usize>,
    pub descriptor: MatAnimJoint,
    pub material_animations: Vec<SourceMatAnim>,
}

#[derive(Clone, Debug)]
pub struct SourceMatAnim {
    pub descriptor: MatAnim,
    pub animation: Option<SourceAnimation>,
    pub texture_animations: Vec<SourceTexAnim>,
}

#[derive(Clone, Debug)]
pub struct SourceTexAnim {
    pub descriptor: TexAnim,
    pub animation: Option<SourceAnimation>,
}

#[derive(Clone, Debug)]
pub struct SourceAnimation {
    pub descriptor: AObj,
    pub tracks: Vec<FObj>,
}

fn retain_prefix<T>(outcome: TraversalOutcome<T>, issues: &mut Vec<TraversalIssue>) -> Vec<T> {
    issues.extend(outcome.issues);
    outcome.nodes
}

pub fn walk_material_animation_tree(
    dat: &DatFile,
    root_offset: u32,
    max_nodes: usize,
) -> TraversalOutcome<SourceMatAnimJoint> {
    let mut nodes = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![(root_offset, None)];
    let mut budget = TraversalBudget::new(max_nodes);

    while let Some((offset, parent_index)) = pending.pop() {
        if !seen.insert(offset) {
            issues.push(TraversalIssue::RepeatedPointer {
                kind: DescriptorKind::MatAnimJoint,
                offset,
            });
            break;
        }
        if !budget.charge(DescriptorKind::MatAnimJoint, &mut issues) {
            break;
        }

        let descriptor = match MatAnimJoint::parse(dat, offset) {
            Ok(descriptor) => descriptor,
            Err(source) => {
                issues.push(TraversalIssue::Descriptor {
                    kind: DescriptorKind::MatAnimJoint,
                    offset,
                    source,
                });
                break;
            }
        };
        let material_animations =
            parse_material_anim_list(dat, descriptor.material_anim_ptr, &mut budget, &mut issues);
        let child_ptr = descriptor.child_ptr;
        let next_ptr = descriptor.next_ptr;
        let index = nodes.len();
        nodes.push(SourceMatAnimJoint {
            parent_index,
            descriptor,
            material_animations,
        });

        if budget.is_exhausted() {
            break;
        }
        // LIFO: push siblings first so the child is walked first, matching
        // the JObj walk's preorder.
        if let Some(next_offset) = next_ptr {
            pending.push((next_offset, parent_index));
        }
        if let Some(child_offset) = child_ptr {
            pending.push((child_offset, Some(index)));
        }
    }

    TraversalOutcome { nodes, issues }
}

fn parse_material_anim_list(
    dat: &DatFile,
    start: Option<u32>,
    budget: &mut TraversalBudget,
    issues: &mut Vec<TraversalIssue>,
) -> Vec<SourceMatAnim> {
    let descriptors = retain_prefix(
        read_linked_list_with_budget::<MatAnim>(dat, start, budget),
        issues,
    );
    descriptors
        .into_iter()
        .map(|descriptor| {
            let animation = parse_animation(dat, descriptor.animation_ptr, budget, issues);
            let texture_animations =
                parse_texture_anim_list(dat, descriptor.texture_animation_ptr, budget, issues);
            SourceMatAnim {
                descriptor,
                animation,
                texture_animations,
            }
        })
        .collect()
}

fn parse_texture_anim_list(
    dat: &DatFile,
    start: Option<u32>,
    budget: &mut TraversalBudget,
    issues: &mut Vec<TraversalIssue>,
) -> Vec<SourceTexAnim> {
    let descriptors = retain_prefix(
        read_linked_list_with_budget::<TexAnim>(dat, start, budget),
        issues,
    );
    descriptors
        .into_iter()
        .map(|descriptor| {
            let animation = parse_animation(dat, descriptor.animation_ptr, budget, issues);
            SourceTexAnim {
                descriptor,
                animation,
            }
        })
        .collect()
}

fn parse_animation(
    dat: &DatFile,
    offset: Option<u32>,
    budget: &mut TraversalBudget,
    issues: &mut Vec<TraversalIssue>,
) -> Option<SourceAnimation> {
    let offset = offset?;
    if !budget.charge(DescriptorKind::AObj, issues) {
        return None;
    }

    let descriptor = match AObj::parse(dat, offset) {
        Ok(descriptor) => descriptor,
        Err(source) => {
            issues.push(TraversalIssue::Descriptor {
                kind: DescriptorKind::AObj,
                offset,
                source,
            });
            return None;
        }
    };
    let tracks = parse_fobj_list(dat, descriptor.fobj_ptr, budget, issues);
    Some(SourceAnimation { descriptor, tracks })
}

fn parse_fobj_list(
    dat: &DatFile,
    start: Option<u32>,
    budget: &mut TraversalBudget,
    issues: &mut Vec<TraversalIssue>,
) -> Vec<FObj> {
    retain_prefix(
        read_linked_list_with_budget::<FObj>(dat, start, budget),
        issues,
    )
}

#[cfg(test)]
mod tests {
    use super::walk_material_animation_tree;
    use crate::DatFile;
    use crate::descriptor::DescriptorParseError;
    use crate::descriptor::traversal::{DescriptorKind, TraversalIssue};

    fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn dat(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    #[test]
    fn material_list_cycle_retains_its_valid_prefix() {
        let mut data = vec![0u8; 0x80];
        write_u32(&mut data, 0x08, 0x20);
        write_u32(&mut data, 0x20, 0x20);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x08, 0x20]), 0, 128);

        assert_eq!(outcome.nodes.len(), 1);
        assert_eq!(outcome.nodes[0].material_animations.len(), 1);
        assert_eq!(
            outcome.nodes[0].material_animations[0].descriptor.offset,
            0x20
        );
        assert_eq!(
            outcome.issues,
            [TraversalIssue::RepeatedPointer {
                kind: DescriptorKind::MatAnim,
                offset: 0x20,
            }]
        );
    }

    #[test]
    fn texture_and_track_cycles_are_reported_independently() {
        let mut data = vec![0u8; 0x100];
        write_u32(&mut data, 0x08, 0x20);
        write_u32(&mut data, 0x20 + 0x08, 0x40);
        write_u32(&mut data, 0x40, 0x40);
        write_u32(&mut data, 0x40 + 0x08, 0x60);
        write_u32(&mut data, 0x60 + 0x08, 0x80);
        write_u32(&mut data, 0x80, 0x80);
        let relocation_sites = vec![0x08, 0x28, 0x40, 0x48, 0x68, 0x80];
        let outcome = walk_material_animation_tree(&dat(data, relocation_sites), 0, 128);

        assert_eq!(outcome.nodes[0].material_animations.len(), 1);
        assert_eq!(
            outcome.issues,
            [
                TraversalIssue::RepeatedPointer {
                    kind: DescriptorKind::TexAnim,
                    offset: 0x40,
                },
                TraversalIssue::RepeatedPointer {
                    kind: DescriptorKind::FObj,
                    offset: 0x80,
                },
            ]
        );
    }

    #[test]
    fn malformed_joint_stops_before_pending_siblings() {
        let mut data = vec![0u8; 0x60];
        write_u32(&mut data, 0x00, 0x20);
        write_u32(&mut data, 0x04, 0x40);
        write_u32(&mut data, 0x20, 0x50);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x00, 0x04]), 0, 128);

        assert_eq!(
            outcome
                .nodes
                .iter()
                .map(|joint| joint.descriptor.offset)
                .collect::<Vec<_>>(),
            [0]
        );
        assert!(matches!(
            outcome.issues.as_slice(),
            [TraversalIssue::Descriptor {
                kind: DescriptorKind::MatAnimJoint,
                offset: 0x20,
                ..
            }]
        ));
    }

    #[test]
    fn repeated_joint_stops_before_pending_siblings() {
        let mut data = vec![0u8; 0x60];
        write_u32(&mut data, 0x00, 0x20);
        write_u32(&mut data, 0x04, 0x40);
        write_u32(&mut data, 0x20 + 0x04, 0x20);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x00, 0x04, 0x24]), 0, 128);

        assert_eq!(
            outcome
                .nodes
                .iter()
                .map(|joint| joint.descriptor.offset)
                .collect::<Vec<_>>(),
            [0, 0x20]
        );
        assert_eq!(
            outcome.issues,
            [TraversalIssue::RepeatedPointer {
                kind: DescriptorKind::MatAnimJoint,
                offset: 0x20,
            }]
        );
    }

    #[test]
    fn malformed_material_descriptor_keeps_prior_entry() {
        let mut data = vec![0u8; 0x48];
        write_u32(&mut data, 0x08, 0x20);
        write_u32(&mut data, 0x20, 0x40);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x08, 0x20]), 0, 128);

        assert_eq!(outcome.nodes.len(), 1);
        assert_eq!(outcome.nodes[0].material_animations.len(), 1);
        assert_eq!(
            outcome.issues,
            [TraversalIssue::Descriptor {
                kind: DescriptorKind::MatAnim,
                offset: 0x40,
                source: DescriptorParseError::Truncated {
                    descriptor: "MatAnim",
                    offset: 0x40,
                },
            }]
        );
    }

    #[test]
    fn truncated_nested_descriptors_report_their_source_kind() {
        let mut texture_data = vec![0u8; 0x50];
        write_u32(&mut texture_data, 0x08, 0x20);
        write_u32(&mut texture_data, 0x20 + 0x08, 0x40);

        let mut aobj_data = vec![0u8; 0x48];
        write_u32(&mut aobj_data, 0x08, 0x20);
        write_u32(&mut aobj_data, 0x20 + 0x04, 0x40);

        let mut fobj_data = vec![0u8; 0x68];
        write_u32(&mut fobj_data, 0x08, 0x20);
        write_u32(&mut fobj_data, 0x20 + 0x04, 0x40);
        write_u32(&mut fobj_data, 0x40 + 0x08, 0x60);

        let cases = [
            (
                DescriptorKind::TexAnim,
                "TexAnim",
                0x40,
                texture_data,
                vec![0x08, 0x28],
            ),
            (
                DescriptorKind::AObj,
                "AObj",
                0x40,
                aobj_data,
                vec![0x08, 0x24],
            ),
            (
                DescriptorKind::FObj,
                "FObj",
                0x60,
                fobj_data,
                vec![0x08, 0x24, 0x48],
            ),
        ];

        for (kind, descriptor, offset, data, relocation_sites) in cases {
            let outcome = walk_material_animation_tree(&dat(data, relocation_sites), 0, 128);

            assert_eq!(outcome.nodes.len(), 1);
            assert_eq!(outcome.nodes[0].material_animations.len(), 1);
            assert_eq!(
                outcome.issues,
                [TraversalIssue::Descriptor {
                    kind,
                    offset,
                    source: DescriptorParseError::Truncated { descriptor, offset },
                }]
            );
        }
    }

    #[test]
    fn graph_node_budget_is_shared_across_nested_lists() {
        let mut data = vec![0u8; 0x60];
        write_u32(&mut data, 0x08, 0x20);
        write_u32(&mut data, 0x20 + 0x04, 0x40);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x08, 0x24]), 0, 2);

        assert_eq!(outcome.nodes.len(), 1);
        assert_eq!(outcome.nodes[0].material_animations.len(), 1);
        assert!(outcome.nodes[0].material_animations[0].animation.is_none());
        assert_eq!(
            outcome.issues,
            [TraversalIssue::LimitExceeded {
                kind: DescriptorKind::AObj,
                limit: 2,
            }]
        );
    }

    #[test]
    fn nodes_record_their_preorder_parent() {
        // 0x00 -> child 0x10 -> next 0x20; 0x10 -> child 0x30.
        let mut data = vec![0u8; 0x40];
        write_u32(&mut data, 0x00, 0x10);
        write_u32(&mut data, 0x10, 0x30);
        write_u32(&mut data, 0x14, 0x20);
        let outcome = walk_material_animation_tree(&dat(data, vec![0x00, 0x10, 0x14]), 0, 128);

        assert!(outcome.issues.is_empty());
        assert_eq!(
            outcome
                .nodes
                .iter()
                .map(|joint| (joint.descriptor.offset, joint.parent_index))
                .collect::<Vec<_>>(),
            [(0, None), (0x10, Some(0)), (0x30, Some(1)), (0x20, Some(0))]
        );
    }

    #[test]
    fn graph_node_budget_is_enforced_before_parsing() {
        let outcome = walk_material_animation_tree(&dat(vec![0u8; 0x20], Vec::new()), 0, 0);

        assert!(outcome.nodes.is_empty());
        assert_eq!(
            outcome.issues,
            [TraversalIssue::LimitExceeded {
                kind: DescriptorKind::MatAnimJoint,
                limit: 0,
            }]
        );
    }
}
