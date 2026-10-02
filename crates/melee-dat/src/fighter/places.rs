//! Where a stock costume's textures sit on the fighter: a body region
//! ("Head", "Left hand", "Feet") and which model detail draws them.
//!
//! Most of a fighter's display objects hang on the root joint and are
//! envelope-skinned, so the owning joint says nothing. A display object's
//! region comes from its skinning instead: every vertex's joint weights,
//! gathered by region, where a joint's region is its nearest ancestor that is
//! a landmark part. Parts come from `PlCo.dat`'s parts table, placed on the
//! costume's joints as `ftParts_SetupParts` does. The fighter data's model
//! tables tell the high-poly model (table 0) from the low-poly one (table 1).
//!
//! Only these landmark parts name regions. The decompilation's
//! `Fighter_Part` names are unreliable past the torso (on Falco and Fox,
//! `NeckN` lands on a finger joint), so each landmark here was checked
//! against the stock trees: `HeadN` is the head's parent joint, the
//! `ShoulderJA` joints start each arm, and `BustN` holds the torso above the
//! hips.
//!
//! A texture a texture animation flips through on the head (or on a round
//! fighter's body) is an eye: in Melee, every stock fighter's texture
//! animations are its two eyes blinking and changing expression. Its frames
//! the scene never draws statically take the place of the TObj they animate.
//!
//! A costume the catalog doesn't recognize, or whose joint hierarchy differs
//! from the stock fighter's, has no places: the tables address joints and
//! display objects by position, so they only apply to stock-shaped trees.

use crate::catalog::MeleeReferenceCatalog;
use crate::error::{MeleeError, Result};
use crate::fighter::animation::fighter_part_slots;
use crate::fighter::parts::FighterModelParts;
use crate::references::MeleeReferenceStore;
use dat_parser::DatFile;
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::scene::{HsdPolygonBinding, HsdSceneRoot, HsdTextureSourceId, JObjId};
use dat_parser::hsd::texture_animation::texture_animations;
use std::collections::HashMap;

/// A region of a fighter's body. Left and right are the fighter's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BodyRegion {
    Head,
    Chest,
    Hips,
    /// A fighter with no head part (Kirby, Jigglypuff), or no landmark at all.
    Body,
    LeftArm,
    RightArm,
    Arms,
    LeftHand,
    RightHand,
    Hands,
    LeftLeg,
    RightLeg,
    Legs,
    LeftFoot,
    RightFoot,
    Feet,
}

impl BodyRegion {
    pub fn label(self) -> &'static str {
        match self {
            Self::Head => "Head",
            Self::Chest => "Chest",
            Self::Hips => "Hips",
            Self::Body => "Body",
            Self::LeftArm => "Left arm",
            Self::RightArm => "Right arm",
            Self::Arms => "Arms",
            Self::LeftHand => "Left hand",
            Self::RightHand => "Right hand",
            Self::Hands => "Hands",
            Self::LeftLeg => "Left leg",
            Self::RightLeg => "Right leg",
            Self::Legs => "Legs",
            Self::LeftFoot => "Left foot",
            Self::RightFoot => "Right foot",
            Self::Feet => "Feet",
        }
    }

    /// The fighter's other side, and the name for both together.
    fn mirror(self) -> Option<(Self, Self)> {
        Some(match self {
            Self::LeftArm => (Self::RightArm, Self::Arms),
            Self::RightArm => (Self::LeftArm, Self::Arms),
            Self::LeftHand => (Self::RightHand, Self::Hands),
            Self::RightHand => (Self::LeftHand, Self::Hands),
            Self::LeftLeg => (Self::RightLeg, Self::Legs),
            Self::RightLeg => (Self::LeftLeg, Self::Legs),
            Self::LeftFoot => (Self::RightFoot, Self::Feet),
            Self::RightFoot => (Self::LeftFoot, Self::Feet),
            _ => return None,
        })
    }

    /// The region a landmark `Fighter_Part` starts.
    fn of_part(part: u8) -> Option<Self> {
        Some(match part {
            34 => Self::Head,      // HeadN
            21 => Self::LeftHand,  // LHandN
            39 => Self::RightHand, // RHandN
            18 => Self::LeftArm,   // LShoulderJA
            36 => Self::RightArm,  // RShoulderJA
            10 => Self::LeftFoot,  // LFootJ
            15 => Self::RightFoot, // RFootJ
            6 => Self::LeftLeg,    // LLegJA
            11 => Self::RightLeg,  // RLegJA
            16 => Self::Chest,     // BustN
            4 => Self::Hips,       // HipN
            _ => return None,
        })
    }
}

/// Which of the fighter's models draws a texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelDetail {
    /// The model the game draws normally (model table 0).
    High,
    /// The low-poly alternative (model table 1).
    Low,
}

/// Where a texture sits on the fighter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TexturePlace {
    pub region: BodyRegion,
    /// `None` when both models (or neither) draw it.
    pub detail: Option<ModelDetail>,
    /// An eye: a texture animation on the head (or a round body) shows it.
    pub eyes: bool,
}

impl TexturePlace {
    /// "Head", "Eyes", or "Feet · low poly".
    pub fn label(&self) -> String {
        let what = if self.eyes {
            "Eyes"
        } else {
            self.region.label()
        };
        match self.detail {
            Some(ModelDetail::Low) => format!("{what} · low poly"),
            _ => what.to_owned(),
        }
    }
}

/// How much of a texture's drawing lands in each region, and which models
/// draw it.
#[derive(Clone, Debug, Default)]
struct Tally {
    regions: HashMap<BodyRegion, f32>,
    high: bool,
    low: bool,
    other: bool,
    animated: bool,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        for (&region, &weight) in &other.regions {
            *self.regions.entry(region).or_default() += weight;
        }
        self.high |= other.high;
        self.low |= other.low;
        self.other |= other.other;
        self.animated |= other.animated;
    }

    fn place(&self) -> Option<TexturePlace> {
        let mut ranked: Vec<(BodyRegion, f32)> = self
            .regions
            .iter()
            .map(|(&region, &weight)| (region, weight))
            .collect();
        // Heaviest first; ties by region order, so the result is stable.
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let &(top, weight) = ranked.first()?;
        // A symmetric object (both feet, both hands) splits its weight
        // between the sides: name the pair when the other side carries at
        // least half as much.
        let region = match top.mirror() {
            Some((other, both))
                if self
                    .regions
                    .get(&other)
                    .is_some_and(|&mirror| mirror >= weight * 0.5) =>
            {
                both
            }
            _ => top,
        };
        let detail = match (self.high, self.low, self.other) {
            (true, false, false) => Some(ModelDetail::High),
            (false, true, false) => Some(ModelDetail::Low),
            _ => None,
        };
        let eyes = self.animated && matches!(region, BodyRegion::Head | BodyRegion::Body);
        Some(TexturePlace {
            region,
            detail,
            eyes,
        })
    }
}

/// The places of a stock costume's textures.
#[derive(Clone, Debug, Default)]
pub struct CostumePlaces {
    textures: HashMap<HsdTextureSourceId, Tally>,
}

impl CostumePlaces {
    /// Place the textures of a costume the catalog recognizes; `Ok(None)` for
    /// anything else. Needs `PlCo.dat` and the fighter's `PlXx.dat` in
    /// `store`.
    pub fn read(
        dat: &DatFile,
        scene: &HsdScene,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
    ) -> Result<Option<Self>> {
        let Some(costume) = catalog.recognize(scene) else {
            return Ok(None);
        };
        let parse = |bytes: Vec<u8>| {
            DatFile::parse(&bytes).map_err(|error| MeleeError::Places(error.to_string()))
        };
        let common = parse(catalog.load_common(store)?)?;
        let fighter = parse(catalog.load_fighter(store, costume.fighter_kind)?)?;
        let root = &scene.roots[costume.root_index];
        let slots = fighter_part_slots(&common, costume.fighter_kind, root)
            .map_err(|error| MeleeError::Places(format!("fighter parts: {error}")))?;
        let parts: HashMap<JObjId, u8> = slots
            .slots
            .iter()
            .zip(&slots.logical)
            .filter_map(|(joint, &part)| Some(((*joint)?, part)))
            .collect();
        let models = FighterModelParts::load(&fighter, costume.fighter_kind, costume.costume)
            .map_err(|error| MeleeError::Places(format!("model parts: {error}")))?;
        let mut places = Self::place(scene, root, &parts, &models);
        places.add_animations(dat, scene, costume.root_index);
        Ok(Some(places))
    }

    fn place(
        scene: &HsdScene,
        root: &HsdSceneRoot,
        parts: &HashMap<JObjId, u8>,
        models: &FighterModelParts,
    ) -> Self {
        let regions = joint_regions(root, parts);
        let index: HashMap<JObjId, usize> = root
            .joints
            .iter()
            .enumerate()
            .map(|(index, joint)| (joint.source_id, index))
            .collect();
        let in_table = |table: usize, ordinal: usize| {
            models.tables[table].as_ref().is_some_and(|table| {
                table
                    .iter()
                    .flatten()
                    .flatten()
                    .any(|&named| usize::from(named) == ordinal)
            })
        };

        let mut textures: HashMap<HsdTextureSourceId, Tally> = HashMap::new();
        // Display objects in `ftParts` ordinal order: preorder joints, then
        // each joint's DObj list.
        let objects = root.joints.iter().enumerate().flat_map(|(owner, joint)| {
            joint
                .display_objects
                .iter()
                .map(move |object| (owner, object))
        });
        for (ordinal, (owner, object)) in objects.enumerate() {
            let mut tally = Tally::default();
            let mut add = |joint: usize, weight: f32| {
                *tally.regions.entry(regions[joint]).or_default() += weight;
            };
            for polygon in &object.polygons {
                let vertices = &polygon.decoded.vertices;
                match &polygon.binding {
                    HsdPolygonBinding::Rigid { joint } => {
                        let joint = joint
                            .and_then(|joint| index.get(&joint).copied())
                            .unwrap_or(owner);
                        add(joint, vertices.len() as f32);
                    }
                    HsdPolygonBinding::Envelope { entries, .. } => {
                        for vertex in vertices {
                            let Some(envelope) = entries.get(usize::from(vertex.pn_mtx_idx) / 3)
                            else {
                                continue;
                            };
                            for weight in &envelope.weights {
                                if let Some(&joint) = index.get(&weight.joint) {
                                    add(joint, weight.weight);
                                }
                            }
                        }
                    }
                }
            }
            let (high, low) = (in_table(0, ordinal), in_table(1, ordinal));
            tally.high = high;
            tally.low = low;
            tally.other = !high && !low;
            let Some(material) = &object.material else {
                continue;
            };
            for texture in material.textures.iter().filter_map(|stage| stage.texture) {
                if let Some(texture) = scene.textures.get(texture.0) {
                    textures.entry(texture.id).or_default().add(&tally);
                }
            }
        }
        Self { textures }
    }

    /// Mark animated textures, and place each animation frame where the TObj
    /// it animates sits.
    fn add_animations(&mut self, dat: &DatFile, scene: &HsdScene, root_index: usize) {
        for animation in texture_animations(dat, scene) {
            if animation.root_index != root_index {
                continue;
            }
            let Some(base) = animation.base.and_then(|base| self.textures.get_mut(&base)) else {
                continue;
            };
            base.animated = true;
            let base = base.clone();
            for frame in animation.frames {
                self.textures.entry(frame).or_default().add(&base);
            }
        }
    }

    /// Where the texture drawn through `uses` (descriptor pairs that share
    /// its pixels) sits, or `None` when no display object draws it.
    pub fn place_of(&self, uses: &[HsdTextureSourceId]) -> Option<TexturePlace> {
        let mut tally = Tally::default();
        for id in uses {
            if let Some(found) = self.textures.get(id) {
                tally.add(found);
            }
        }
        tally.place()
    }
}

/// Each joint's region: its nearest landmark ancestor (itself included). A
/// fighter without a head part is one round body, so its hips are "Body".
fn joint_regions(root: &HsdSceneRoot, parts: &HashMap<JObjId, u8>) -> Vec<BodyRegion> {
    let landmark = |joint: usize| {
        parts
            .get(&root.joints[joint].source_id)
            .and_then(|&part| BodyRegion::of_part(part))
    };
    let has_head = (0..root.joints.len()).any(|joint| landmark(joint) == Some(BodyRegion::Head));
    (0..root.joints.len())
        .map(|joint| {
            let mut at = Some(joint);
            while let Some(current) = at {
                if let Some(region) = landmark(current) {
                    return match region {
                        BodyRegion::Hips if !has_head => BodyRegion::Body,
                        region => region,
                    };
                }
                at = root.joints[current].parent.map(|parent| parent.0);
            }
            BodyRegion::Body
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BodyRegion, ModelDetail, Tally, TexturePlace};

    fn tally(regions: &[(BodyRegion, f32)], high: bool, low: bool) -> Tally {
        Tally {
            regions: regions.iter().copied().collect(),
            high,
            low,
            other: !high && !low,
            animated: false,
        }
    }

    #[test]
    fn the_heaviest_region_names_a_texture() {
        let place = tally(
            &[(BodyRegion::Head, 40.0), (BodyRegion::Chest, 12.0)],
            true,
            false,
        )
        .place()
        .unwrap();
        assert_eq!(place.region, BodyRegion::Head);
        assert_eq!(place.detail, Some(ModelDetail::High));
        assert_eq!(place.label(), "Head");
    }

    #[test]
    fn mirrored_sides_merge_into_a_pair() {
        let feet = tally(
            &[
                (BodyRegion::LeftFoot, 132.0),
                (BodyRegion::RightFoot, 131.0),
            ],
            false,
            true,
        );
        assert_eq!(
            feet.place(),
            Some(TexturePlace {
                region: BodyRegion::Feet,
                detail: Some(ModelDetail::Low),
                eyes: false,
            })
        );
        assert_eq!(feet.place().unwrap().label(), "Feet · low poly");
        let lopsided = tally(
            &[(BodyRegion::LeftHand, 100.0), (BodyRegion::RightHand, 30.0)],
            true,
            false,
        );
        assert_eq!(lopsided.place().unwrap().region, BodyRegion::LeftHand);
    }

    #[test]
    fn animated_head_textures_are_eyes() {
        let mut eye = tally(&[(BodyRegion::Head, 50.0)], true, false);
        eye.animated = true;
        assert_eq!(eye.place().unwrap().label(), "Eyes");
        let mut hand = tally(&[(BodyRegion::LeftHand, 50.0)], true, false);
        hand.animated = true;
        assert_eq!(hand.place().unwrap().label(), "Left hand");
    }

    #[test]
    fn a_texture_both_models_draw_has_no_detail() {
        let mut both = tally(&[(BodyRegion::Hips, 5.0)], true, false);
        both.add(&tally(&[(BodyRegion::Hips, 5.0)], false, true));
        assert_eq!(both.place().unwrap().detail, None);
        assert_eq!(Tally::default().place(), None);
    }
}
