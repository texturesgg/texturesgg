//! Fighter model-part visibility: which of a costume's display objects the
//! game draws.
//!
//! A fighter costume carries alternative models for its parts (faces, hands,
//! items, detail levels) as ordinary DObjs. The fighter data (`PlXx.dat`)
//! groups them into visibility tables, and the game shows one alternative per
//! group. Source: the Melee decompilation's `ft/ftparts.c` (`ftParts_8007487C`,
//! `ftParts_800749CC`, `ftParts_80074A4C`, `ftParts_80074A8C`,
//! `ftParts_80074B6C`, `ftParts_800750C8`), `ft/ftdrawcommon.c`
//! (`ftDrawCommon_800805C8`), and each fighter's `ftXx_Init_OnDeath`.
//!
//! Display objects are addressed by their global ordinal in the costume's
//! preorder JObj list followed by each linked DObj list (`ftParts_80074E58`
//! builds `dobj_list` in that order). Ordinals no table names stay visible.

use dat_parser::DatFile;
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};

/// `ftParts_8007487C` asserts `model_num <= 11`.
pub const MAX_MODEL_GROUPS: usize = 11;
/// Bounds for hostile data; stock tables stay far below both.
const MAX_ALTERNATIVES: usize = 128;
const MAX_ORDINALS: usize = 255;
const TABLE_COUNT: usize = 5;

const FIGHTER_KIND_PEACH: u8 = 0x09;
const FIGHTER_KIND_PICHU: u8 = 0x17;
const FIGHTER_KIND_GAME_AND_WATCH: u8 = 0x18;
const FIGHTER_KIND_COUNT: u8 = 0x21;

/// One visibility table: per model group, per alternative, the display-object
/// ordinals that alternative uses (`FtPartsVisLookup` / `TempS`).
pub type VisibilityTable = Vec<Vec<Vec<u8>>>;

#[derive(Debug, thiserror::Error)]
pub enum ModelPartsError {
    #[error("fighter kind {0} is outside the source FighterKind table")]
    FighterKind(u8),
    #[error("missing or ambiguous ftData public root")]
    Root,
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("required {0} pointer is null")]
    NullPointer(&'static str),
    #[error("model-part table exceeds the {0} budget")]
    ResourceLimit(&'static str),
    #[error(
        "visibility table names display object {ordinal} but the costume has {count} display objects"
    )]
    OrdinalOutOfRange { ordinal: u8, count: usize },
}

/// The visibility tables `ftParts_8007487C` installs for one costume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FighterModelParts {
    pub model_count: usize,
    /// Tables 0–4 (`FtPartsVis.xC`): 0 is the normal main pass, 1 an
    /// alternate main pass, 2 the metal list (`x203C`), 3 suppressed in the
    /// main pass, and 4 set only by Game & Watch's `OnLoad` (`items[10]`).
    pub tables: [Option<VisibilityTable>; TABLE_COUNT],
}

impl FighterModelParts {
    /// Read the tables for `costume` from the fighter's `ftData` root.
    ///
    /// `costume` must already be clamped as `fighter.c:722` does: an ID at or
    /// beyond the fighter's costume count plays as costume 0. A null
    /// per-costume table falls back to costume 0's (`ftParts_8007487C`).
    pub fn load(
        fighter: &DatFile,
        fighter_kind: u8,
        costume: usize,
    ) -> Result<Self, ModelPartsError> {
        if fighter_kind >= FIGHTER_KIND_COUNT {
            return Err(ModelPartsError::FighterKind(fighter_kind));
        }
        let root = unique_ftdata_root(fighter)?;
        let ft_data = DescriptorReader::new(fighter, "ftData", root);
        let parts_offset = required(ft_data, "ftData.x8", 0x08)?;
        let parts =
            DescriptorReader::new(fighter, "FtPartsDesc", parts_offset).require_extent(8)?;
        let model_count = parts.u32(0)? as usize;
        if model_count > MAX_MODEL_GROUPS {
            return Err(ModelPartsError::ResourceLimit("model group"));
        }
        let vis_table = required(parts, "FtPartsDesc.vis_table", 4)?;
        let costume_offset = u32::try_from(costume)
            .ok()
            .and_then(|costume| costume.checked_mul(16))
            .ok_or(ModelPartsError::ResourceLimit("costume"))?;
        let row = DescriptorReader::new(fighter, "vis_table", vis_table);
        let mut tables: [Option<VisibilityTable>; TABLE_COUNT] = Default::default();
        for (index, table) in tables.iter_mut().take(4).enumerate() {
            let slot = index as u32 * 4;
            let lookup = match row.pointer("vis_table entry", costume_offset + slot)? {
                Some(lookup) => Some(lookup),
                None => row.pointer("vis_table entry", slot)?,
            };
            *table = lookup
                .map(|lookup| read_table(fighter, lookup, model_count))
                .transpose()?;
        }
        if fighter_kind == FIGHTER_KIND_GAME_AND_WATCH {
            // ftGw_Init_OnLoad: fp->x5AC.xC[4] = ft_data->x48_items[10].
            let items = required(ft_data, "ftData.x48_items", 0x48)?;
            let items = DescriptorReader::new(fighter, "x48_items", items);
            if let Some(lookup) = items.pointer("x48_items[10]", 40)? {
                tables[4] = Some(read_table(fighter, lookup, model_count)?);
            }
        }
        Ok(Self {
            model_count,
            tables,
        })
    }

    /// Visibility per display-object ordinal for the normal, non-metal main
    /// pass, given each group's committed selection (`-1` selects none).
    ///
    /// `ftParts_8007487C` hides every object tables 0, 1 and 3 name (table 2
    /// addresses the separate metal list). Table 4 is Mr. Game & Watch's
    /// outline, which the game draws in passes of its own
    /// (`ftDrawCommon_80080E18`); it stays hidden here. Then `ftDrawCommon_800805C8`
    /// applies table 0 through `ftParts_80074B6C`: per group, the selected
    /// alternative is shown and every other alternative hidden, in order.
    pub fn main_pass_visibility(
        &self,
        selections: &[i32],
        display_object_count: usize,
    ) -> Result<Vec<bool>, ModelPartsError> {
        let mut visible = vec![true; display_object_count];
        let mut set = |ordinal: u8, value: bool| {
            let slot = visible.get_mut(usize::from(ordinal)).ok_or(
                ModelPartsError::OrdinalOutOfRange {
                    ordinal,
                    count: display_object_count,
                },
            )?;
            *slot = value;
            Ok::<_, ModelPartsError>(())
        };
        for table in [0, 1, 3, 4]
            .into_iter()
            .filter_map(|index| self.tables[index].as_ref())
        {
            for ordinal in table.iter().flatten().flatten() {
                set(*ordinal, false)?;
            }
        }
        if let Some(table) = &self.tables[0] {
            for (group, alternatives) in table.iter().enumerate() {
                let selected = selections.get(group).copied().unwrap_or(-1);
                for (alternative, ordinals) in alternatives.iter().enumerate() {
                    let show = i32::try_from(alternative).is_ok_and(|index| index == selected);
                    for ordinal in ordinals {
                        set(*ordinal, show)?;
                    }
                }
            }
        }
        Ok(visible)
    }
}

/// Group selections a fighter's first action starts from: `ftParts_800749CC`
/// sets every group to -1, `ftXx_Init_OnDeath` requests its defaults through
/// `ftParts_80074A4C`, and `ftParts_80074A8C` commits them when the first
/// action state begins. `costume` is the clamped costume ID.
///
/// Kirby's copy-ability hat (a player setting, `ftKb_SpecialN_800F1BAC`) is
/// not modeled: a preview has no copy ability.
pub fn default_selections(fighter_kind: u8, costume: usize, model_count: usize) -> Vec<i32> {
    let mut selections = vec![-1; model_count];
    let mut request = |group: usize, value: i32| {
        if let Some(slot) = selections.get_mut(group) {
            *slot = value;
        }
    };
    match fighter_kind {
        // ftPe_Init_OnDeath: costume 1 swaps groups 1, 5, and 6.
        FIGHTER_KIND_PEACH => {
            for (group, value) in [(0, 0), (2, 0), (3, -1), (4, 0)] {
                request(group, value);
            }
            let costume_groups = if costume == 1 {
                [(1, -1), (5, 0), (6, -1)]
            } else {
                [(1, 0), (5, -1), (6, 0)]
            };
            for (group, value) in costume_groups {
                request(group, value);
            }
        }
        // ftPc_Init_OnDeath: costumes 1–3 each show one of groups 1–3; the
        // switch has no default, so any other costume leaves them at -1.
        FIGHTER_KIND_PICHU => {
            request(0, 0);
            for group in 1..=3 {
                request(group, if costume == group { 0 } else { -1 });
            }
        }
        kind => {
            for &(group, value) in on_death_requests(kind) {
                request(group, value);
            }
        }
    }
    selections
}

/// The Wait1 script's `set_dobj_flags` commands (`ftAction_80071D40` ->
/// `ftParts_80074B0C`), applied on top of [`default_selections`] when a
/// fighter enters Wait1.
pub fn wait1_script_selections(fighter_kind: u8) -> &'static [(usize, i32)] {
    match fighter_kind {
        // Game & Watch's Wait1 script selects model 2, alternative 1.
        FIGHTER_KIND_GAME_AND_WATCH => &[(2, 1)],
        _ => &[],
    }
}

/// `ftParts_80074A4C` calls in each costume-independent `OnDeath`, indexed by
/// internal FighterKind (`ftData_OnDeath`, `ftdata.c:313`).
fn on_death_requests(fighter_kind: u8) -> &'static [(usize, i32)] {
    const ONE: &[(usize, i32)] = &[(0, 0)];
    const TWO: &[(usize, i32)] = &[(0, 0), (1, 0)];
    const THREE: &[(usize, i32)] = &[(0, 0), (1, 0), (2, 0)];
    match fighter_kind {
        // Mario, Fox, Captain Falcon, Donkey Kong, Bowser, Ness, Samus, Yoshi,
        // Jigglypuff, Mewtwo, Luigi (`false`), Dr. Mario, Falco.
        0x00 | 0x01 | 0x02 | 0x03 | 0x05 | 0x08 | 0x0D | 0x0E | 0x0F | 0x10 | 0x11 | 0x15
        | 0x16 => ONE,
        // Kirby, Popo, Nana, Pikachu, Marth, Zelda.
        0x04 | 0x0A | 0x0B | 0x0C | 0x12 | 0x13 => TWO,
        // Link, Young Link.
        0x06 | 0x14 => THREE,
        // Sheik, Ganondorf.
        0x07 | 0x19 => &[(0, 0), (1, -1)],
        // Game & Watch.
        0x18 => &[
            (0, 0),
            (1, -1),
            (2, 0),
            (3, 0),
            (4, -1),
            (5, -1),
            (6, -1),
            (7, -1),
            (8, -1),
            (9, -1),
            (10, -1),
        ],
        // Roy.
        0x1A => &[(0, 0), (1, 0), (2, -1)],
        _ => &[],
    }
}

fn read_table(
    fighter: &DatFile,
    lookup: u32,
    model_count: usize,
) -> Result<VisibilityTable, ModelPartsError> {
    let groups = DescriptorReader::new(fighter, "FtPartsVisLookup", lookup)
        .require_extent(model_count * 8)?;
    (0..model_count)
        .map(|group| {
            let base = group as u32 * 8;
            let count = groups.u32(base)? as usize;
            if count > MAX_ALTERNATIVES {
                return Err(ModelPartsError::ResourceLimit("alternative"));
            }
            let Some(entries) = groups.pointer("FtPartsVisLookup.x4", base + 4)? else {
                return if count == 0 {
                    Ok(Vec::new())
                } else {
                    Err(ModelPartsError::NullPointer("FtPartsVisLookup.x4"))
                };
            };
            let entries =
                DescriptorReader::new(fighter, "TempS", entries).require_extent(count * 8)?;
            (0..count)
                .map(|alternative| {
                    let base = alternative as u32 * 8;
                    let ordinals = entries.u32(base)? as usize;
                    if ordinals > MAX_ORDINALS {
                        return Err(ModelPartsError::ResourceLimit("ordinal"));
                    }
                    match entries.pointer("TempS.x4", base + 4)? {
                        Some(list) => Ok(DescriptorReader::new(fighter, "TempS ordinals", list)
                            .bytes(0, ordinals)?
                            .to_vec()),
                        None if ordinals == 0 => Ok(Vec::new()),
                        None => Err(ModelPartsError::NullPointer("TempS.x4")),
                    }
                })
                .collect()
        })
        .collect()
}

fn unique_ftdata_root(fighter: &DatFile) -> Result<u32, ModelPartsError> {
    let mut roots = fighter
        .roots
        .iter()
        .filter(|root| root.name.starts_with("ftData"));
    let root = roots.next().ok_or(ModelPartsError::Root)?;
    if roots.next().is_some() {
        return Err(ModelPartsError::Root);
    }
    Ok(root.data_offset)
}

fn required(
    reader: DescriptorReader<'_>,
    field: &'static str,
    relative: u32,
) -> Result<u32, ModelPartsError> {
    reader
        .pointer(field, relative)?
        .ok_or(ModelPartsError::NullPointer(field))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(table0: VisibilityTable, table1: Option<VisibilityTable>) -> FighterModelParts {
        FighterModelParts {
            model_count: table0.len(),
            tables: [Some(table0), table1, None, None, None],
        }
    }

    #[test]
    fn main_pass_shows_only_the_selected_alternative_of_table_zero() {
        // Group 0: alternatives {1}, {2, 3}; group 1: alternative {4}.
        let parts = parts(vec![vec![vec![1], vec![2, 3]], vec![vec![4]]], None);
        let visible = parts.main_pass_visibility(&[1, -1], 6).unwrap();
        assert_eq!(visible, vec![true, false, true, true, false, true]);
    }

    #[test]
    fn table_one_objects_stay_hidden_in_the_main_pass() {
        let parts = parts(vec![vec![vec![1]]], Some(vec![vec![vec![0], vec![2]]]));
        let visible = parts.main_pass_visibility(&[0], 3).unwrap();
        assert_eq!(visible, vec![false, true, false]);
    }

    #[test]
    fn a_later_alternative_wins_for_a_shared_ordinal() {
        // ftParts_80074B6C writes alternatives in order, so the last write wins.
        let parts = parts(vec![vec![vec![0], vec![0]]], None);
        assert_eq!(parts.main_pass_visibility(&[0], 1).unwrap(), vec![false]);
        assert_eq!(parts.main_pass_visibility(&[1], 1).unwrap(), vec![true]);
    }

    #[test]
    fn ordinals_beyond_the_costume_are_rejected() {
        let parts = parts(vec![vec![vec![5]]], None);
        assert!(matches!(
            parts.main_pass_visibility(&[0], 3),
            Err(ModelPartsError::OrdinalOutOfRange {
                ordinal: 5,
                count: 3
            })
        ));
    }

    #[test]
    fn peach_and_pichu_defaults_follow_their_costume_switches() {
        assert_eq!(
            default_selections(FIGHTER_KIND_PEACH, 1, 7),
            vec![0, -1, 0, -1, 0, 0, -1]
        );
        assert_eq!(
            default_selections(FIGHTER_KIND_PEACH, 3, 7),
            vec![0, 0, 0, -1, 0, -1, 0]
        );
        assert_eq!(
            default_selections(FIGHTER_KIND_PICHU, 2, 4),
            vec![0, -1, 0, -1]
        );
        assert_eq!(
            default_selections(FIGHTER_KIND_PICHU, 0, 4),
            vec![0, -1, -1, -1]
        );
    }
}
