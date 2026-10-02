//! Playback of a stock fighter's animations: the catalog's Wait1 idle on
//! attach, then any of the fighter's animations. No GPU is involved:
//! renderers draw what [`MeleeFighterPlayback::evaluate`] returns.

use crate::catalog::{IdleProfile, MeleeReferenceCatalog};
use crate::error::{MeleeError, Result, playback_error};
use crate::fighter::animation::{
    AttachedFighterAnimation, FighterAnimationBindingError, bind_nana_fighter_animation,
    bind_same_kind_fighter_animation, fighter_animation_actions,
};
use crate::fighter::moves::{MoveGroup, move_group, move_name};
use crate::fighter::parts::{
    FighterModelParts, ModelPartsError, default_selections, wait1_script_selections,
};
use crate::references::MeleeReferenceStore;
use dat_parser::DatFile;
use dat_parser::hsd::HsdScene;
use dat_parser::hsd::draw::{HsdDrawEvaluationPolicy, HsdEvaluatedDrawWork};
use dat_parser::hsd::scene::{DObjId, HsdJointIndex, HsdTransform, JObjId};
use dat_parser::hsd::source::{HsdSource, HsdSourceError};

/// ftParts_80074194 caps the game's global display-object list at 124.
const MAX_FIGHTER_DISPLAY_OBJECTS: usize = 124;

/// A costume the catalog did not admit (`error` is `None`) or failed to attach;
/// the source comes back so the caller can still render its bind pose.
pub struct NotAttached {
    pub source: HsdSource,
    pub error: Option<MeleeError>,
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
    fighter_kind: u8,
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
        let scene_root = &source.scene.roots[self.root_index];
        let binding = match &self.record_fighter {
            None => bind_same_kind_fighter_animation(
                &self.fighter,
                &self.common,
                &self.aj,
                scene_root,
                self.fighter_kind,
                index,
                self.animation_count,
            ),
            Some(record_fighter) => bind_nana_fighter_animation(
                &self.fighter,
                record_fighter,
                &self.common,
                &self.aj,
                scene_root,
                index,
                self.animation_count,
                self.record_count,
            ),
        }
        .map_err(|error| MeleeError::Unplayable(unplayable(&error)))?;
        let animation = AttachedFighterAnimation::from_binding(
            &source.scene,
            self.root_index,
            &self.aj,
            &binding,
        )
        .map_err(|error| playback_error(error.to_string()))?;
        if animation.receivers.len() != self.receivers
            || (index == self.idle
                && (animation.flags != self.idle_flags
                    || animation.end_frame != self.idle_end_frame))
        {
            return Err(playback_error(format!(
                "native {} metadata differs from the verified reference",
                self.fighter_label
            )));
        }
        Ok(animation)
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
    /// Attach the catalog idle for the costume's recognized root, or return
    /// the source unchanged when no profile admits it (it then renders its
    /// bind pose, like the site).
    pub fn attach(
        mut source: HsdSource,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
    ) -> std::result::Result<Self, Box<NotAttached>> {
        let not_attached = |source, error| Box::new(NotAttached { source, error });
        // The site attaches idle only to MeleeFighter-policy evaluators; the
        // policy selects fighter envelope skinning.
        if source.policy != HsdDrawEvaluationPolicy::MELEE_FIGHTER {
            return Err(not_attached(
                source,
                Some(playback_error(
                    "fighter playback requires the MeleeFighter evaluation policy",
                )),
            ));
        }
        match Self::profile_for(&source.scene, catalog) {
            None => Err(not_attached(source, None)),
            Some((profile, root_index, costume)) => {
                match Self::attach_profile(
                    &mut source,
                    catalog,
                    store,
                    profile,
                    root_index,
                    costume,
                ) {
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
                            Ok(()) => Ok(playback),
                            Err(error) => Err(not_attached(playback.source, Some(error))),
                        }
                    }
                    Ok(None) => Err(not_attached(source, None)),
                    Err(error) => Err(not_attached(source, Some(error))),
                }
            }
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
    ) -> Option<(&'c IdleProfile, usize, usize)> {
        catalog.profile_for_roots(contract.roots.iter().map(|root| root.name.as_deref()))
    }

    fn attach_profile(
        source: &mut HsdSource,
        catalog: &MeleeReferenceCatalog,
        store: &MeleeReferenceStore,
        profile: &IdleProfile,
        root_index: usize,
        costume: usize,
    ) -> Result<Option<AttachParts>> {
        let root = &source.scene.roots[root_index];
        let initialization = &profile.initialization;
        if root.joints.first().map(|joint| joint.source_id) != Some(root.source_id)
            || !initialization.has_hierarchy(&root.joints)
        {
            return Ok(None);
        }
        let entry = catalog.fighter(profile.fighter_kind)?;
        let record_kind = entry
            .primary_idle
            .record_fighter_kind
            .unwrap_or(profile.fighter_kind);
        let record = catalog.fighter(record_kind)?;
        if entry.primary_idle.animation_index != 2
            || !(initialization.model_scaling.is_finite() && initialization.model_scaling > 0.0)
            || initialization
                .root_scale
                .iter()
                .any(|scale| !(scale.is_finite() && *scale > 0.0))
        {
            return Err(playback_error(
                "reference is not a source-verified native idle initialization",
            ));
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

        let parse =
            |bytes: &[u8]| DatFile::parse(bytes).map_err(|error| playback_error(error.to_string()));
        let fighter_bytes = store.load(catalog.asset(&entry.fighter_key)?)?;
        let fighter = parse(&fighter_bytes)?;
        // Model-part visibility comes from the fighter data, as the game reads
        // it, and applies before animation binding so a costume whose idle
        // cannot attach still renders only the parts the game draws.
        let hidden = hidden_display_objects(&fighter, profile.fighter_kind, costume, &objects)?;
        if !hidden.is_empty() {
            source
                .evaluator
                .set_hidden_display_objects(root_index, &hidden)
                .map_err(|error| playback_error(error.to_string()))?;
        }
        let root = &source.scene.roots[root_index];

        let common_bytes = store.load(catalog.asset(&catalog.common_key)?)?;
        let aj = store.load(catalog.asset(&record.animations_key)?)?;
        let common = parse(&common_bytes)?;
        // Nana's unresolved motions play Popo's record and AJ (ftData_80085FD4).
        let record_fighter = if record_kind == profile.fighter_kind {
            None
        } else {
            Some(parse(&store.load(catalog.asset(&record.fighter_key)?)?)?)
        };
        let actions = fighter_animation_actions(
            record_fighter.as_ref().unwrap_or(&fighter),
            record.animation_count.min(entry.animation_count),
        )
        .map_err(|error| playback_error(error.to_string()))?;
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
            fighter_kind: profile.fighter_kind,
            animation_count: entry.animation_count,
            record_count: record.animation_count,
            root_index,
            receivers: initialization.joint_parents.len(),
            fighter_label: entry.label.clone(),
            idle: entry.primary_idle.animation_index,
            idle_flags: profile.idle.flags,
            idle_end_frame: profile.idle.end_frame,
        };
        let animation = inputs.bind(source, inputs.idle)?;
        let scale_receiver_id = *animation
            .receivers
            .get(initialization.scale_receiver_index)
            .ok_or_else(|| playback_error("native idle scale receiver is missing"))?;
        let scale_receiver = joint_index(source, root_index, scale_receiver_id)?;

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

    /// The fighter's name ("Falco").
    pub fn fighter(&self) -> &str {
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
        self.animation.end_frame
    }

    /// Show `frame`, clamped to the animation.
    pub fn seek(&mut self, frame: f32) -> Result<()> {
        // `clamp` panics on a negative upper bound.
        self.reset_native(frame.clamp(0.0, self.animation.end_frame.max(0.0)))
    }

    /// Frames advanced per tick: 1 plays at the game's speed.
    pub fn rate(&self) -> f32 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: f32) -> Result<()> {
        if !(rate.is_finite() && rate > 0.0) {
            return Err(playback_error(format!(
                "playback rate {rate} must be positive"
            )));
        }
        self.rate = rate;
        self.animation
            .pose
            .set_rate(rate)
            .map_err(|error| playback_error(error.to_string()))
    }

    fn reset_native(&mut self, frame: f32) -> Result<()> {
        // Re-requesting resets the native FObj interpreters without reparsing.
        let pose = &mut self.animation.pose;
        let fail = |error: dat_parser::hsd::animation::HsdJointPoseError| {
            playback_error(error.to_string())
        };
        pose.set_rate(self.rate).map_err(fail)?;
        pose.request(frame).map_err(fail)?;
        pose.set_local_transform(self.scale_receiver, self.scale_transform)
            .map_err(fail)?;
        pose.advance().map_err(fail)?;
        pose.set_local_transform(HsdJointIndex(0), self.root_transform)
            .map_err(fail)?;
        if pose.is_stopped() && frame == 0.0 {
            return Err(playback_error(format!(
                "{} stopped at its reset frame",
                self.label
            )));
        }
        self.frame = frame;
        Ok(())
    }

    /// Advance one 60 Hz tick, restarting at the end of each cycle.
    pub fn advance(&mut self) -> Result<()> {
        self.animation
            .pose
            .advance()
            .map_err(|error| playback_error(error.to_string()))?;
        self.frame += self.rate;
        if self.animation.pose.is_stopped() {
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
        let pose = self
            .animation
            .pose
            .pose()
            .map_err(|error| playback_error(error.to_string()))?;
        let work = self
            .source
            .evaluator
            .evaluate(&self.source.scene, &[pose])
            .map_err(|error| HsdSourceError::InvalidDrawWork(error.to_string()))?;
        Ok((&self.source.scene, work))
    }
}

/// Display objects the game hides in the fighter's normal Wait1 main pass,
/// derived from the fighter data exactly as `ftparts.c` applies it (see
/// [`FighterModelParts`]).
pub(crate) fn hidden_display_objects(
    fighter: &DatFile,
    fighter_kind: u8,
    costume: usize,
    objects: &[u32],
) -> Result<Vec<DObjId>> {
    let fail = |error: ModelPartsError| playback_error(format!("model-part visibility: {error}"));
    let parts = FighterModelParts::load(fighter, fighter_kind, costume).map_err(fail)?;
    let mut selections = default_selections(fighter_kind, costume, parts.model_count);
    for &(group, value) in wait1_script_selections(fighter_kind) {
        if let Some(selection) = selections.get_mut(group) {
            *selection = value;
        }
    }
    let visible = parts
        .main_pass_visibility(&selections, objects.len())
        .map_err(fail)?;
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

fn joint_index(source: &HsdSource, root_index: usize, id: JObjId) -> Result<HsdJointIndex> {
    let mut matches = source.scene.roots[root_index]
        .joints
        .iter()
        .enumerate()
        .filter(|(_, joint)| joint.source_id == id);
    let (index, _) = matches
        .next()
        .ok_or_else(|| playback_error("joint is absent from the selected model root"))?;
    if matches.next().is_some() {
        return Err(playback_error("joint identity is ambiguous"));
    }
    Ok(HsdJointIndex(index))
}
