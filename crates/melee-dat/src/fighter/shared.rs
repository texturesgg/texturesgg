//! What every costume of a fighter shares: the models its data file and its
//! effects file hold for the game to spawn, such as Fox's laser and his
//! shine. A skin for one of these files changes them for every costume.
//!
//! Each fighter's models are listed by hand, from the code that spawns them,
//! so each can be named as players know it. Sources, in the Melee
//! decompilation:
//!
//! - Articles: `ftData.x48_items` (`ft/types.h`), whose slots each fighter's
//!   code reads by index (`FTDATA_ITEM_KIND_FOX`, `FTDATA_ITEM_KIND_FALCO`);
//!   an article's model is `Article.x10_modelDesc->x0_joint` (`it/types.h`).
//! - Effects: `EffectDataTable.descs[gfx_id % 1000]` (`ef/types.h`,
//!   `efLib_Create`), each a lifetime and a `StaticModelDesc` whose joint is
//!   the model. Fox's down special spawns 3000 to 3002 at his hip
//!   (`ftFx_SpecialLw_CreateLoopGFX` and its start and reflect siblings,
//!   through `efAlt_Spawn`); his up special spawns 3003 at `TransN` and 3004
//!   at his hip (`ftFx_SpecialHi_CreateChargeGFX`, `…CreateLaunchGFX`).
//!   Falco's moves are Fox's code (`ftfalco.c`), so his are the same.

use crate::error::Result;
use crate::file_names::{Character, Effects, MeleeSlot};
use crate::vanilla::vanilla_shared;
use dat_parser::DatFile;
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};
use dat_parser::hsd::scene::{HsdScene, HsdTransform, hsd_scene_limits};
use dat_parser::hsd::source;
use sha2::{Digest, Sha256};

/// Where a shared model lives in its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Place {
    /// The article in `ftData.x48_items[slot]` of the fighter's data file.
    Article(u32),
    /// The models of these `EffectDataTable.descs` in the effects file.
    Effects(&'static [u32]),
}

/// Each fighter's shared models, by its file code: what players call each,
/// and where it lives.
const SHARED: &[(&str, &[(&str, Place)])] = &[
    (
        "Fx",
        &[
            ("Laser", Place::Article(0)),
            ("Blaster", Place::Article(1)),
            ("Illusion", Place::Article(2)),
            ("Shine", Place::Effects(&[0])),
            ("Fire Fox", Place::Effects(&[3, 4])),
        ],
    ),
    (
        "Fc",
        &[
            ("Laser", Place::Article(0)),
            ("Blaster", Place::Article(1)),
            ("Phantasm", Place::Article(3)),
            ("Shine", Place::Effects(&[0])),
            ("Fire Bird", Place::Effects(&[3, 4])),
        ],
    ),
];

/// `sizeof(EF_EffectDesc)`: a lifetime and a `StaticModelDesc`.
const EFFECT_DESC_SIZE: u32 = 0x14;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SharedModelError {
    #[error("missing or ambiguous {0} public root")]
    Root(&'static str),
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("required {0} pointer is null")]
    NullPointer(&'static str),
}

/// A model every costume of a fighter shares: see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SharedModel {
    character: Character,
    name: &'static str,
    place: Place,
}

impl SharedModel {
    /// `character`'s shared models, in the order players meet them. A
    /// fighter not yet listed has none.
    pub fn of(character: Character) -> impl Iterator<Item = Self> {
        SHARED
            .iter()
            .find(|(code, _)| *code == character.code())
            .into_iter()
            .flat_map(|(_, models)| models.iter())
            .map(move |&(name, place)| Self {
                character,
                name,
                place,
            })
    }

    /// Every listed shared model `file` holds, of every fighter it's for:
    /// an effects file holds the same shine for Fox and for Falco.
    pub fn in_file(file: MeleeSlot) -> impl Iterator<Item = Self> {
        Character::all()
            .flat_map(Self::of)
            .filter(move |model| model.slot() == file)
    }

    pub fn character(self) -> Character {
        self.character
    }

    /// The name players use ("Laser").
    pub fn name(self) -> &'static str {
        self.name
    }

    /// The file it lives in: the fighter's data file, or its effects file.
    pub fn slot(self) -> MeleeSlot {
        match self.place {
            Place::Article(_) => MeleeSlot::FighterData(self.character),
            Place::Effects(_) => MeleeSlot::Effects(
                Effects::of(self.character).expect("a listed effect has its file"),
            ),
        }
    }

    /// Where its models' root joints are in `dat`, its file.
    pub fn model_roots(self, dat: &DatFile) -> std::result::Result<Vec<u32>, SharedModelError> {
        match self.place {
            Place::Article(slot) => {
                let root = unique_root(dat, "ftData", |name| name.starts_with("ftData"))?;
                let ft_data = DescriptorReader::new(dat, "ftData", root);
                let items = required(ft_data, "ftData.x48_items", 0x48)?;
                let items = DescriptorReader::new(dat, "x48_items", items);
                let article = required(items, "x48_items entry", slot * 4)?;
                let article = DescriptorReader::new(dat, "Article", article);
                let model = required(article, "Article.x10_modelDesc", 0x10)?;
                let model = DescriptorReader::new(dat, "ItemModelDesc", model);
                Ok(vec![required(model, "ItemModelDesc.x0_joint", 0)?])
            }
            Place::Effects(descs) => {
                let root = unique_root(dat, "eff*DataTable", |name| {
                    name.starts_with("eff") && name.ends_with("DataTable")
                })?;
                let table = DescriptorReader::new(dat, "EffectDataTable", root);
                descs
                    .iter()
                    .map(|&desc| {
                        // `descs` starts at 0x8; a desc's joint follows its lifetime.
                        required(
                            table,
                            "StaticModelDesc.joint",
                            0x8 + desc * EFFECT_DESC_SIZE + 0x4,
                        )
                    })
                    .collect()
            }
        }
    }

    /// Its models in `bytes`, its file, as a scene.
    pub fn scene(self, bytes: &[u8]) -> Result<HsdScene> {
        let dat = source::parse(bytes)?;
        let roots = self.model_roots(&dat)?;
        Ok(
            HsdScene::from_model_roots_with_limits(&dat, &roots, hsd_scene_limits())
                .map_err(source::HsdSourceError::from)?,
        )
    }

    /// A hash of what its models draw in `bytes`, its file: their joints,
    /// materials, decoded textures and vertices. Where the data sits in the
    /// file doesn't count, so a skin that moves it and changes nothing
    /// drawn has the same fingerprint.
    pub fn fingerprint(self, bytes: &[u8]) -> Result<String> {
        Ok(fingerprint(&self.scene(bytes)?))
    }

    /// Whether `bytes`, its file, draw it as the game shipped it.
    pub fn is_vanilla(self, bytes: &[u8]) -> Result<bool> {
        let expected = vanilla_shared(self.character, self.name);
        Ok(expected
            .is_some_and(|expected| self.fingerprint(bytes).ok().as_deref() == Some(expected)))
    }
}

fn fingerprint(scene: &HsdScene) -> String {
    let mut hash = Sha256::new();
    let floats = |hash: &mut Sha256, values: &[f32]| {
        values
            .iter()
            .for_each(|value| hash.update(value.to_bits().to_be_bytes()))
    };
    let transform = |hash: &mut Sha256, transform: &HsdTransform| {
        for part in [transform.scale, transform.rotation, transform.translation] {
            part.iter()
                .for_each(|value| hash.update(value.to_bits().to_be_bytes()));
        }
    };
    for root in &scene.roots {
        for joint in &root.joints {
            hash.update(joint.flags.to_be_bytes());
            transform(&mut hash, &joint.local);
            for object in &joint.display_objects {
                if let Some(material) = &object.material {
                    hash.update(material.render_flags.to_be_bytes());
                    if let Some(colors) = &material.colors {
                        hash.update(colors.ambient);
                        hash.update(colors.diffuse);
                        hash.update(colors.specular);
                        floats(&mut hash, &[colors.alpha, colors.shininess]);
                    }
                    for texture in &material.textures {
                        transform(&mut hash, &texture.transform);
                        let Some(index) = texture.texture else {
                            continue;
                        };
                        let texture = &scene.textures[index.0];
                        hash.update(texture.image.width.to_be_bytes());
                        hash.update(texture.image.height.to_be_bytes());
                        if let Ok(rgba) = &texture.rgba {
                            hash.update(rgba);
                        }
                    }
                }
                for polygon in &object.polygons {
                    for vertex in &polygon.decoded.vertices {
                        floats(&mut hash, &vertex.position);
                        floats(&mut hash, &vertex.normal);
                        floats(&mut hash, &vertex.color0);
                        vertex
                            .tex_coords
                            .iter()
                            .for_each(|coords| floats(&mut hash, coords));
                    }
                    for triangle in &polygon.decoded.triangles {
                        triangle
                            .iter()
                            .for_each(|index| hash.update((*index as u32).to_be_bytes()));
                    }
                }
            }
        }
    }
    format!("{:x}", hash.finalize())
}

fn unique_root(
    dat: &DatFile,
    what: &'static str,
    matches: impl Fn(&str) -> bool,
) -> std::result::Result<u32, SharedModelError> {
    let mut roots = dat.roots.iter().filter(|root| matches(&root.name));
    let root = roots.next().ok_or(SharedModelError::Root(what))?;
    if roots.next().is_some() {
        return Err(SharedModelError::Root(what));
    }
    Ok(root.data_offset)
}

fn required(
    reader: DescriptorReader<'_>,
    field: &'static str,
    relative: u32,
) -> std::result::Result<u32, SharedModelError> {
    reader
        .pointer(field, relative)?
        .ok_or(SharedModelError::NullPointer(field))
}
