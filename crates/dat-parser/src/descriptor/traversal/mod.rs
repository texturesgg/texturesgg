pub mod material_animation;

use core::fmt;
use std::collections::HashSet;

use hal_dat_raw::DatFile;

use crate::descriptor::{DescriptorParseError, dobj::DObj, jobj::JObj, pobj::PObj, tobj::TObj};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorKind {
    JObj,
    DObj,
    PObj,
    TObj,
    MatAnimJoint,
    MatAnim,
    TexAnim,
    AObj,
    FObj,
}

impl fmt::Display for DescriptorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DescriptorKind::JObj => write!(f, "JObj"),
            DescriptorKind::DObj => write!(f, "DObj"),
            DescriptorKind::PObj => write!(f, "PObj"),
            DescriptorKind::TObj => write!(f, "TObj"),
            DescriptorKind::MatAnimJoint => write!(f, "MatAnimJoint"),
            DescriptorKind::MatAnim => write!(f, "MatAnim"),
            DescriptorKind::TexAnim => write!(f, "TexAnim"),
            DescriptorKind::AObj => write!(f, "AObj"),
            DescriptorKind::FObj => write!(f, "FObj"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TraversalIssue {
    #[error("failed to parse {kind:?} at offset {offset:#010x}: {source}")]
    Descriptor {
        kind: DescriptorKind,
        offset: u32,

        #[source]
        source: DescriptorParseError,
    },

    #[error("repeated {kind:?} pointer at {offset:#010x}")]
    RepeatedPointer { kind: DescriptorKind, offset: u32 },

    #[error("{kind:?} traversal exceeds the node budget of {limit}")]
    LimitExceeded { kind: DescriptorKind, limit: usize },
}

#[must_use = "traversal issues must be inspected"]
#[derive(Clone, Debug)]
pub struct TraversalOutcome<T> {
    pub nodes: Vec<T>,
    pub issues: Vec<TraversalIssue>,
}

#[derive(Clone, Debug)]
pub struct SourceJoint {
    pub index: usize,
    pub parent_index: Option<usize>,
    pub jobj: JObj,
    /// Owned children only. INSTANCE.child remains a reference in `jobj.child_ptr`.
    pub children: Vec<usize>,
}

pub fn walk_joint_tree(
    dat: &DatFile,
    root_offset: u32,
    max_joints: usize,
) -> TraversalOutcome<SourceJoint> {
    let mut nodes = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![(root_offset, None)];

    while let Some((offset, parent_index)) = pending.pop() {
        if !seen.insert(offset) {
            issues.push(TraversalIssue::RepeatedPointer {
                kind: DescriptorKind::JObj,
                offset,
            });
            continue;
        }

        if nodes.len() >= max_joints {
            issues.push(TraversalIssue::LimitExceeded {
                kind: DescriptorKind::JObj,
                limit: max_joints,
            });
            break;
        }

        let jobj = match JObj::parse(dat, offset) {
            Ok(jobj) => jobj,
            Err(source) => {
                issues.push(TraversalIssue::Descriptor {
                    kind: DescriptorKind::JObj,
                    offset,
                    source,
                });
                continue;
            }
        };

        let index = nodes.len();
        let child_ptr = if jobj.flags & crate::descriptor::jobj::flags::INSTANCE == 0 {
            jobj.child_ptr
        } else {
            None
        };
        let next_ptr = jobj.next_ptr;

        nodes.push(SourceJoint {
            index,
            parent_index,
            jobj,
            children: Vec::new(),
        });

        if let Some(parent_index) = parent_index {
            nodes[parent_index].children.push(index);
        }

        // LIFO: push siblings first so the child is processed first.
        if let Some(next_offset) = next_ptr {
            pending.push((next_offset, parent_index));
        }
        if let Some(child_offset) = child_ptr {
            pending.push((child_offset, Some(index)));
        }
    }

    TraversalOutcome { nodes, issues }
}

pub(crate) trait LinkedDescriptor: Sized {
    const KIND: DescriptorKind;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError>;

    fn next_offset(&self) -> Option<u32>;
}

pub(crate) struct TraversalBudget {
    visited_nodes: usize,
    max_nodes: usize,
    exhausted: bool,
}

impl TraversalBudget {
    pub(crate) fn new(max_nodes: usize) -> Self {
        Self {
            visited_nodes: 0,
            max_nodes,
            exhausted: false,
        }
    }

    pub(crate) fn charge(
        &mut self,
        kind: DescriptorKind,
        issues: &mut Vec<TraversalIssue>,
    ) -> bool {
        if self.exhausted {
            return false;
        }
        if self.visited_nodes >= self.max_nodes {
            self.exhausted = true;
            issues.push(TraversalIssue::LimitExceeded {
                kind,
                limit: self.max_nodes,
            });
            return false;
        }
        self.visited_nodes += 1;
        true
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.exhausted
    }
}

pub(crate) fn read_linked_list_with_budget<T: LinkedDescriptor>(
    dat: &DatFile,
    start_offset: Option<u32>,
    budget: &mut TraversalBudget,
) -> TraversalOutcome<T> {
    let mut nodes = Vec::new();
    let mut issues = Vec::new();
    let mut seen = HashSet::new();
    let mut current = start_offset;

    while let Some(offset) = current {
        if !seen.insert(offset) {
            issues.push(TraversalIssue::RepeatedPointer {
                kind: T::KIND,
                offset,
            });
            break;
        }
        if !budget.charge(T::KIND, &mut issues) {
            break;
        }

        let node = match T::parse_at(dat, offset) {
            Ok(node) => node,
            Err(source) => {
                issues.push(TraversalIssue::Descriptor {
                    kind: T::KIND,
                    offset,
                    source,
                });
                break;
            }
        };

        current = node.next_offset();
        nodes.push(node);
    }

    TraversalOutcome { nodes, issues }
}

impl LinkedDescriptor for PObj {
    const KIND: DescriptorKind = DescriptorKind::PObj;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        PObj::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

impl LinkedDescriptor for TObj {
    const KIND: DescriptorKind = DescriptorKind::TObj;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        TObj::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

impl LinkedDescriptor for DObj {
    const KIND: DescriptorKind = DescriptorKind::DObj;

    fn parse_at(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        DObj::parse(dat, offset)
    }

    fn next_offset(&self) -> Option<u32> {
        self.next_ptr
    }
}

fn read_linked_list<T: LinkedDescriptor>(
    dat: &DatFile,
    start_offset: u32,
    max_nodes: usize,
) -> TraversalOutcome<T> {
    read_linked_list_with_budget(
        dat,
        Some(start_offset),
        &mut TraversalBudget::new(max_nodes),
    )
}

pub fn read_dobj_list(
    dat: &DatFile,
    start_offset: u32,
    max_nodes: usize,
) -> TraversalOutcome<DObj> {
    read_linked_list(dat, start_offset, max_nodes)
}

pub fn read_pobj_list(
    dat: &DatFile,
    start_offset: u32,
    max_nodes: usize,
) -> TraversalOutcome<PObj> {
    read_linked_list(dat, start_offset, max_nodes)
}

pub fn read_tobj_list(
    dat: &DatFile,
    start_offset: u32,
    max_nodes: usize,
) -> TraversalOutcome<TObj> {
    read_linked_list(dat, start_offset, max_nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn joint_tree_preserves_child_before_sibling_order() {
        let mut data = vec![0; 0xc0];
        write_u32(&mut data, 0x08, 0x40);
        write_u32(&mut data, 0x0c, 0x80);
        let dat = dat_with_relocations(data, vec![0x08, 0x0c]);

        let outcome = walk_joint_tree(&dat, 0, 3);

        assert!(outcome.issues.is_empty());
        assert_eq!(
            outcome
                .nodes
                .iter()
                .map(|joint| (joint.index, joint.parent_index, joint.jobj.offset))
                .collect::<Vec<_>>(),
            [(0, None, 0), (1, Some(0), 0x40), (2, None, 0x80)]
        );
        assert_eq!(outcome.nodes[0].children, [1]);
        assert!(outcome.nodes[1].children.is_empty());
        assert!(outcome.nodes[2].children.is_empty());
    }

    #[test]
    fn instance_references_do_not_load_or_reparent_their_targets() {
        let mut data = vec![0; 0xC0];
        write_u32(&mut data, 0x08, 0x40);
        write_u32(&mut data, 0x4C, 0x80);
        write_u32(&mut data, 0x84, crate::descriptor::jobj::flags::INSTANCE);
        write_u32(&mut data, 0x88, 0x40);
        let mut dat = dat_with_relocations(data, vec![0x08, 0x4C, 0x88]);
        let outcome = walk_joint_tree(&dat, 0, 3);
        assert!(outcome.issues.is_empty());
        assert_eq!(outcome.nodes.len(), 3);
        assert_eq!(outcome.nodes[0].children, [1, 2]);
        assert_eq!(outcome.nodes[1].parent_index, Some(0));
        assert_eq!(outcome.nodes[2].jobj.child_ptr, Some(0x40));
        assert!(outcome.nodes[2].children.is_empty());

        // Resolution belongs to the load context, not the owned traversal.
        write_u32(&mut dat.data, 0x88, 0xC0);
        let unresolved = walk_joint_tree(&dat, 0, 3);
        assert!(unresolved.issues.is_empty());
        assert_eq!(unresolved.nodes.len(), 3);
        assert_eq!(unresolved.nodes[2].jobj.child_ptr, Some(0xC0));
    }

    #[test]
    fn joint_tree_reports_repeated_pointer() {
        let mut data = vec![0; 0x40];
        write_u32(&mut data, 0x08, 0);
        let dat = dat_with_relocations(data, vec![0x08]);

        let outcome = walk_joint_tree(&dat, 0, 2);

        assert_eq!(outcome.nodes.len(), 1);
        assert!(outcome.nodes[0].children.is_empty());
        assert_eq!(
            outcome.issues,
            [TraversalIssue::RepeatedPointer {
                kind: DescriptorKind::JObj,
                offset: 0,
            }]
        );
    }

    #[test]
    fn joint_tree_retains_parse_error_and_continues_other_branches() {
        let mut data = vec![0; 0x80];
        write_u32(&mut data, 0x08, 0x80);
        write_u32(&mut data, 0x0c, 0x40);
        let dat = dat_with_relocations(data, vec![0x08, 0x0c]);

        let outcome = walk_joint_tree(&dat, 0, 3);

        assert_eq!(
            outcome
                .nodes
                .iter()
                .map(|joint| (joint.parent_index, joint.jobj.offset))
                .collect::<Vec<_>>(),
            [(None, 0), (None, 0x40)]
        );
        assert!(outcome.nodes[0].children.is_empty());
        assert_eq!(
            outcome.issues,
            [TraversalIssue::Descriptor {
                kind: DescriptorKind::JObj,
                offset: 0x80,
                source: DescriptorParseError::Truncated {
                    descriptor: "JObj",
                    offset: 0x80,
                },
            }]
        );
    }

    #[test]
    fn joint_tree_enforces_exact_node_limit() {
        let mut data = vec![0; 0x80];
        write_u32(&mut data, 0x08, 0x40);
        let dat = dat_with_relocations(data, vec![0x08]);

        let complete = walk_joint_tree(&dat, 0, 2);
        assert_eq!(complete.nodes.len(), 2);
        assert!(complete.issues.is_empty());

        let limited = walk_joint_tree(&dat, 0, 1);
        assert_eq!(limited.nodes.len(), 1);
        assert_eq!(
            limited.issues,
            [TraversalIssue::LimitExceeded {
                kind: DescriptorKind::JObj,
                limit: 1,
            }]
        );

        let empty = walk_joint_tree(&dat, 0, 0);
        assert!(empty.nodes.is_empty());
        assert_eq!(
            empty.issues,
            [TraversalIssue::LimitExceeded {
                kind: DescriptorKind::JObj,
                limit: 0,
            }]
        );
    }

    fn assert_linked_list_contract<T>(
        descriptor_size: usize,
        descriptor_name: &'static str,
        kind: DescriptorKind,
        read: impl Fn(&DatFile, u32, usize) -> TraversalOutcome<T>,
        node_offset: impl Fn(&T) -> u32,
    ) {
        let second_offset = descriptor_size as u32;
        let mut complete_data = vec![0; descriptor_size * 2];
        write_u32(&mut complete_data, 0x04, second_offset);
        let complete_dat = dat_with_relocations(complete_data, vec![0x04]);

        let complete = read(&complete_dat, 0, 2);
        assert_eq!(
            complete.nodes.iter().map(&node_offset).collect::<Vec<_>>(),
            [0, second_offset],
            "{kind:?} source order"
        );
        assert!(complete.issues.is_empty(), "{kind:?} exact limit");

        let limited = read(&complete_dat, 0, 1);
        assert_eq!(
            limited.nodes.iter().map(&node_offset).collect::<Vec<_>>(),
            [0],
            "{kind:?} retained limit prefix"
        );
        assert_eq!(
            limited.issues,
            [TraversalIssue::LimitExceeded { kind, limit: 1 }],
            "{kind:?} limit issue"
        );

        let mut cycle_data = vec![0; descriptor_size];
        write_u32(&mut cycle_data, 0x04, 0);
        let cycle_dat = dat_with_relocations(cycle_data, vec![0x04]);
        let cycle = read(&cycle_dat, 0, 2);
        assert_eq!(cycle.nodes.len(), 1, "{kind:?} retained cycle prefix");
        assert_eq!(
            cycle.issues,
            [TraversalIssue::RepeatedPointer { kind, offset: 0 }],
            "{kind:?} repeated pointer"
        );

        let mut truncated_data = vec![0; descriptor_size + 1];
        write_u32(&mut truncated_data, 0x04, second_offset);
        let truncated_dat = dat_with_relocations(truncated_data, vec![0x04]);
        let truncated = read(&truncated_dat, 0, 2);
        assert_eq!(
            truncated.nodes.iter().map(&node_offset).collect::<Vec<_>>(),
            [0],
            "{kind:?} retained malformed prefix"
        );
        assert_eq!(
            truncated.issues,
            [TraversalIssue::Descriptor {
                kind,
                offset: second_offset,
                source: DescriptorParseError::Truncated {
                    descriptor: descriptor_name,
                    offset: second_offset,
                },
            }],
            "{kind:?} descriptor issue"
        );
    }

    #[test]
    fn linked_descriptor_walkers_share_order_cycle_error_and_limit_contracts() {
        assert_linked_list_contract(0x10, "DObj", DescriptorKind::DObj, read_dobj_list, |node| {
            node.offset
        });
        assert_linked_list_contract(0x18, "PObj", DescriptorKind::PObj, read_pobj_list, |node| {
            node.offset
        });
        assert_linked_list_contract(0x5c, "TObj", DescriptorKind::TObj, read_tobj_list, |node| {
            node.offset
        });
    }
}
