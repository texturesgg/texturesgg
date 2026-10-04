use super::HsdSceneError;
use crate::DatFile;
use crate::descriptor::map_head::MapHead;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModelRootOrigin {
    Character { root_index: usize },
    Stage { descriptor_index: usize },
    Given { index: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveredModelRoot {
    pub(crate) offset: u32,
    pub(crate) require_renderable: bool,
    pub(crate) origin: ModelRootOrigin,
}

/// Discover HSD model roots in deterministic source order.
///
/// A `map_head` makes the DAT a stage, whose models are its model groups.
/// Stage DATs can also export loose `_joint` roots (Fountain of Dreams and
/// Poke Floats carry a star the game spawns from code); those are not part of
/// the stage as it loads.
pub(crate) fn discover_model_roots(
    dat: &DatFile,
    max_roots: usize,
) -> Result<Vec<DiscoveredModelRoot>, HsdSceneError> {
    match MapHead::find(dat) {
        Some(map_head) => discover_stage_roots(&map_head, max_roots),
        None => discover_character_roots(dat, max_roots),
    }
}

fn discover_character_roots(
    dat: &DatFile,
    max_roots: usize,
) -> Result<Vec<DiscoveredModelRoot>, HsdSceneError> {
    let character_roots = dat
        .roots
        .iter()
        .enumerate()
        .filter(|(_, root)| root.name.ends_with("_joint") && !root.name.contains("matanim"))
        .map(|(root_index, root)| DiscoveredModelRoot {
            offset: root.data_offset,
            require_renderable: false,
            origin: ModelRootOrigin::Character { root_index },
        })
        .collect::<Vec<_>>();
    if character_roots.len() > max_roots {
        return Err(HsdSceneError::LimitExceeded {
            resource: "model root",
            limit: max_roots,
        });
    }
    Ok(character_roots)
}

fn discover_stage_roots(
    map_head: &MapHead<'_>,
    max_roots: usize,
) -> Result<Vec<DiscoveredModelRoot>, HsdSceneError> {
    if map_head.model_group_count()? > max_roots {
        return Err(HsdSceneError::LimitExceeded {
            resource: "stage descriptor",
            limit: max_roots,
        });
    }
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for group in map_head.model_groups()? {
        let Some(root_offset) = group.root()? else {
            continue;
        };
        if seen.insert(root_offset) {
            roots.push(DiscoveredModelRoot {
                offset: root_offset,
                require_renderable: true,
                origin: ModelRootOrigin::Stage {
                    descriptor_index: group.index,
                },
            });
        }
    }
    Ok(roots)
}

/// The model roots a caller names: models the DAT's root table doesn't list,
/// which only the game's own structures lead to (a fighter's articles, an
/// effect table's models), in the caller's order.
pub(crate) fn given_model_roots(
    roots: &[u32],
    max_roots: usize,
) -> Result<Vec<DiscoveredModelRoot>, HsdSceneError> {
    if roots.len() > max_roots {
        return Err(HsdSceneError::LimitExceeded {
            resource: "model root",
            limit: max_roots,
        });
    }
    Ok(roots
        .iter()
        .enumerate()
        .map(|(index, &offset)| DiscoveredModelRoot {
            offset,
            require_renderable: false,
            origin: ModelRootOrigin::Given { index },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::DescriptorParseError;
    use crate::raw::root::RootNode;
    use crate::{DatExternError, DatPointerError};

    fn dat(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(
            data,
            vec![RootNode {
                name: "map_head".into(),
                data_offset: 0,
            }],
            relocation_sites,
        )
    }

    fn word(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn absent_roots_and_zero_counts_remain_empty() {
        let mut source = dat(vec![0; 0x10], vec![]);
        assert!(discover_model_roots(&source, 0).unwrap().is_empty());
        source.relocation_sites.push(8);
        // A relocated zero is a present table at data start, still with no entries.
        assert!(discover_model_roots(&source, 0).unwrap().is_empty());
        source.roots.clear();
        source.data.clear();
        assert!(discover_model_roots(&source, 0).unwrap().is_empty());
    }

    #[test]
    fn malformed_stage_headers_do_not_become_empty_scenes() {
        assert_eq!(
            discover_model_roots(&dat(vec![0; 0x0F], vec![]), 8).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::Truncated {
                descriptor: "MapHead",
                offset: 0,
            })
        );
        let mut source = dat(vec![0; 0x50], vec![]);
        word(&mut source.data, 0x0C, 1);
        assert!(matches!(
            discover_model_roots(&source, 8),
            Err(HsdSceneError::InvalidData { .. })
        ));
        word(&mut source.data, 8, 0x10);
        assert_eq!(
            discover_model_roots(&source, 8).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "MapHead",
                field: "model_groups",
                field_offset: 8,
                source: DatPointerError::MissingRelocation,
            })
        );
        source.relocation_sites.push(8);
        word(&mut source.data, 0x10, 0x20);
        assert_eq!(
            discover_model_roots(&source, 8).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "MapModelGroup",
                field: "root",
                field_offset: 0x10,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn character_roots_are_the_joint_roots_within_their_budget() {
        let mut source = dat(vec![], vec![]);
        source.roots[0].name = "model_joint".into();
        assert_eq!(
            discover_model_roots(&source, 1).unwrap(),
            vec![DiscoveredModelRoot {
                offset: 0,
                require_renderable: false,
                origin: ModelRootOrigin::Character { root_index: 0 },
            }]
        );
        assert!(matches!(
            discover_model_roots(&source, 0),
            Err(HsdSceneError::LimitExceeded {
                resource: "model root",
                limit: 0
            })
        ));
    }

    #[test]
    fn a_map_head_makes_a_stage_despite_loose_joint_roots() {
        // Fountain of Dreams exports a star model beside its map_head.
        let mut source = dat(vec![0; 0x80], vec![0x08]);
        word(&mut source.data, 0x08, 0x30);
        word(&mut source.data, 0x0C, 1);
        word(&mut source.data, 0x30, 0x70);
        source.relocation_sites.push(0x30);
        source.roots.push(RootNode {
            name: "GrdIzumiStar_TopN_joint".into(),
            data_offset: 0x20,
        });
        assert_eq!(
            discover_model_roots(&source, 1).unwrap(),
            vec![DiscoveredModelRoot {
                offset: 0x70,
                require_renderable: true,
                origin: ModelRootOrigin::Stage {
                    descriptor_index: 0
                },
            }]
        );
    }

    #[test]
    fn stage_roots_preserve_external_nulls_zero_targets_order_and_deduplication() {
        let mut source = dat(vec![0; 0x200], vec![0x48, 0xA4, 0xD8, 0x140]);
        source.roots[0].data_offset = 0x40;
        word(&mut source.data, 0x48, 0x70);
        word(&mut source.data, 0x4C, 5);
        word(&mut source.data, 0x50, 0x70);
        word(&mut source.data, 0x70, u32::MAX);
        word(&mut source.data, 0x140, 0x180);
        source.externs.push(RootNode {
            name: "external_model".into(),
            data_offset: 0x50,
        });
        let expected = vec![
            DiscoveredModelRoot {
                offset: 0,
                require_renderable: true,
                origin: ModelRootOrigin::Stage {
                    descriptor_index: 1,
                },
            },
            DiscoveredModelRoot {
                offset: 0x180,
                require_renderable: true,
                origin: ModelRootOrigin::Stage {
                    descriptor_index: 4,
                },
            },
        ];
        assert_eq!(discover_model_roots(&source, 5).unwrap(), expected);
        assert_eq!(source.read_u32(0x70), Some(u32::MAX));
        assert!(matches!(
            discover_model_roots(&source, 4),
            Err(HsdSceneError::LimitExceeded {
                resource: "stage descriptor",
                limit: 4
            })
        ));
        word(&mut source.data, 0x4C, 8);
        assert!(matches!(
            discover_model_roots(&source, 8),
            Err(HsdSceneError::InvalidData { .. })
        ));
        word(&mut source.data, 0x4C, 5);
        word(&mut source.data, 0x70, 0x50);
        assert_eq!(
            discover_model_roots(&source, 5).unwrap_err(),
            HsdSceneError::ExternalFixup(DatExternError::RepeatedSite { site: 0x50 })
        );
    }
}
