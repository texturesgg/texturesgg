use dat_parser::DatFile;
use dat_parser::descriptor::DescriptorParseError;
use dat_parser::descriptor::jobj::flags;
use dat_parser::hsd::scene::{HsdJoint, HsdJointIndex, HsdSceneRoot, HsdTransform, JObjId};
use melee_dat::FighterKind;
use melee_dat::fighter::animation::{
    FighterAnimationBinding, FighterAnimationBindingError as Error, FighterAnimationFiles,
    bind_nana_fighter_animation, bind_same_kind_fighter_animation,
};

const SYMBOL: &str = "synthetic_ACTION_Wait_figatree";
const KIND: u8 = 1;

fn kind(value: u8) -> FighterKind {
    FighterKind::new(value).expect("a source fighter kind")
}
const RECORD: usize = 0x38;

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn archive(data: &[u8], relocations: &[u32], root: u32, symbol: &str) -> Vec<u8> {
    let mut raw = vec![0; 32 + data.len() + relocations.len() * 4 + 8 + symbol.len() + 1];
    let size = raw.len() as u32;
    put(&mut raw, 0, size);
    put(&mut raw, 4, data.len() as u32);
    put(&mut raw, 8, relocations.len() as u32);
    put(&mut raw, 12, 1);
    raw[32..32 + data.len()].copy_from_slice(data);
    let relocation_start = 32 + data.len();
    for (index, offset) in relocations.iter().enumerate() {
        put(&mut raw, relocation_start + index * 4, *offset);
    }
    let root_start = relocation_start + relocations.len() * 4;
    put(&mut raw, root_start, root);
    raw[root_start + 8..root_start + 8 + symbol.len()].copy_from_slice(symbol.as_bytes());
    raw
}

fn animation(counts: &[u8], symbol: &str) -> Vec<u8> {
    let mut data = vec![0; 0x90];
    put(&mut data, 0x10, 1);
    put(&mut data, 0x18, 20.0_f32.to_bits());
    put(&mut data, 0x1c, 0x24);
    put(&mut data, 0x20, 0x30);
    data[0x24..0x24 + counts.len()].copy_from_slice(counts);
    data[0x24 + counts.len()] = 0xff;
    let mut relocations = vec![0x1c, 0x20];
    for index in 0..counts
        .iter()
        .map(|count| usize::from(*count))
        .sum::<usize>()
    {
        let track = 0x30 + index * 12;
        data[track..track + 2].copy_from_slice(&3u16.to_be_bytes());
        data[track + 4] = 6;
        data[track + 5] = 0x80;
        data[track + 6] = 0x80;
        put(&mut data, track + 8, 0x80);
        relocations.push((track + 8) as u32);
    }
    data[0x80..0x83].copy_from_slice(&[0x01, 3, 0]);
    archive(&data, &relocations, 0x10, symbol)
}

struct Fixture {
    fighter: DatFile,
    common: DatFile,
    aj: Vec<u8>,
    root: HsdSceneRoot,
}

impl Fixture {
    fn new(counts: &[u8]) -> Self {
        let mini = animation(counts, SYMBOL);
        let mut aj = vec![0xa5; 32];
        aj.extend_from_slice(&mini);
        // Bytes after the selected archive are not part of HSD_ArchiveParse.
        aj.extend_from_slice(&[0x5a; 32]);
        let mut fighter = vec![0; 0x100];
        put(&mut fighter, 0x0c, 0x20);
        put(&mut fighter, RECORD, 0x80);
        put(&mut fighter, RECORD + 4, 32);
        put(&mut fighter, RECORD + 8, mini.len() as u32);
        put(&mut fighter, RECORD + 0x10, 0x1000_0000 | u32::from(KIND));
        fighter[0x80..0x80 + SYMBOL.len()].copy_from_slice(SYMBOL.as_bytes());
        let fighter = DatFile::parse(&archive(
            &fighter,
            &[0x0c, RECORD as u32],
            0,
            "ftDataSynthetic",
        ))
        .unwrap();

        let mut common = vec![0; 0xa0];
        put(&mut common, 0x10, 0x20);
        put(&mut common, 0x14, 0x30);
        put(&mut common, 0x24, 0x40);
        put(&mut common, 0x34, 0x80);
        put(&mut common, 0x40, 0x60);
        put(&mut common, 0x44, 0x68);
        put(&mut common, 0x48, 4);
        common[0x60..0x64].copy_from_slice(&[0, 1, 2, 3]);
        common[0x68..0x6c].copy_from_slice(&[0, 1, 2, 3]);
        put(&mut common, 0x80, 0x90);
        put(&mut common, 0x84, 1);
        common[0x90..0x94].copy_from_slice(&[1, 0, 0, 0xff]);
        let common = DatFile::parse(&archive(
            &common,
            &[0x10, 0x14, 0x24, 0x34, 0x40, 0x44, 0x80],
            0,
            "ftLoadCommonData",
        ))
        .unwrap();
        // Neither flat-vector ordinals nor sorted DAT offsets give attachment
        // order. Only physical child-edge traversal yields root, child A, child B.
        let root = HsdSceneRoot {
            source_id: JObjId(0x220),
            name: Some("synthetic_costume_joint".into()),
            joints: vec![
                joint(0x120, Some(2), &[]),
                joint(0x340, Some(2), &[]),
                joint(0x220, None, &[0, 1]),
            ],
        };
        Self {
            fighter,
            common,
            aj,
            root,
        }
    }

    /// The fixture's files, claiming `animation_count` records.
    fn files(&self, animation_count: usize) -> FighterAnimationFiles<'_> {
        FighterAnimationFiles {
            fighter: &self.fighter,
            common: &self.common,
            aj: &self.aj,
            root: &self.root,
            animation_count,
        }
    }

    fn bind(&self) -> Result<FighterAnimationBinding, Error> {
        bind_same_kind_fighter_animation(&self.files(2), kind(KIND), 1)
    }
}

fn joint(offset: u32, parent: Option<usize>, children: &[usize]) -> HsdJoint {
    HsdJoint {
        source_id: JObjId(offset),
        parent: parent.map(HsdJointIndex),
        children: children.iter().copied().map(HsdJointIndex).collect(),
        flags: 0,
        local: HsdTransform {
            scale: [1.0; 3],
            rotation: [0.0; 3],
            translation: [0.0; 3],
        },
        inverse_bind_transform: None,
        display_objects: Vec::new(),
    }
}

#[test]
fn binds_physical_tree_through_static_auxiliary_slots_including_zero_counts() {
    let fixture = Fixture::new(&[1, 0, 1]);
    let binding = fixture.bind().unwrap();
    assert_eq!(
        binding.receivers,
        [JObjId(0x220), JObjId(0x120), JObjId(0x340)]
    );
    assert_eq!(binding.animation_symbol, SYMBOL);
    assert_eq!(binding.packed_flags, 0x1000_0001);
    assert_eq!(binding.archive_range, 32..fixture.aj.len() - 32);
}

#[test]
fn accepts_absent_auxiliary_descriptor_without_inventing_slots() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    put(&mut fixture.common.data, 0x34, 0);
    fixture.common.relocation_sites.retain(|site| *site != 0x34);
    put(&mut fixture.common.data, 0x48, 3);
    assert_eq!(fixture.bind().unwrap().receivers.len(), 3);
}

#[test]
fn explicit_animation_count_is_authoritative_and_checked_before_selection() {
    let fixture = Fixture::new(&[0, 0, 0]);
    assert!(matches!(
        bind_same_kind_fighter_animation(&fixture.files(2), kind(KIND), 2),
        Err(Error::AnimationIndex { index: 2, count: 2 })
    ));
    assert!(matches!(
        bind_same_kind_fighter_animation(&fixture.files(usize::MAX), kind(KIND), 0),
        Err(Error::Metadata(_))
    ));
    assert!(matches!(
        bind_same_kind_fighter_animation(&fixture.files(11), kind(KIND), 1),
        Err(Error::Descriptor(DescriptorParseError::Truncated { .. }))
    ));
}

#[test]
fn rejects_cross_kind_auxiliary_enabled_and_partial_attachment_metadata() {
    for (packed, expected) in [(2, "kind"), (0x201, "aux"), (0x41, "partial")] {
        let mut fixture = Fixture::new(&[0, 0, 0]);
        put(&mut fixture.fighter.data, RECORD + 0x10, packed);
        match (fixture.bind(), expected) {
            (Err(Error::RemappedAnimation { .. }), "kind")
            | (Err(Error::AuxiliaryEnabled(_)), "aux")
            | (Err(Error::PartialAnimation(_)), "partial") => {}
            (actual, _) => panic!("unexpected result: {actual:?}"),
        }
    }
}

#[test]
fn refuses_unrelocated_metadata_pointers_and_runtime_patches() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture
        .fighter
        .relocation_sites
        .retain(|site| *site != RECORD as u32);
    assert!(matches!(
        fixture.bind(),
        Err(Error::Descriptor(
            DescriptorParseError::InvalidPointer { .. }
        ))
    ));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    put(&mut fixture.fighter.data, RECORD + 0x14, 0x8000_1000);
    assert!(matches!(fixture.bind(), Err(Error::Metadata(_))));
}

#[test]
fn requires_exact_mini_dat_extent_and_selected_public_symbol() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    let size = fixture.fighter.read_u32((RECORD + 8) as u32).unwrap();
    put(&mut fixture.fighter.data, RECORD + 8, size + 1);
    assert!(matches!(fixture.bind(), Err(Error::Archive(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    put(&mut fixture.fighter.data, RECORD + 4, u32::MAX);
    assert!(matches!(fixture.bind(), Err(Error::ArchiveRange)));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.fighter.data[0x80] = b'X';
    assert!(matches!(
        fixture.bind(),
        Err(Error::Root("selected animation symbol"))
    ));
}

#[test]
fn rejects_ambiguous_fighter_roots() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.fighter.roots.push(fixture.fighter.roots[0].clone());
    assert!(matches!(fixture.bind(), Err(Error::Root("ftData*"))));
}

#[test]
fn rejects_short_and_long_count_lists_instead_of_partial_binding() {
    for counts in [&[0, 0][..], &[0, 0, 0, 0][..]] {
        let fixture = Fixture::new(counts);
        assert!(matches!(
            fixture.bind(),
            Err(Error::ReceiverCount { physical: 3, .. })
        ));
    }
}

#[test]
fn rejects_invalid_part_mapping_and_auxiliary_boundaries() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.common.data[0x68] = 2;
    assert!(matches!(
        fixture.bind(),
        Err(Error::Parts("non-reciprocal logical part map"))
    ));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.common.data[0x90] = 4;
    assert!(matches!(fixture.bind(), Err(Error::Parts(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    put(&mut fixture.common.data, 0x84, 2);
    fixture.common.data[0x94..0x98].copy_from_slice(&[1, 0, 0, 0xff]);
    assert!(matches!(
        fixture.bind(),
        Err(Error::Parts("duplicate auxiliary slot"))
    ));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.common.data[0x90] = 3;
    assert!(matches!(
        fixture.bind(),
        Err(Error::Parts(
            "costume traversal does not exhaust the part table"
        ))
    ));
}

#[test]
fn rejects_instance_alias_cycles_disconnected_and_inconsistent_parent_edges() {
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.root.joints[0].flags |= flags::INSTANCE;
    assert!(matches!(fixture.bind(), Err(Error::Hierarchy(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.root.joints[0].source_id = fixture.root.source_id;
    assert!(matches!(fixture.bind(), Err(Error::Hierarchy(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.root.joints[0].children.push(HsdJointIndex(2));
    assert!(matches!(fixture.bind(), Err(Error::Hierarchy(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.root.joints[2].children.pop();
    assert!(matches!(fixture.bind(), Err(Error::Hierarchy(_))));
    let mut fixture = Fixture::new(&[0, 0, 0]);
    fixture.root.joints[0].parent = None;
    assert!(matches!(fixture.bind(), Err(Error::Hierarchy(_))));
}

const POPO: u8 = 0x0a;
const NANA: u8 = 0x0b;

/// The fixture's fighter becomes Popo; Nana gets an unresolved record whose
/// flags still name Nana, as the original PlNn.dat motion table does.
fn nana_fixture(counts: &[u8]) -> (Fixture, DatFile) {
    let mut fixture = Fixture::new(counts);
    put(
        &mut fixture.fighter.data,
        RECORD + 0x10,
        0x1000_0000 | u32::from(POPO),
    );
    // Point Nana's parts and auxiliary entries at the fixture's kind-1 layout.
    for (table, target) in [(0x20, 0x40), (0x30, 0x80)] {
        let site = table + usize::from(NANA) * 4;
        put(&mut fixture.common.data, site, target);
        fixture.common.relocation_sites.push(site as u32);
    }
    fixture.common.relocation_sites.sort_unstable();
    let mut nana = Fixture::new(counts).fighter;
    for relative in [0, 4, 8] {
        put(&mut nana.data, RECORD + relative, 0);
    }
    nana.relocation_sites.retain(|site| *site != RECORD as u32);
    put(&mut nana.data, RECORD + 0x10, u32::from(NANA));
    (fixture, nana)
}

/// Nana's files over the fixture's, which are Popo's: his archive, and the
/// shared common data and costume root.
fn nana_files<'a>(fixture: &'a Fixture, nana: &'a DatFile) -> FighterAnimationFiles<'a> {
    FighterAnimationFiles {
        fighter: nana,
        ..fixture.files(2)
    }
}

fn bind_nana(fixture: &Fixture, nana: &DatFile) -> Result<FighterAnimationBinding, Error> {
    bind_nana_fighter_animation(&nana_files(fixture, nana), &fixture.fighter, 2, 1)
}

#[test]
fn unresolved_nana_motion_plays_popo_record_with_nana_flags_and_parts() {
    let (fixture, nana) = nana_fixture(&[1, 0, 1]);
    let binding = bind_nana(&fixture, &nana).unwrap();
    assert_eq!(binding.animation_symbol, SYMBOL);
    assert_eq!(binding.archive_range, 32..fixture.aj.len() - 32);
    assert_eq!(binding.packed_flags, u32::from(NANA));
    assert_eq!(
        binding.receivers,
        [JObjId(0x220), JObjId(0x120), JObjId(0x340)]
    );
}

#[test]
fn populated_nana_record_does_not_take_the_popo_fallback() {
    let (fixture, mut nana) = nana_fixture(&[1, 0, 1]);
    put(&mut nana.data, RECORD + 8, 1);
    assert!(matches!(
        bind_nana(&fixture, &nana),
        Err(Error::NanaRecordPresent)
    ));
}

#[test]
fn nana_motion_flags_select_the_attach_path_not_popo_flags() {
    let (fixture, mut nana) = nana_fixture(&[1, 0, 1]);
    put(&mut nana.data, RECORD + 0x10, u32::from(POPO));
    assert!(matches!(
        bind_nana(&fixture, &nana),
        Err(Error::RemappedAnimation { fighter_kind, source_kind: POPO })
            if fighter_kind == FighterKind::NANA
    ));
    let (fixture, mut nana) = nana_fixture(&[1, 0, 1]);
    put(&mut nana.data, RECORD + 0x10, 0x200 | u32::from(NANA));
    assert!(matches!(
        bind_nana(&fixture, &nana),
        Err(Error::AuxiliaryEnabled(_))
    ));
}

#[test]
fn nana_fallback_checks_both_authenticated_counts() {
    let (fixture, nana) = nana_fixture(&[1, 0, 1]);
    assert!(matches!(
        bind_nana_fighter_animation(&nana_files(&fixture, &nana), &fixture.fighter, 1, 1),
        Err(Error::AnimationIndex { index: 1, count: 1 })
    ));
}
