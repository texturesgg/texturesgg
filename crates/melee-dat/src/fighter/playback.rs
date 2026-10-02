//! Playback of a stock fighter's animations: the catalog's Wait1 idle on
//! attach, then any of the fighter's animations. No GPU is involved:
//! renderers draw what [`MeleeFighterPlayback::evaluate`] returns.

use crate::catalog::{IdleProfile, MeleeReferenceCatalog};
use crate::error::{MeleeError, Result};
use crate::fighter::animation::{
    AttachedFighterAnimation, FighterAnimationBindingError, FighterAnimationFiles,
    bind_nana_fighter_animation, bind_same_kind_fighter_animation, fighter_animation_actions,
};
use crate::fighter::moves::{MoveGroup, move_group, move_name};
use crate::fighter::parts::{FighterModelParts, default_selections, wait1_script_selections};
use crate::fighter::{CostumeIndex, FighterKind};
use crate::references::MeleeReferenceStore;
use dat_parser::DatFile;
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::draw::{HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork};
use dat_parser::hsd::scene::{DObjId, HsdJointIndex, HsdTransform, JObjId};
use dat_parser::hsd::source::HsdSource;

/// ftParts_80074194 caps the game's global display-object list at 124.
const MAX_FIGHTER_DISPLAY_OBJECTS: usize = 124;

/// What [`MeleeFighterPlayback::attach`] made of a model. A model that does
/// not attach comes back, so the caller can still draw its bind pose.
pub enum FighterAttach {
    /// The fighter's idle is playing.
    Attached(Box<MeleeFighterPlayback>),
    /// No catalog profile recognizes the model as a stock fighter's costume:
    /// its root is not one the catalog names, or its joints or display
    /// objects are not the fighter's. Nothing went wrong.
    Unrecognized(Box<HsdSource>),
    /// The catalog recognizes the costume and its idle did not attach.
    Failed {
        source: Box<HsdSource>,
        error: MeleeError,
    },
}

/// One of a fighter's animations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FighterAnimation {
    /// Its index in the fighter's animation table.
    pub index: usize,
    /// The action it plays (`AttackHi3`), from its symbol; `None` for a
    /// record with no animation.
    pub action: Option<String>,
    /// The name players use ("Up tilt"), when [`move_name`] knows it.
    pub name: Option<String>,
    /// Where a move list shows it.
    pub group: MoveGroup,
}

impl FighterAnimation {
    /// The player name, else the action, else the index.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.action.clone())
            .unwrap_or_else(|| format!("Animation {}", self.index))
    }
}

/// What binding any of a fighter's animations needs, kept after attaching.
struct AnimationInputs {
    fighter: DatFile,
    common: DatFile,
    /// The fighter's (or, for Nana, Popo's) animation archive.
    aj: Vec<u8>,
    /// Popo's data when the fighter is Nana, whose motions play Popo's
    /// records (ftData_80085FD4).
    record_fighter: Option<DatFile>,
    fighter_kind: FighterKind,
    animation_count: usize,
    record_count: usize,
    root_index: usize,
    /// Every bound animation drives this many parts.
    receivers: usize,
    fighter_label: String,
    /// The catalog's verified idle, which must still match on every attach.
    idle: usize,
    idle_flags: u32,
    idle_end_frame: f32,
}

impl AnimationInputs {
    /// Bind animation `index` over the costume's parts, as ChangeMotionState
    /// would; the idle must match the catalog's verified metadata.
    fn bind(&self, source: &HsdSource, index: usize) -> Result<AttachedFighterAnimation> {
        let files = FighterAnimationFiles {
            fighter: &self.fighter,
            common: &self.common,
            aj: &self.aj,
            root: &source.scene.roots[self.root_index],
            animation_count: self.animation_count,
        };
        let binding = match &self.record_fighter {
            None => bind_same_kind_fighter_animation(&files, self.fighter_kind, index),
            Some(record_fighter) => {
                bind_nana_fighter_animation(&files, record_fighter, self.record_count, index)
            }
        }
        .map_err(|source| MeleeError::Unplayable {
            reason: unplayable(&source),
            source,
        })?;
        let animation = AttachedFighterAnimation::from_binding(
            &source.scene,
            self.root_index,
            &self.aj,
            &binding,
        )?;
        if animation.receivers().len() != self.receivers
            || (index == self.idle
                && (animation.flags() != self.idle_flags
                    || animation.end_frame() != self.idle_end_frame))
        {
            return Err(self.mismatch("the animation's metadata"));
        }
        Ok(animation)
    }

    fn mismatch(&self, what: &'static str) -> MeleeError {
        MeleeError::ReferenceMismatch {
            fighter: self.fighter_label.clone(),
            what,
        }
    }
}

/// Why the binder refuses an animation, in the words a move list shows.
fn unplayable(error: &FighterAnimationBindingError) -> String {
    use FighterAnimationBindingError as Error;
    match error {
        Error::NullPointer("symbol") => "no animation in this slot".into(),
        Error::RemappedAnimation { .. } => "made for another fighter's skeleton".into(),
        Error::PartialAnimation(_) => "blends over part of the body, not supported yet".into(),
        Error::AuxiliaryEnabled(_) => "moves extra parts, not supported yet".into(),
        // Nana's own records need her own AJ; only Popo's is loaded.
        Error::NanaRecordPresent => "Nana's own animation, not supported yet".into(),
        other => other.to_string(),
    }
}

/// A stock fighter costume playing its fighter's animations: the catalog's
/// Wait1 idle when it attaches, then any animation the fighter has, at any
/// frame and speed. Animations the binder can't reproduce exactly (auxiliary
/// or partial parts) are refused rather than approximated.
pub struct MeleeFighterPlayback {
    source: HsdSource,
    inputs: AnimationInputs,
    animations: Vec<FighterAnimation>,
    current: usize,
    animation: AttachedFighterAnimation,
    label: String,
    scale_receiver: HsdJointIndex,
    scale_transform: HsdTransform,
    root_transform: HsdTransform,
    /// The animation frame shown, and how many frames each tick advances.
    frame: f32,
    rate: f32,
    tick: u64,
    loops: u64,
}

impl MeleeFighterPlayback {
    /// Play the catalog idle on a costume the catalog recognizes.
    pub fn attach(
        mut source: HsdSource,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
    ) -> FighterAttach {
        let failed = |source, error| FighterAttach::Failed {
            source: Box::new(source),
            error,
        };
        // Fighter animation needs the MeleeFighter policy, which selects
        // fighter envelope skinning.
        if source.policy != HsdDrawEvaluationPolicy::MELEE_FIGHTER {
            return failed(source, MeleeError::WrongPolicy);
        }
        let Some((profile, root_index, costume)) = Self::profile_for(&source.scene, catalog) else {
            return FighterAttach::Unrecognized(Box::new(source));
        };
        match Self::attach_profile(&mut source, catalog, store, profile, root_index, costume) {
            Ok(Some(parts)) => {
                let mut playback = Self {
                    source,
                    current: parts.inputs.idle,
                    inputs: parts.inputs,
                    animations: parts.animations,
                    animation: parts.animation,
                    label: parts.label,
                    scale_receiver: parts.scale_receiver,
                    scale_transform: parts.scale_transform,
                    root_transform: parts.root_transform,
                    frame: 0.0,
                    rate: 1.0,
                    tick: 0,
                    loops: 0,
                };
                match playback.reset() {
                    Ok(()) => FighterAttach::Attached(Box::new(playback)),
                    Err(error) => failed(playback.source, error),
                }
            }
            Ok(None) => FighterAttach::Unrecognized(Box::new(source)),
            Err(error) => failed(source, error),
        }
    }

    /// The profile whose costume root the scene holds, that root's index, and
    /// the costume ID. The catalog lists each fighter's costume roots in
    /// costume-ID order. A root that follows the fighter's naming but is not
    /// a vanilla costume (an expansion slot such as `PlyPurin5KWh`) plays as
    /// costume 0, as `fighter.c:722` clamps an out-of-range costume ID;
    /// topology and table bounds are still checked before anything applies.
    pub(crate) fn profile_for<'c>(
        contract: &HsdScene,
        catalog: &'c MeleeReferenceCatalog,
    ) -> Option<(&'c IdleProfile, usize, CostumeIndex)> {
        catalog.profile_for_roots(contract.roots.iter().map(|root| root.name.as_deref()))
    }

    fn attach_profile(
        source: &mut HsdSource,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
        profile: &IdleProfile,
        root_index: usize,
        costume: CostumeIndex,
    ) -> Result<Option<AttachParts>> {
        let root = &source.scene.roots[root_index];
        let initialization = &profile.initialization;
        if root.joints.first().map(|joint| joint.source_id) != Some(root.source_id)
            || !initialization.has_hierarchy(&root.joints)
        {
            return Ok(None);
        }
        let entry = catalog.fighter(profile);
        let record = catalog.record_fighter(entry);
        let mismatch = |what| MeleeError::ReferenceMismatch {
            fighter: entry.label.clone(),
            what,
        };
        if entry.idle_animation != 2
            || !(initialization.model_scaling.is_finite() && initialization.model_scaling > 0.0)
            || initialization
                .root_scale
                .iter()
                .any(|scale| !(scale.is_finite() && *scale > 0.0))
        {
            return Err(mismatch("the catalog's idle initialization"));
        }
        // ftParts enumerates preorder JObjs followed by each linked DObj list;
        // only global ordinals drive selection, and appended objects stay
        // visible.
        let objects: Vec<u32> = root
            .joints
            .iter()
            .flat_map(|joint| &joint.display_objects)
            .map(|object| object.source_id.0)
            .collect();
        if objects.len() > MAX_FIGHTER_DISPLAY_OBJECTS {
            return Ok(None);
        }

        let fighter = store.load_dat(catalog.data_asset(entry))?;
        // Model-part visibility comes from the fighter data, as the game reads
        // it, and applies before animation binding so a costume whose idle
        // cannot attach still renders only the parts the game draws.
        let hidden = hidden_display_objects(&fighter, entry.kind, costume, &objects)?;
        if !hidden.is_empty() {
            source
                .evaluator
                .set_hidden_display_objects(root_index, &hidden)?;
        }
        let root = &source.scene.roots[root_index];

        let aj = store.load(catalog.animations_asset(record))?;
        let common = store.load_dat(catalog.common_asset())?;
        // Nana's unresolved motions play Popo's record and AJ (ftData_80085FD4).
        let record_fighter = if record.kind == entry.kind {
            None
        } else {
            Some(store.load_dat(catalog.data_asset(record))?)
        };
        let actions = fighter_animation_actions(
            record_fighter.as_ref().unwrap_or(&fighter),
            record.animation_count.min(entry.animation_count),
        )
        .map_err(MeleeError::AnimationTable)?;
        let animations = actions
            .into_iter()
            .enumerate()
            .map(|(index, action)| FighterAnimation {
                index,
                name: action.as_deref().and_then(move_name),
                group: action.as_deref().map_or(MoveGroup::Other, move_group),
                action,
            })
            .collect();
        let inputs = AnimationInputs {
            fighter,
            common,
            aj,
            record_fighter,
            fighter_kind: entry.kind,
            animation_count: entry.animation_count,
            record_count: record.animation_count,
            root_index,
            receivers: initialization.joint_parents.len(),
            fighter_label: entry.label.clone(),
            idle: entry.idle_animation,
            idle_flags: profile.idle.flags,
            idle_end_frame: profile.idle.end_frame,
        };
        let animation = inputs.bind(source, inputs.idle)?;
        let scale_receiver_id = *animation
            .receivers()
            .get(initialization.scale_receiver_index)
            .ok_or_else(|| mismatch("the idle's scale receiver"))?;
        let scale_receiver = joint_index(source, root_index, scale_receiver_id)
            .ok_or_else(|| mismatch("the scale receiver's joint"))?;

        let receiver = &root.joints[scale_receiver.0].local;
        let reciprocal = (1.0 / initialization.model_scaling) as f32;
        Ok(Some(AttachParts {
            label: format!("{} Wait1", entry.label),
            inputs,
            animations,
            // ftCommon_8007F6A4 SETS this part's scale; keep the costume's
            // rotation and translation.
            scale_transform: HsdTransform {
                scale: [reciprocal; 3],
                rotation: receiver.rotation,
                translation: receiver.translation,
            },
            // Fighter_UpdateModelScale, then ChangeMotionState's explicit
            // right-facing (+1) Euler. Keep in-place costume translation.
            root_transform: HsdTransform {
                scale: initialization.root_scale.map(|scale| scale as f32),
                rotation: [0.0, std::f32::consts::FRAC_PI_2, 0.0],
                translation: root.joints[0].local.translation,
            },
            scale_receiver,
            animation,
        }))
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Which fighter this is.
    pub fn fighter(&self) -> FighterKind {
        self.inputs.fighter_kind
    }

    /// The fighter's name ("Falco").
    pub fn fighter_name(&self) -> &str {
        &self.inputs.fighter_label
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn loops(&self) -> u64 {
        self.loops
    }

    pub fn scene(&self) -> &HsdScene {
        &self.source.scene
    }

    /// Every animation the fighter has, in table order.
    pub fn animations(&self) -> &[FighterAnimation] {
        &self.animations
    }

    /// The index of the animation playing.
    pub fn current(&self) -> usize {
        self.current
    }

    /// Whether animation `index` can play, binding it without playing it; a
    /// refusal the binder explains is [`MeleeError::Unplayable`].
    pub fn check(&self, index: usize) -> Result<()> {
        self.inputs.bind(&self.source, index).map(drop)
    }

    /// Play animation `index` from its first frame at the current speed. A
    /// refused animation leaves the current one playing.
    pub fn play(&mut self, index: usize) -> Result<()> {
        let animation = self.inputs.bind(&self.source, index)?;
        self.animation = animation;
        self.current = index;
        // The action, as the idle's label names Wait1.
        let action = self
            .animations
            .get(index)
            .and_then(|animation| animation.action.clone())
            .unwrap_or_else(|| format!("animation {index}"));
        self.label = format!("{} {action}", self.inputs.fighter_label);
        self.reset()
    }

    /// The frame shown, from 0 to [`end_frame`](Self::end_frame).
    pub fn frame(&self) -> f32 {
        self.frame
    }

    /// The animation's length in frames.
    pub fn end_frame(&self) -> f32 {
        self.animation.end_frame()
    }

    /// Show `frame`, clamped to the animation.
    pub fn seek(&mut self, frame: f32) -> Result<()> {
        // `clamp` panics on a negative upper bound.
        self.reset_native(frame.clamp(0.0, self.animation.end_frame().max(0.0)))
    }

    /// Frames advanced per tick: 1 plays at the game's speed.
    pub fn rate(&self) -> f32 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        if !(rate.is_finite() && rate > 0.0) {
            return Err(MeleeError::InvalidRate(rate));
        }
        self.rate = rate;
        Ok(self.animation.pose_mut().set_rate(rate)?)
    }

    fn reset_native(&mut self, frame: f32) -> Result<()> {
        // Re-requesting resets the native FObj interpreters without reparsing.
        let pose = self.animation.pose_mut();
        pose.set_rate(self.rate)?;
        pose.request(frame)?;
        pose.set_local_transform(self.scale_receiver, self.scale_transform)?;
        pose.advance()?;
        pose.set_local_transform(HsdJointIndex(0), self.root_transform)?;
        if pose.is_stopped() && frame == 0.0 {
            return Err(MeleeError::StoppedAtReset {
                label: self.label.clone(),
            });
        }
        self.frame = frame;
        Ok(())
    }

    /// Advance one 60 Hz tick, restarting at the end of each cycle.
    pub fn advance(&mut self) -> Result<()> {
        self.animation.pose_mut().advance()?;
        self.frame += self.rate;
        if self.animation.pose().is_stopped() {
            self.reset_native(0.0)?;
            self.loops += 1;
        }
        self.tick += 1;
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        self.reset_native(0.0)?;
        self.tick = 0;
        self.loops = 0;
        Ok(())
    }

    /// Evaluate the current pose; returns the scene with its draw work.
    pub fn evaluate(&mut self) -> Result<(&HsdScene, &HsdEvaluatedDrawWork)> {
        let pose = self.animation.pose().pose()?;
        let work = self
            .source
            .evaluator
            .evaluate(&self.source.scene, &[pose])?;
        Ok((&self.source.scene, work))
    }
}

/// Display objects the game hides in the fighter's normal Wait1 main pass,
/// derived from the fighter data exactly as `ftparts.c` applies it (see
/// [`FighterModelParts`]).
pub(crate) fn hidden_display_objects(
    fighter: &DatFile,
    fighter_kind: FighterKind,
    costume: CostumeIndex,
    objects: &[u32],
) -> Result<Vec<DObjId>> {
    let parts = FighterModelParts::load(fighter, fighter_kind, costume)?;
    let mut selections = default_selections(fighter_kind, costume, parts.model_count());
    for &(group, value) in wait1_script_selections(fighter_kind) {
        if let Some(selection) = selections.get_mut(group) {
            *selection = value;
        }
    }
    let visible = parts.main_pass_visibility(&selections, objects.len())?;
    Ok(objects
        .iter()
        .zip(visible)
        .filter(|(_, visible)| !visible)
        .map(|(id, _)| DObjId(*id))
        .collect())
}

pub(crate) struct AttachParts {
    label: String,
    inputs: AnimationInputs,
    animations: Vec<FighterAnimation>,
    scale_transform: HsdTransform,
    root_transform: HsdTransform,
    scale_receiver: HsdJointIndex,
    animation: AttachedFighterAnimation,
}

/// The one joint of the root with identity `id`; `None` when it is absent
/// or more than one joint has it.
fn joint_index(source: &HsdSource, root_index: usize, id: JObjId) -> Option<HsdJointIndex> {
    let mut matches = source.scene.roots[root_index]
        .joints
        .iter()
        .enumerate()
        .filter(|(_, joint)| joint.source_id == id);
    let (index, _) = matches.next()?;
    matches.next().is_none().then_some(HsdJointIndex(index))
}
