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
//! - Actions: the blaster comes out in `SpecialNStart` and fires lasers
//!   through `SpecialNLoop` (`ftFox_SpecialN_StartAnimation` sets
//!   `ftFx_SpecialN_CreateBlasterShot`); the illusion trails `SpecialS`
//!   (`ftFox_SpecialS_CreateGhostItem`).

use crate::error::Result;
use crate::fighter::script::{ScriptError, cmd_var_frames};
use crate::file_names::{Character, Effects, MeleeSlot};
use crate::vanilla::vanilla_shared;
use dat_parser::DatFile;
use dat_parser::descriptor::generic_animation::{RawAnimJointGraph, RawGenericAnimationError};
use dat_parser::descriptor::jobj::flags as jobj;
use dat_parser::descriptor::mobj::render_flags;
use dat_parser::descriptor::{DescriptorParseError, DescriptorReader};
use dat_parser::hsd::animation::{HsdJointPoseEvaluator, HsdJointPoseLimits, attach_anim_joints};
use dat_parser::hsd::draw::{
    HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork, HsdJointConstraint, HsdRootPose,
};
use dat_parser::hsd::scene::{HsdJointIndex, HsdScene, HsdTransform, hsd_scene_limits};
use dat_parser::hsd::source::{self, HsdSource};
use dat_parser::math::Mat4;
use sha2::{Digest, Sha256};

/// Where a shared model lives in its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Place {
    /// The article in `ftData.x48_items[slot]` of the fighter's data file.
    Article(u32),
    /// The models of these `EffectDataTable.descs` in the effects file.
    Effects(&'static [u32]),
}

/// How a model drawn on its fighter follows the part it spawns at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Follow {
    /// Its position only, at the fighter's scale, as an effect does
    /// (`efLib_Create_Attach_Scale`).
    Position,
    /// Its grip joint held to the part by position and orientation, as a
    /// held item is: the blaster in Fox's hand (`it_80274F48`), at
    /// [`HELD_SCALE`] of the item's own scale.
    Held,
    /// Fired from a point on the part at the frames the action's script
    /// sets `cmd_vars[2]`, then flying on its own: Fox's laser
    /// (`ftFx_SpecialN_CreateBlasterShot`). See [`Firing`].
    Fired,
}

/// One of a shared model's roots drawn on its fighter: at which fighter
/// part (`Fighter_Part`), following it how, while which actions play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Spawn {
    /// Its root among the model's.
    root: usize,
    pub part: u8,
    follow: Follow,
    actions: &'static [&'static str],
}

/// `FtPart_TransN`, `FtPart_HipN` and `FtPart_RThumbNb` (`ft/forward.h`).
const TRANS_N: u8 = 1;
const HIP_N: u8 = 4;
const RTHUMB_NB: u8 = 49;

/// The blaster's scale against its item scale (`it_8026BAE8(blaster, 0.85)`
/// in `ftFox_SpecialN_SpawnBlaster`).
const HELD_SCALE: f32 = 0.85;

/// The neutral special's actions, through which the blaster is out.
const SPECIAL_N: &[&str] = &[
    "SpecialNStart",
    "SpecialNLoop",
    "SpecialNEnd",
    "SpecialAirNStart",
    "SpecialAirNLoop",
    "SpecialAirNEnd",
];
const BLASTER: &[Spawn] = &[Spawn {
    root: 0,
    part: RTHUMB_NB,
    follow: Follow::Held,
    actions: SPECIAL_N,
}];
/// Shots leave the blaster's muzzle only from the loops
/// (`ftFx_SpecialNLoop_Anim`, `ftFx_SpecialAirNLoop_Anim`).
const LASER: &[Spawn] = &[Spawn {
    root: 0,
    part: RTHUMB_NB,
    follow: Follow::Fired,
    actions: &["SpecialNLoop", "SpecialAirNLoop"],
}];
/// The loop effect, from the reflector's loop on (`ftFx_SpecialLw_CreateLoopGFX`).
const SHINE: &[Spawn] = &[Spawn {
    root: 0,
    part: HIP_N,
    follow: Follow::Position,
    actions: &[
        "SpecialLwLoop",
        "SpecialLwHit",
        "SpecialAirLwLoop",
        "SpecialAirLwHit",
    ],
}];
/// The charge at `TransN` while held, then the launch at the hip.
const FIRE: &[Spawn] = &[
    Spawn {
        root: 0,
        part: TRANS_N,
        follow: Follow::Position,
        actions: &["SpecialHiHold", "SpecialHiHoldAir"],
    },
    Spawn {
        root: 1,
        part: HIP_N,
        follow: Follow::Position,
        actions: &["SpecialHi"],
    },
];

/// One shared model as listed: what players call it, where it lives, the
/// action that shows it, and its roots drawn on the fighter or fired from it.
/// An illusion trails the fighter on its own, so it has none yet.
type Listing = (&'static str, Place, &'static str, &'static [Spawn]);

/// Each fighter's shared models, by its file code.
const SHARED: &[(&str, &[Listing])] = &[
    (
        "Fx",
        &[
            ("Laser", Place::Article(0), "SpecialNLoop", LASER),
            ("Blaster", Place::Article(1), "SpecialNLoop", BLASTER),
            ("Illusion", Place::Article(2), "SpecialS", &[]),
            ("Shine", Place::Effects(&[0]), "SpecialLwLoop", SHINE),
            ("Fire Fox", Place::Effects(&[3, 4]), "SpecialHiHold", FIRE),
        ],
    ),
    (
        "Fc",
        &[
            ("Laser", Place::Article(0), "SpecialNLoop", LASER),
            ("Blaster", Place::Article(1), "SpecialNLoop", BLASTER),
            ("Phantasm", Place::Article(3), "SpecialS", &[]),
            ("Shine", Place::Effects(&[0]), "SpecialLwLoop", SHINE),
            ("Fire Bird", Place::Effects(&[3, 4]), "SpecialHiHold", FIRE),
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
    #[error("an effect's joint animation does not read: {0}")]
    Animation(#[from] RawGenericAnimationError),
    #[error("the fighter's move script does not read: {0}")]
    Script(#[from] ScriptError),
}

/// A model every costume of a fighter shares: see the module documentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SharedModel {
    character: Character,
    name: &'static str,
    place: Place,
    action: &'static str,
    spawns: &'static [Spawn],
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
            .map(move |&(name, place, action, spawns)| Self {
                character,
                name,
                place,
                action,
                spawns,
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

    /// The fighter's action that shows it, as its animations name it
    /// (`SpecialLwLoop`, where the shine spawns).
    pub fn action(self) -> &'static str {
        self.action
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

    /// Its roots drawn on its fighter while `action` plays, loaded from
    /// `bytes`, its file; none for an action that doesn't spawn it.
    pub fn spawned(self, bytes: &[u8], action: &str) -> Result<Vec<SpawnedModel>> {
        let spawns: Vec<Spawn> = self
            .spawns
            .iter()
            .filter(|spawn| spawn.follow != Follow::Fired && spawn.actions.contains(&action))
            .copied()
            .collect();
        if spawns.is_empty() {
            return Ok(Vec::new());
        }
        let dat = source::parse(bytes)?;
        let roots = self.model_roots(&dat)?;
        let item_scale = self.item_scale(&dat)?;
        spawns
            .into_iter()
            .map(|spawn| {
                let root = *roots
                    .get(spawn.root)
                    .ok_or(SharedModelError::NullPointer("spawned root"))?;
                let mut source = HsdSource::from_model_roots(
                    &dat,
                    &[root],
                    HsdDrawEvaluationPolicy::GENERIC_HSD,
                )?;
                if self.depth_always(&dat, spawn.root)? {
                    always_in_front(&mut source.scene);
                }
                let held_joint = self.held_joint(&dat, &source.scene)?;
                let limits = HsdJointPoseLimits {
                    max_joints: source.scene.roots[0].joints.len(),
                    ..HsdJointPoseLimits::default()
                };
                // An effect plays its joint animation from when it spawns;
                // an article without one keeps its serialized pose.
                let animation = match self.anim_joint(&dat, spawn.root)? {
                    Some(anim_joint) => {
                        let graph = RawAnimJointGraph::parse(&dat, anim_joint)
                            .map_err(SharedModelError::from)?;
                        let mut animation = attach_anim_joints(&source.scene, 0, &graph, limits)?;
                        animation.set_looping(true);
                        // `efLib_Create` requests frame 0 (`HSD_JObjReqAnimAll`)
                        // and `efAlt_Spawn` applies it at once.
                        animation.request(0.0)?;
                        animation.advance()?;
                        animation.into_owned()
                    }
                    None => HsdJointPoseEvaluator::unanimated(&source.scene, 0, limits)?,
                };
                Ok(SpawnedModel {
                    source,
                    spawn,
                    item_scale,
                    held_joint,
                    animation,
                })
            })
            .collect()
    }

    /// The joint animation of its root `root` (`StaticModelDesc.animjoint`,
    /// after the joint), for an effect that has one.
    fn anim_joint(
        self,
        dat: &DatFile,
        root: usize,
    ) -> std::result::Result<Option<u32>, SharedModelError> {
        let Place::Effects(descs) = self.place else {
            return Ok(None);
        };
        let Some(&desc) = descs.get(root) else {
            return Ok(None);
        };
        let table = unique_root(dat, "eff*DataTable", |name| {
            name.starts_with("eff") && name.ends_with("DataTable")
        })?;
        Ok(
            DescriptorReader::new(dat, "EffectDataTable", table).pointer(
                "StaticModelDesc.animjoint",
                0x8 + desc * EFFECT_DESC_SIZE + 0x8,
            )?,
        )
    }

    /// Whether the game draws its effect root `root` over everything already
    /// drawn: `efLib_Create` sets `RENDER_ZMODE_ALWAYS` on every material of
    /// an effect whose lifetime has a nonzero first decimal (the shine's is
    /// 0.1). Effects draw after fighters, so it shows over them.
    fn depth_always(
        self,
        dat: &DatFile,
        root: usize,
    ) -> std::result::Result<bool, SharedModelError> {
        let Place::Effects(descs) = self.place else {
            return Ok(false);
        };
        let Some(&desc) = descs.get(root) else {
            return Ok(false);
        };
        let table = unique_root(dat, "eff*DataTable", |name| {
            name.starts_with("eff") && name.ends_with("DataTable")
        })?;
        let lifetime = DescriptorReader::new(dat, "EffectDataTable", table)
            .f32(0x8 + desc * EFFECT_DESC_SIZE)?;
        Ok(!((10.0 * lifetime) as u32).is_multiple_of(10))
    }

    /// The joint of `scene`, its model, a hand holds it by: for an article,
    /// `ItemModelDesc.x8_bone_attach_id` first children down from the root
    /// (`it_80274F48`, with no bone table); the root otherwise.
    fn held_joint(
        self,
        dat: &DatFile,
        scene: &HsdScene,
    ) -> std::result::Result<HsdJointIndex, SharedModelError> {
        let Some(article) = self.article(dat)? else {
            return Ok(HsdJointIndex(0));
        };
        let model = required(article, "Article.x10_modelDesc", 0x10)?;
        let depth = DescriptorReader::new(dat, "ItemModelDesc", model).u32(0x8)?;
        let joints = &scene.roots[0].joints;
        let mut joint = HsdJointIndex(0);
        for _ in 0..depth {
            joint = *joints[joint.0]
                .children
                .first()
                .ok_or(SharedModelError::NullPointer(
                    "ItemModelDesc.x8_bone_attach_id",
                ))?;
        }
        Ok(joint)
    }

    /// An article's own scale (`ItemAttr.x60_scale`), which a held one is
    /// drawn at a part of; 1 for an effect.
    fn item_scale(self, dat: &DatFile) -> std::result::Result<f32, SharedModelError> {
        let Some(article) = self.article(dat)? else {
            return Ok(1.0);
        };
        let attributes = required(article, "Article.x0_common_attr", 0)?;
        Ok(DescriptorReader::new(dat, "ItemAttr", attributes).f32(0x60)?)
    }

    /// Its article in `dat`, the fighter's data file, when it is one.
    fn article(
        self,
        dat: &DatFile,
    ) -> std::result::Result<Option<DescriptorReader<'_>>, SharedModelError> {
        let Place::Article(slot) = self.place else {
            return Ok(None);
        };
        let root = unique_root(dat, "ftData", |name| name.starts_with("ftData"))?;
        let ft_data = DescriptorReader::new(dat, "ftData", root);
        let items = required(ft_data, "ftData.x48_items", 0x48)?;
        let items = DescriptorReader::new(dat, "x48_items", items);
        let article = required(items, "x48_items entry", slot * 4)?;
        Ok(Some(DescriptorReader::new(dat, "Article", article)))
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

/// One of a shared model's roots, drawn on its fighter while a move plays:
/// see [`SharedModel::spawned`].
pub struct SpawnedModel {
    source: HsdSource,
    spawn: Spawn,
    /// For a held article, its own scale.
    item_scale: f32,
    /// For a held article, the joint the hand holds it by
    /// (`ItemModelDesc.x8_bone_attach_id`); the root otherwise.
    held_joint: HsdJointIndex,
    /// Its joint animation from when it spawned, or its serialized pose.
    animation: HsdJointPoseEvaluator<'static>,
}

impl SpawnedModel {
    pub fn spawn(&self) -> Spawn {
        self.spawn
    }

    /// Face its billboarded joints (the shine's) toward a camera with this
    /// `view` in later poses; `None` leaves them as posed.
    pub fn set_view(&mut self, view: Option<Mat4>) {
        self.source.evaluator.set_view(view);
    }

    /// Step its animation one frame, as the fighter's does each tick.
    pub fn advance(&mut self) -> Result<()> {
        self.animation.advance()?;
        Ok(())
    }

    /// Pose it on its fighter for a frame: `joint` is its part's world
    /// matrix in the fighter's pose, and `fighter_scale` the fighter's
    /// model scale. [`Self::drawn`] then reads the result.
    pub fn pose(&mut self, joint: Mat4, fighter_scale: f32) -> Result<()> {
        let animated = self.animation.pose()?;
        let mut transforms = animated.transforms.to_vec();
        // The root's scale is set when it spawns, as the game sets it: an
        // effect takes the fighter's (`efLib_Create_Attach_Scale`), a held
        // item its own (`it_8026BAE8`).
        let (scale, constraint) = match self.spawn.follow {
            // A fired model is never posed on its fighter: see [`Shot`].
            Follow::Position | Follow::Fired => (
                fighter_scale,
                HsdJointConstraint {
                    joint: HsdJointIndex(0),
                    target: joint,
                    position: true,
                    orientation: false,
                },
            ),
            Follow::Held => (
                HELD_SCALE * self.item_scale,
                HsdJointConstraint {
                    joint: self.held_joint,
                    target: joint,
                    position: true,
                    orientation: true,
                },
            ),
        };
        transforms[0].scale = [scale; 3];
        let pose = HsdRootPose {
            transforms: &transforms,
            constraints: &[constraint],
            ..animated
        };
        self.source
            .evaluator
            .evaluate(&self.source.scene, &[pose])?;
        Ok(())
    }

    /// Its scene and the draw work its last [`Self::pose`] left, so several
    /// can be posed and then drawn together.
    pub fn drawn(&self) -> (&HsdScene, &HsdEvaluatedDrawWork) {
        (&self.source.scene, self.source.evaluator.work())
    }
}

/// Draw `scene` over whatever is already drawn: `RENDER_ZMODE_ALWAYS` on
/// every material, as `lb_80011C18` sets it, which skips particle and spline
/// joints.
fn always_in_front(scene: &mut HsdScene) {
    for joint in scene.roots.iter_mut().flat_map(|root| &mut root.joints) {
        if joint.flags & (jobj::PTCL | jobj::SPLINE) != 0 {
            continue;
        }
        for object in &mut joint.display_objects {
            if let Some(material) = &mut object.material {
                material.render_flags |= render_flags::ZMODE_ALWAYS;
            }
        }
    }
}

/// Where a shot leaves the blaster: this point in the hand bone's space,
/// flattened onto the stage plane (`ftFox_SpecialN_GetHoldJoint`,
/// `ftFox_SpecialN_PrepareBlasterShot`).
const MUZZLE: [f32; 3] = [0.0, 1.2325, 4.2636];

/// How far a ray's length grows per unit it travels
/// (`Item_UpdateRayAnimation`'s 11.25).
const RAY_GROWTH: f32 = 11.25;

/// A shared model fired from its fighter as an action plays: when, how
/// fast, which way, and for how long. Each shot is a [`Shot`].
pub struct Firing {
    dat: DatFile,
    root: u32,
    /// The action's frames that fire, from its script.
    pub frames: Vec<f32>,
    /// The fighter part shots leave from.
    pub part: u8,
    speed: f32,
    angle: f32,
    lifetime: f32,
    max_length: f32,
    item_scale: f32,
}

impl SharedModel {
    /// How it is fired while `action`, the fighter's animation `animation`,
    /// plays, read from `bytes`, its file; `None` for a model that isn't
    /// fired in that action.
    pub fn firing(self, bytes: &[u8], action: &str, animation: usize) -> Result<Option<Firing>> {
        let Some(spawn) = self
            .spawns
            .iter()
            .find(|spawn| spawn.follow == Follow::Fired && spawn.actions.contains(&action))
        else {
            return Ok(None);
        };
        let dat = source::parse(bytes)?;
        let frames = cmd_var_frames(&dat, animation, 2).map_err(SharedModelError::from)?;
        let root = self.model_roots(&dat)?[spawn.root];
        let shot = self.shot_attributes(&dat)?;
        Ok(Some(Firing {
            dat,
            root,
            frames,
            part: spawn.part,
            speed: shot.speed,
            angle: shot.angle,
            lifetime: shot.lifetime,
            max_length: shot.max_length,
            item_scale: shot.item_scale,
        }))
    }

    /// What a fired article's shots do, from its attributes and its
    /// fighter's.
    fn shot_attributes(
        self,
        dat: &DatFile,
    ) -> std::result::Result<ShotAttributes, SharedModelError> {
        let article = self
            .article(dat)?
            .ok_or(SharedModelError::NullPointer("fired article"))?;
        // The laser's own (`FoxLaserAttr`): its lifetime, then its longest
        // stretch.
        let laser = required(article, "Article.x4_specialAttributes", 0x4)?;
        let laser = DescriptorReader::new(dat, "FoxLaserAttr", laser);
        let common = required(article, "Article.x0_common_attr", 0)?;
        // The fighter's (`ftFox_DatAttrs`, `ftData.x4`): its angle and speed.
        let ft_data = unique_root(dat, "ftData", |name| name.starts_with("ftData"))?;
        let fighter = required(
            DescriptorReader::new(dat, "ftData", ft_data),
            "ftData.x4",
            0x4,
        )?;
        let fighter = DescriptorReader::new(dat, "ftFox_DatAttrs", fighter);
        Ok(ShotAttributes {
            lifetime: laser.f32(0x0)?,
            max_length: laser.f32(0x4)?,
            item_scale: DescriptorReader::new(dat, "ItemAttr", common).f32(0x60)?,
            angle: fighter.f32(0x10)?,
            speed: fighter.f32(0x14)?,
        })
    }
}

struct ShotAttributes {
    lifetime: f32,
    max_length: f32,
    item_scale: f32,
    angle: f32,
    speed: f32,
}

impl Firing {
    /// A shot leaving the part whose world matrix is `joint`, from a
    /// fighter facing right (`facing` 1) or left (-1).
    pub fn fire(&self, joint: Mat4, facing: f32) -> Result<Shot> {
        let source = HsdSource::from_model_roots(
            &self.dat,
            &[self.root],
            HsdDrawEvaluationPolicy::GENERIC_HSD,
        )?;
        let mut position = joint.transform_point(MUZZLE);
        position[2] = 0.0;
        let angle = if facing >= 0.0 {
            self.angle
        } else {
            std::f32::consts::PI - self.angle
        };
        let transforms = source.scene.roots[0]
            .joints
            .iter()
            .map(|joint| joint.local)
            .collect();
        Ok(Shot {
            source,
            transforms,
            position,
            angle,
            speed: self.speed,
            length: 0.0,
            max_length: self.max_length,
            item_scale: self.item_scale,
            life: self.lifetime,
        })
    }
}

/// One shot in flight: it stretches out from where it was fired and flies
/// straight on at its speed until its lifetime runs out
/// (`Item_UpdateRayAnimation`, `it_80273130`).
pub struct Shot {
    source: HsdSource,
    transforms: Vec<HsdTransform>,
    position: [f32; 3],
    angle: f32,
    speed: f32,
    /// Its root's Z scale: how far its length has grown.
    length: f32,
    max_length: f32,
    item_scale: f32,
    /// Frames left.
    life: f32,
}

impl Shot {
    /// Whether it still flies.
    pub fn alive(&self) -> bool {
        self.life > 0.0
    }

    /// Step it one frame: it grows, ages, and moves on.
    pub fn advance(&mut self) {
        self.length = (self.length + self.speed.abs() / RAY_GROWTH).min(self.max_length);
        if self.length < 1e-5 {
            self.length = 1e-3;
        }
        self.life -= 1.0;
        self.position[0] += self.speed * self.angle.cos();
        self.position[1] += self.speed * self.angle.sin();
    }

    /// Pose it where it is, turned along its flight. [`Self::drawn`] then
    /// reads the result.
    pub fn pose(&mut self) -> Result<()> {
        let (x, y) = (self.speed * self.angle.cos(), self.speed * self.angle.sin());
        let facing = if x > 0.0 { 1.0 } else { -1.0 };
        let x = if facing == 1.0 { -x } else { x };
        self.transforms[0] = HsdTransform {
            scale: [self.item_scale, self.item_scale, self.length],
            rotation: [
                std::f32::consts::PI + y.atan2(x),
                std::f32::consts::FRAC_PI_2 * facing,
                0.0,
            ],
            translation: self.position,
        };
        let pose = HsdRootPose {
            root_index: 0,
            transforms: &self.transforms,
            hidden_joints: None,
            constraints: &[],
        };
        self.source
            .evaluator
            .evaluate(&self.source.scene, &[pose])?;
        Ok(())
    }

    /// Its scene and the draw work its last [`Self::pose`] left.
    pub fn drawn(&self) -> (&HsdScene, &HsdEvaluatedDrawWork) {
        (&self.source.scene, self.source.evaluator.work())
    }
}
