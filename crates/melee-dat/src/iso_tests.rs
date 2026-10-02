//! Tests that need the game itself: stock costumes and the fighter files
//! they play with cannot be written by hand or committed. They read a clean
//! Melee NTSC 1.02 disc image named by `TGG_MELEE_ISO`:
//!
//! ```text
//! TGG_MELEE_ISO=/path/to/melee.iso cargo test -p melee-dat --features melee-iso
//! ```

use crate::catalog::CATALOG_JSON;
use crate::fighter::places::{BodyRegion, CostumePlaces, ModelDetail};
use crate::fighter::playback::hidden_display_objects;
use crate::{MeleeFighterPlayback, MeleeReferenceCatalog, MeleeReferenceStore};
use dat_parser::DatFile;
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::draw::HsdDrawEvaluationPolicy;
use dat_parser::hsd::scene::DObjId;
use dat_parser::hsd::source::HsdSource;
use gc_iso::Disc;

fn disc() -> Disc {
    let iso = std::env::var_os("TGG_MELEE_ISO")
        .expect("set TGG_MELEE_ISO to a clean Melee NTSC 1.02 disc image");
    Disc::open(iso).expect("open the disc image")
}

/// The references `scene`'s costume plays with, from the disc.
fn references(
    disc: &mut Disc,
    catalog: &MeleeReferenceCatalog,
    scene: &HsdScene,
) -> MeleeReferenceStore {
    MeleeReferenceStore::for_costume(catalog, scene, |name| disc.read(name).ok())
        .expect("a stock costume is recognized")
}

/// Fighter tables address textures by where they sit in the costume, so
/// each texture is named for the body part that draws it.
#[test]
fn stock_costume_textures_are_placed_on_the_fighter() {
    let mut disc = disc();
    let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
    let mut places = |file: &str| {
        let raw = disc.read(file).expect("read the stock costume");
        let dat = DatFile::parse(&raw).expect("parse the stock costume");
        let scene = HsdScene::from_dat(&dat).expect("build the costume scene");
        let store = references(&mut disc, &catalog, &scene);
        let places = CostumePlaces::read(&dat, &scene, &catalog, &store)
            .expect("read places")
            .expect("a stock costume is recognized");
        scene
            .textures
            .iter()
            .map(|texture| places.place_of(&[texture.id]))
            .collect::<Vec<_>>()
    };

    // Falco Red: boots, the head's textures, gloves, and the low-poly model.
    let falco = places("PlFcRe.dat");
    let at = |index: usize| falco[index].expect("placed");
    assert_eq!(at(0).region, BodyRegion::Feet);
    assert_eq!(at(0).detail, Some(ModelDetail::High));
    for head in 13..=19 {
        assert_eq!(at(head).region, BodyRegion::Head, "texture {head}");
    }
    // Textures 13 and 19 are the eyes, which blink through texture
    // animations.
    assert_eq!(at(13).label(), "Eyes");
    assert_eq!(at(19).label(), "Eyes");
    assert_eq!(at(14).label(), "Head");
    assert_eq!(at(11).region, BodyRegion::Hands);
    assert_eq!(at(24).label(), "Hips · low poly");
    assert_eq!(at(44).label(), "Left foot · low poly");

    // Kirby has no head part: his round body is "Body", not "Hips".
    let kirby = places("PlKbNr.dat");
    assert_eq!(kirby[0].expect("placed").region, BodyRegion::Body);
}

/// The parts a costume hides (alternate faces, hands, items, low-poly
/// models) are read at run time from the fighter's data. The catalog holds
/// the same masks derived once from the game's source; the two must agree
/// for every stock costume on the disc.
#[test]
fn runtime_masks_match_the_catalog_for_every_stock_costume() {
    let mut disc = disc();
    let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
    let raw: serde_json::Value = serde_json::from_str(CATALOG_JSON).expect("catalog JSON");
    let costumes: Vec<String> = disc
        .files()
        .iter()
        .filter(|file| !file.is_dir && crate::vanilla::vanilla_file(&file.name).is_some())
        .filter(|file| file.name.starts_with("Pl"))
        .map(|file| file.name.clone())
        .collect();
    let mut compared = 0;
    for name in costumes {
        let bytes = disc.read(&name).expect("costume");
        let source = HsdSource::from_dat(&bytes, HsdDrawEvaluationPolicy::MELEE_FIGHTER)
            .expect("a stock costume loads");
        let (profile, root_index, costume) =
            MeleeFighterPlayback::profile_for(&source.scene, &catalog)
                .expect("a stock costume is recognized");
        let expected = raw["rosterIdleProfiles"]
            .as_array()
            .expect("profiles")
            .iter()
            .find(|value| value["fighterKind"] == profile.fighter_kind)
            .map(|value| &value["initialization"]["displayObjects"])
            .expect("raw profile");
        // Some fighters hide nothing; the catalog records no mask for them.
        if expected.is_null() {
            continue;
        }
        let objects: Vec<u32> = source.scene.roots[root_index]
            .joints
            .iter()
            .flat_map(|joint| &joint.display_objects)
            .map(|object| object.source_id.0)
            .collect();
        let fighter = catalog.fighter(profile.fighter_kind).expect("fighter");
        let fighter_file = &catalog
            .asset(&fighter.fighter_key)
            .expect("asset")
            .file_name;
        let fighter_dat = disc.read(fighter_file).expect("fighter data");
        let fighter_dat = DatFile::parse(&fighter_dat).expect("fighter DAT");
        let hidden = hidden_display_objects(&fighter_dat, profile.fighter_kind, costume, &objects)
            .expect("mask");
        let hidden: Vec<u64> = objects
            .iter()
            .enumerate()
            .filter(|(_, id)| hidden.contains(&DObjId(**id)))
            .map(|(ordinal, _)| ordinal as u64)
            .collect();
        let expected: Vec<u64> = expected["hiddenIndices"]
            .as_array()
            .expect("hidden indices")
            .iter()
            .map(|value| value.as_u64().expect("ordinal"))
            .collect();
        assert_eq!(hidden, expected, "{name}");
        compared += 1;
    }
    assert!(compared > 50, "only {compared} costumes compared");
}

/// Any animation plays, seeks, and changes speed; a refused one leaves the
/// current animation playing.
#[test]
fn a_fighter_plays_seeks_and_speeds_up_any_animation() {
    let mut disc = disc();
    let catalog = MeleeReferenceCatalog::checked_in().expect("catalog");
    let bytes = disc.read("PlFcNr.dat").expect("Falco");
    let source =
        HsdSource::from_dat(&bytes, HsdDrawEvaluationPolicy::MELEE_FIGHTER).expect("source");
    let store = references(&mut disc, &catalog, &source.scene);
    let Ok(mut playback) = MeleeFighterPlayback::attach(source, &catalog, &store) else {
        panic!("Falco attaches");
    };
    let find = |action: &str| {
        playback
            .animations()
            .iter()
            .find(|animation| animation.action.as_deref() == Some(action))
            .expect(action)
            .clone()
    };
    let (idle, jab, back_throw) = (find("Wait1"), find("Attack11"), find("ThrowB"));
    assert_eq!(playback.current(), idle.index);
    assert_eq!(jab.name.as_deref(), Some("Jab 1"));

    playback.play(jab.index).expect("jab binds");
    assert_eq!(playback.label(), "Falco Attack11");
    let end = playback.end_frame();
    playback.seek(end + 10.0).expect("seek");
    assert_eq!(playback.frame(), end);

    // At double speed a cycle takes half the ticks.
    playback.set_rate(2.0).expect("rate");
    playback.reset().expect("reset");
    while playback.loops() == 0 {
        playback.advance().expect("advance");
    }
    assert_eq!(playback.tick(), (end / 2.0).ceil() as u64);
    assert!(playback.set_rate(0.0).is_err());

    // Back throw requests a partial part the binder doesn't reproduce.
    assert!(playback.play(back_throw.index).is_err());
    assert_eq!(playback.current(), jab.index);
}
