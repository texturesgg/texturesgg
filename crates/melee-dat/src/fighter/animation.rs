//! Melee's full compact-animation attachment boundary: same-kind records and
//! Nana's Popo fallback, both over the playing fighter's own parts.
//!
//! Source: melee revision 90f83f6665648a73122146d981eec48f173b1ca3,
//! `src/melee/ft/{ftdata.c,ftparts.c,ftanim.c,fighter.c,types.h}` and
//! `src/melee/lb/lbanim.c`. Game part indices are not HAL joint identities.

use std::collections::HashSet;
use std::ops::Range;

use dat_parser::descriptor::animation::{RawFigaTree, RawFigaTreeError};
use dat_parser::descriptor::jobj::flags;
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::animation::{HsdJointPoseError, HsdJointPoseEvaluator, HsdJointPoseLimits};
use dat_parser::hsd::scene::{HsdJointIndex, HsdSceneRoot, JObjId};
use dat_parser::{DatFile, DatParseError};

const FIGHTER_KIND_COUNT: u8 = 0x21;
const FIGHTER_KIND_POPO: u8 = 0x0a;
const FIGHTER_KIND_NANA: u8 = 0x0b;
const MAX_PARTS: usize = 0x8c;
const RECORD_SIZE: usize = 0x18;
const AUXILIARY_MASK: u32 = 0x003f_fe00;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FighterAnimationBinding {
    /// Exact byte extent in the original, unrelocated Pl*AJ.dat concatenation.
    pub archive_range: Range<usize>,
    pub animation_symbol: String,
    /// Fighter_WaitAnimData.x10, not FigaTree.flags.
    pub packed_flags: u32,
    /// Costume-DAT data-section identities, one per count-list entry (even zero).
    pub receivers: Vec<JObjId>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FighterAnimationBindingError {
    #[error("fighter kind {0} is outside the source FighterKind table")]
    FighterKind(u8),
    #[error("animation index {index} is outside the authenticated count {count}")]
    AnimationIndex { index: usize, count: usize },
    #[error("missing or ambiguous {0} public root")]
    Root(&'static str),
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("required {0} pointer is null")]
    NullPointer(&'static str),
    #[error("invalid animation metadata: {0}")]
    Metadata(&'static str),
    #[error("animation source kind {source_kind} differs from fighter kind {fighter_kind}")]
    RemappedAnimation { fighter_kind: u8, source_kind: u8 },
    #[error("Nana's own animation record is populated; the Popo fallback does not apply")]
    NanaRecordPresent,
    #[error("animation enables auxiliary parts ({0:#010x})")]
    AuxiliaryEnabled(u32),
    #[error("animation requests an unsupported partial/blend part ({0:#010x})")]
    PartialAnimation(u32),
    #[error("selected AJ archive range is invalid")]
    ArchiveRange,
    #[error("selected AJ mini-DAT is invalid: {0}")]
    Archive(#[from] DatParseError),
    #[error(transparent)]
    FigaTree(#[from] RawFigaTreeError),
    #[error("invalid fighter parts/auxiliary table: {0}")]
    Parts(&'static str),
    #[error("unsupported costume hierarchy: {0}")]
    Hierarchy(&'static str),
    #[error(
        "full animation has {animation} count entries but costume has {physical} physical parts"
    )]
    ReceiverCount { animation: usize, physical: usize },
}

/// Bind an original fighter metadata record to an unmodified physical costume tree.
///
/// `fighter_kind` is the *internal* source FighterKind, not Slippi's external
/// character ID. `animation_count` must come from authenticated executable
/// `ftData_Table_Unk0[kind].count` (ftdata.c:174,1603), never adjacent DAT bytes.
/// `root` must be the scene root parsed from the selected fighter's costume DAT;
/// returned IDs belong to that archive, not the fighter metadata or mini-DAT.
///
/// This models initial `ftParts_SetupParts` followed by `ftAnim_8006FE08` ->
/// `ftAnim_8006F4C8` -> `lbAnim_8001E6D8`, with no blending, per-part animation
/// overrides, disabled bones, dynamic auxiliary insertion, or subtree attachment.
/// Runtime state is intentionally not an input: callers must not use this API to
/// approximate any of those paths. Zero-count entries still consume receivers.
/// Static auxiliary slots are validated and skipped exactly as SetupParts does.
/// INSTANCE edges are rejected because setup and descriptor traversal differ.
pub fn bind_same_kind_fighter_animation(
    fighter: &DatFile,
    common: &DatFile,
    aj: &[u8],
    root: &HsdSceneRoot,
    fighter_kind: u8,
    animation_index: usize,
    animation_count: usize,
) -> Result<FighterAnimationBinding, FighterAnimationBindingError> {
    check_selection(fighter_kind, animation_index, animation_count)?;
    let (record, record_offset) = animation_record(fighter, animation_index, animation_count)?;
    let packed_flags = motion_flags(fighter, record, record_offset, fighter_kind)?;
    bind_record_tree(
        fighter,
        record,
        record_offset,
        common,
        aj,
        root,
        fighter_kind,
        packed_flags,
    )
}

/// Bind a Nana motion through ftData_80085FD4's Popo fallback.
///
/// Outside demo player slots, a Nana motion whose own record never received an
/// AJ address (x14 stays zero because ftData_80085A14 skips a zero x8 size)
/// plays Popo's record for the same motion ID. ChangeMotionState still loads
/// x594 from Nana's own record (fighter.c:1256), so Nana's flags select the
/// same-kind ftAnim_8006F4C8 path over Nana's parts: Popo's tree is not remapped.
/// Both counts are the authenticated `ftData_Table_Unk0` counts for each kind.
// Mirrors the game's inputs: two fighter archives plus the shared common data.
#[allow(clippy::too_many_arguments)]
pub fn bind_nana_fighter_animation(
    nana: &DatFile,
    popo: &DatFile,
    common: &DatFile,
    popo_aj: &[u8],
    root: &HsdSceneRoot,
    animation_index: usize,
    nana_count: usize,
    popo_count: usize,
) -> Result<FighterAnimationBinding, FighterAnimationBindingError> {
    check_selection(FIGHTER_KIND_NANA, animation_index, nana_count)?;
    check_selection(FIGHTER_KIND_POPO, animation_index, popo_count)?;
    let (own, own_offset) = animation_record(nana, animation_index, nana_count)?;
    let packed_flags = motion_flags(nana, own, own_offset, FIGHTER_KIND_NANA)?;
    require_unresolved_address(nana, own, own_offset)?;
    if own.u32(8)? != 0 {
        return Err(FighterAnimationBindingError::NanaRecordPresent);
    }
    let (record, record_offset) = animation_record(popo, animation_index, popo_count)?;
    bind_record_tree(
        popo,
        record,
        record_offset,
        common,
        popo_aj,
        root,
        FIGHTER_KIND_NANA,
        packed_flags,
    )
}

fn check_selection(
    fighter_kind: u8,
    animation_index: usize,
    animation_count: usize,
) -> Result<(), FighterAnimationBindingError> {
    if fighter_kind >= FIGHTER_KIND_COUNT {
        return Err(FighterAnimationBindingError::FighterKind(fighter_kind));
    }
    if animation_index >= animation_count {
        return Err(FighterAnimationBindingError::AnimationIndex {
            index: animation_index,
            count: animation_count,
        });
    }
    Ok(())
}

/// The animation symbol a record names (`Ply<Fighter>5K_Share_ACTION_<Action>_figatree`),
/// or `None` for a record without one.
fn record_symbol(
    fighter: &DatFile,
    record: DescriptorReader<'_>,
) -> Result<Option<String>, FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    let Some(symbol_offset) = record.pointer("symbol", 0)? else {
        return Ok(None);
    };
    let symbol_bytes = fighter
        .data
        .get(symbol_offset as usize..)
        .ok_or(Error::Metadata("symbol outside data"))?;
    let symbol_len = symbol_bytes
        .iter()
        .take(1024)
        .position(|byte| *byte == 0)
        .filter(|len| *len != 0)
        .ok_or(Error::Metadata("empty or unterminated animation symbol"))?;
    let symbol = std::str::from_utf8(&symbol_bytes[..symbol_len])
        .map_err(|_| Error::Metadata("animation symbol is not UTF-8"))?;
    Ok(Some(symbol.to_owned()))
}

/// The action each of a fighter's `animation_count` animation records plays
/// (`Wait1`, `AttackHi3`), read from its symbol; `None` for a record with no
/// animation. `animation_count` is the catalog's authenticated count, as for
/// [`bind_same_kind_fighter_animation`].
pub fn fighter_animation_actions(
    fighter: &DatFile,
    animation_count: usize,
) -> Result<Vec<Option<String>>, FighterAnimationBindingError> {
    (0..animation_count)
        .map(|index| {
            let (record, _) = animation_record(fighter, index, animation_count)?;
            Ok(record_symbol(fighter, record)?.map(|symbol| action_of(&symbol).to_owned()))
        })
        .collect()
}

/// The action part of an animation symbol, or the whole symbol when it
/// doesn't follow the `_ACTION_..._figatree` convention.
fn action_of(symbol: &str) -> &str {
    symbol
        .split_once("_ACTION_")
        .map_or(symbol, |(_, rest)| rest)
        .trim_end_matches("_figatree")
}

/// ftData.xC and Fighter_WaitAnimData (types.h:613,889).
fn animation_record(
    fighter: &DatFile,
    animation_index: usize,
    animation_count: usize,
) -> Result<(DescriptorReader<'_>, u32), FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    let fighter_root = unique_root(fighter, |name| name.starts_with("ftData"), "ftData*")?;
    let fighter_desc = DescriptorReader::new(fighter, "ftData", fighter_root);
    let table = required(fighter_desc, "animations", 0x0c)?;
    let table_size = animation_count
        .checked_mul(RECORD_SIZE)
        .ok_or(Error::Metadata("animation table extent overflow"))?;
    DescriptorReader::new(fighter, "animation table", table).require_extent(table_size)?;
    let record_offset = usize::try_from(table)
        .ok()
        .and_then(|base| {
            animation_index
                .checked_mul(RECORD_SIZE)
                .and_then(|index| base.checked_add(index))
        })
        .and_then(|offset| u32::try_from(offset).ok())
        .ok_or(Error::Metadata("selected record offset overflow"))?;
    let record = DescriptorReader::new(fighter, "Fighter_WaitAnimData", record_offset)
        .require_extent(RECORD_SIZE)?;
    Ok((record, record_offset))
}

/// x10 as ChangeMotionState copies it to x594: the attach path and its parts.
fn motion_flags(
    fighter: &DatFile,
    record: DescriptorReader<'_>,
    record_offset: u32,
    fighter_kind: u8,
) -> Result<u32, FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    if fighter
        .relocation_sites
        .binary_search(&(record_offset + 0x10))
        .is_ok()
    {
        return Err(Error::Metadata("scalar record field is relocated"));
    }
    let packed_flags = record.u32(0x10)?;
    let source_kind = (packed_flags & 0x3f) as u8;
    if source_kind != fighter_kind {
        return Err(Error::RemappedAnimation {
            fighter_kind,
            source_kind,
        });
    }
    // PowerPC bitfield layout: types.h:1223-1226, 10 pad / 13 aux /
    // 3 partial-blend part / 6 kind; fighter.c:1239 uses the three-bit part.
    if packed_flags & AUXILIARY_MASK != 0 {
        return Err(Error::AuxiliaryEnabled(packed_flags));
    }
    if packed_flags & 0x1c0 != 0 {
        return Err(Error::PartialAnimation(packed_flags));
    }
    Ok(packed_flags)
}

fn require_unresolved_address(
    fighter: &DatFile,
    record: DescriptorReader<'_>,
    record_offset: u32,
) -> Result<(), FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    // x14 is a runtime ARAM address, populated by ftData_80085A14. Do not
    // silently interpret relocated/runtime metadata as the original AJ layout.
    if record.u32(0x14)? != 0
        || fighter
            .relocation_sites
            .binary_search(&(record_offset + 0x14))
            .is_ok()
    {
        return Err(Error::Metadata("runtime-patched animation address"));
    }
    Ok(())
}

/// Resolve a record's FigaTree and bind it over `fighter_kind`'s physical parts.
// Mirrors the game's inputs: record location, shared data, and costume root.
#[allow(clippy::too_many_arguments)]
fn bind_record_tree(
    fighter: &DatFile,
    record: DescriptorReader<'_>,
    record_offset: u32,
    common: &DatFile,
    aj: &[u8],
    root: &HsdSceneRoot,
    fighter_kind: u8,
    packed_flags: u32,
) -> Result<FighterAnimationBinding, FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    let animation_symbol = record_symbol(fighter, record)?
        .ok_or(FighterAnimationBindingError::NullPointer("symbol"))?;
    require_unresolved_address(fighter, record, record_offset)?;
    for relative in [4, 8] {
        if fighter
            .relocation_sites
            .binary_search(&(record_offset + relative))
            .is_ok()
        {
            return Err(Error::Metadata("scalar record field is relocated"));
        }
    }
    let start = record.u32(4)? as usize;
    let size = record.u32(8)? as usize;
    // ftData_80085A14 asserts the per-fighter 0x8000-byte animation buffer.
    if size == 0 || size > 0x8000 {
        return Err(Error::ArchiveRange);
    }
    let end = start.checked_add(size).ok_or(Error::ArchiveRange)?;
    let archive_range = start..end;
    let animation = DatFile::parse(aj.get(archive_range.clone()).ok_or(Error::ArchiveRange)?)?;
    if !animation.externs.is_empty() {
        return Err(Error::Metadata("mini-DAT requires external linking"));
    }
    let tree_root = unique_root(
        &animation,
        |name| name == animation_symbol,
        "selected animation symbol",
    )?;
    let tree = RawFigaTree::parse(&animation, tree_root)?;

    let parts = fighter_part_slots(common, fighter_kind, root)?;
    // ftAnim_8006F4C8 skips !flags_b1; every uninserted auxiliary slot is
    // absent. It consumes one node count even when that count is zero.
    let receivers: Vec<_> = parts.slots.iter().flatten().copied().collect();
    if tree.track_counts.len() != receivers.len() {
        return Err(Error::ReceiverCount {
            animation: tree.track_counts.len(),
            physical: receivers.len(),
        });
    }
    Ok(FighterAnimationBinding {
        archive_range,
        animation_symbol,
        packed_flags,
        receivers,
    })
}

/// A costume's physical joints placed in its fighter's part slots.
pub(crate) struct FighterPartSlots {
    /// Per slot, the physical joint in it; `None` for an auxiliary slot.
    pub slots: Vec<Option<JObjId>>,
    /// Per slot, the logical part (`Fighter_Part`), or `0xff` for none.
    pub logical: Vec<u8>,
}

/// Place `root`'s physical joints in `fighter_kind`'s part slots, as
/// `ftParts_SetupParts` does, validating the shared parts and auxiliary
/// tables in `PlCo.dat`.
pub(crate) fn fighter_part_slots(
    common: &DatFile,
    fighter_kind: u8,
    root: &HsdSceneRoot,
) -> Result<FighterPartSlots, FighterAnimationBindingError> {
    use FighterAnimationBindingError as Error;
    // Fighter_LoadCommonData (fighter.c:186-200): pData[4] = ftPartsTable,
    // pData[5] = Fighter_804D6540. The latter contains four-byte aux entries
    // {slot, relative_slot, insertion_type, descriptor_ordinal} (fighter.h:176).
    let common_root = unique_root(
        common,
        |name| name == "ftLoadCommonData",
        "ftLoadCommonData",
    )?;
    let common_desc = DescriptorReader::new(common, "ftLoadCommonData", common_root);
    let parts_table = required(common_desc, "parts table", 0x10)?;
    let parts_desc_offset = required(
        DescriptorReader::new(common, "parts table", parts_table),
        "fighter parts",
        u32::from(fighter_kind) * 4,
    )?;
    let parts =
        DescriptorReader::new(common, "FighterPartsTable", parts_desc_offset).require_extent(12)?;
    let parts_num = parts.u32(8)? as usize;
    if parts_num == 0 || parts_num > MAX_PARTS {
        return Err(Error::Parts("part count outside 1..=140"));
    }
    let joint_to_part = required(parts, "joint_to_part", 0)?;
    let part_to_joint = required(parts, "part_to_joint", 4)?;
    let forward =
        DescriptorReader::new(common, "joint_to_part", joint_to_part).bytes(0, parts_num)?;
    let reverse = DescriptorReader::new(common, "part_to_joint", part_to_joint);
    // The remap arrays do not determine attachment order on this same-kind
    // path. Validate their correspondence without using them to invent IDs.
    for (slot, logical_part) in forward.iter().copied().enumerate() {
        if logical_part != 0xff && reverse.u8(u32::from(logical_part))? as usize != slot {
            return Err(Error::Parts("non-reciprocal logical part map"));
        }
    }
    let aux_table = required(common_desc, "auxiliary table", 0x14)?;
    let auxiliary = DescriptorReader::new(common, "auxiliary table", aux_table)
        .pointer("fighter auxiliary parts", u32::from(fighter_kind) * 4)?;
    let mut auxiliary_slots = [false; MAX_PARTS];
    if let Some(offset) = auxiliary {
        let auxiliary =
            DescriptorReader::new(common, "fighter auxiliary parts", offset).require_extent(8)?;
        let count = auxiliary.u32(4)? as usize;
        if count > 13 {
            return Err(Error::Parts("auxiliary count exceeds packed 13-bit mask"));
        }
        if count != 0 {
            let entries = required(auxiliary, "auxiliary entries", 0)?;
            let entries =
                DescriptorReader::new(common, "auxiliary entries", entries).bytes(0, count * 4)?;
            for entry in entries.as_chunks::<4>().0 {
                let slot = entry[0] as usize;
                if slot >= parts_num || entry[1] as usize >= parts_num || entry[2] > 3 {
                    return Err(Error::Parts(
                        "invalid auxiliary slot or insertion descriptor",
                    ));
                }
                if std::mem::replace(&mut auxiliary_slots[slot], true) {
                    return Err(Error::Parts("duplicate auxiliary slot"));
                }
            }
        }
    }

    let physical = physical_joint_order(root)?;
    // ftParts_SetupParts (ftparts.c:392-455) skips static auxiliary slots
    // before each physical joint, then asserts the consumed part count. In
    // particular, trailing auxiliary slots are NOT consumed after tree end.
    let mut slots = [None; MAX_PARTS];
    let mut part = 0;
    for receiver in physical {
        while part < parts_num && auxiliary_slots[part] {
            part += 1;
        }
        if part == parts_num {
            return Err(Error::Parts(
                "costume has more physical joints than the part table",
            ));
        }
        slots[part] = Some(receiver);
        part += 1;
    }
    if part != parts_num {
        return Err(Error::Parts(
            "costume traversal does not exhaust the part table",
        ));
    }
    Ok(FighterPartSlots {
        slots: slots[..parts_num].to_vec(),
        logical: forward.to_vec(),
    })
}

fn required(
    reader: DescriptorReader<'_>,
    field: &'static str,
    relative: u32,
) -> Result<u32, FighterAnimationBindingError> {
    reader
        .pointer(field, relative)?
        .ok_or(FighterAnimationBindingError::NullPointer(field))
}

fn unique_root(
    dat: &DatFile,
    predicate: impl Fn(&str) -> bool,
    label: &'static str,
) -> Result<u32, FighterAnimationBindingError> {
    let mut matching = dat.roots.iter().filter(|root| predicate(&root.name));
    let root = matching
        .next()
        .ok_or(FighterAnimationBindingError::Root(label))?;
    if matching.next().is_some() {
        return Err(FighterAnimationBindingError::Root(label));
    }
    Ok(root.data_offset)
}

fn physical_joint_order(root: &HsdSceneRoot) -> Result<Vec<JObjId>, FighterAnimationBindingError> {
    use FighterAnimationBindingError::Hierarchy;
    if root.joints.is_empty() || root.joints.len() > MAX_PARTS {
        return Err(Hierarchy("physical joint count outside 1..=140"));
    }
    let mut identities = HashSet::with_capacity(root.joints.len());
    for joint in &root.joints {
        if !identities.insert(joint.source_id) {
            return Err(Hierarchy("duplicate archive-local joint identity"));
        }
        if joint.flags & flags::INSTANCE != 0 {
            return Err(Hierarchy(
                "INSTANCE traversal requires a different attachment policy",
            ));
        }
    }
    let start = root
        .joints
        .iter()
        .position(|joint| joint.source_id == root.source_id)
        .ok_or(Hierarchy("root identity is absent from the hierarchy"))?;
    let mut stack = vec![(HsdJointIndex(start), None)];
    let mut visited = [false; MAX_PARTS];
    let mut order = Vec::with_capacity(root.joints.len());
    while let Some((index, parent)) = stack.pop() {
        let joint = root
            .joints
            .get(index.0)
            .ok_or(Hierarchy("child index outside hierarchy"))?;
        if std::mem::replace(&mut visited[index.0], true) || joint.parent != parent {
            return Err(Hierarchy(
                "cycle, shared child, or inconsistent parent edge",
            ));
        }
        order.push(joint.source_id);
        if joint.children.len() + stack.len() > root.joints.len() {
            return Err(Hierarchy("too many child edges"));
        }
        stack.extend(
            joint
                .children
                .iter()
                .rev()
                .map(|child| (*child, Some(index))),
        );
    }
    if order.len() != root.joints.len() {
        return Err(Hierarchy("disconnected physical joints"));
    }
    Ok(order)
}

/// A compact animation attached to one model root, ready to tick and pose.
pub struct AttachedFighterAnimation {
    pub root_index: usize,
    /// Data offset of the FigaTree root in its (mini-)archive.
    pub animation_root_source_id: u32,
    /// FigaTree.flags, not Fighter_WaitAnimData.x10.
    pub flags: u32,
    pub end_frame: f32,
    pub receivers: Vec<JObjId>,
    pub pose: HsdJointPoseEvaluator<'static>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FighterAnimationAttachError {
    #[error("selected AJ archive range is invalid")]
    ArchiveRange,
    #[error("selected AJ mini-DAT is invalid: {0}")]
    Archive(#[from] DatParseError),
    #[error("selected animation symbol is absent")]
    MissingSymbol,
    #[error("selected animation symbol is ambiguous")]
    AmbiguousSymbol,
    #[error(transparent)]
    FigaTree(#[from] RawFigaTreeError),
    #[error(transparent)]
    Pose(#[from] HsdJointPoseError),
}

impl AttachedFighterAnimation {
    /// Attach a bound AJ record: parse its mini-DAT and select its one symbol.
    pub fn from_binding(
        scene: &HsdScene,
        root_index: usize,
        aj: &[u8],
        binding: &FighterAnimationBinding,
    ) -> Result<Self, FighterAnimationAttachError> {
        let archive = aj
            .get(binding.archive_range.clone())
            .ok_or(FighterAnimationAttachError::ArchiveRange)?;
        let dat = DatFile::parse(archive)?;
        let mut roots = dat
            .roots
            .iter()
            .filter(|root| root.name == binding.animation_symbol);
        let root = roots
            .next()
            .ok_or(FighterAnimationAttachError::MissingSymbol)?;
        if roots.next().is_some() {
            return Err(FighterAnimationAttachError::AmbiguousSymbol);
        }
        Self::from_tree(
            scene,
            root_index,
            &dat,
            root.data_offset,
            &binding.receivers,
        )
    }

    /// Attach the FigaTree at `source_id` to caller-established receivers.
    pub fn from_tree(
        scene: &HsdScene,
        root_index: usize,
        dat: &DatFile,
        source_id: u32,
        receivers: &[JObjId],
    ) -> Result<Self, FighterAnimationAttachError> {
        let tree = RawFigaTree::parse(dat, source_id)?;
        let pose = HsdJointPoseEvaluator::from_figatree(
            scene,
            root_index,
            &tree,
            receivers,
            HsdJointPoseLimits::default(),
        )?;
        Ok(Self {
            root_index,
            animation_root_source_id: source_id,
            flags: tree.flags,
            end_frame: tree.end_frame,
            receivers: receivers.to_vec(),
            pose: pose.into_owned(),
        })
    }
}
