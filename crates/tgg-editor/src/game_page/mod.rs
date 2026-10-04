//! Melee: what's in the player's game. It opens on the roster, a grid
//! of fighters (or stages) showing what each has installed; opening one
//! shows it moving on the stage, with panes floating at the right as the
//! editor's do: Costumes, and Shared for what every costume plays with (the
//! fighter's data file). The pane holding the selected slot says what it can
//! do. Edit textures sits in the top row.

mod view;

use crate::References;
use crate::costumes::{CostumesEvent, Fighter, Notice, has_model};
use crate::install::SlotState;
use crate::library::Skin;
use crate::open_file::OpenFile;
use crate::page::GameChip;
use crate::renders::RenderKey;
use crate::timeline::Timeline;
use gpui::{AppContext, Context, Entity, EventEmitter, RenderImage, SharedString};
use melee_dat::{Character, MeleeSlot, SharedModel, Stage};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use tgg_ui::SplitState;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Fighters,
    Stages,
}

/// The costume or stage on show, with its live preview.
struct Preview {
    file: OpenFile,
    timeline: Entity<Timeline>,
}

pub(crate) struct GamePage {
    references: Rc<References>,
    /// The Costumes pane's width, shared with the editor's panes, so opening
    /// the editor moves nothing.
    split: Entity<SplitState>,
    pub fighters: Vec<Fighter>,
    /// The versus stages the game has.
    stages: Vec<Stage>,
    tab: Tab,
    fighter: usize,
    stage: usize,
    /// Showing one fighter or stage, rather than the roster.
    open: bool,
    /// The selected slot of each fighter.
    slots: HashMap<Character, MeleeSlot>,
    /// The shared model selected in each fighter's shared file, when that
    /// file is its selected slot.
    models: HashMap<Character, SharedModel>,
    pub skins: Vec<Skin>,
    pub states: HashMap<MeleeSlot, SlotState>,
    pub undoable: HashSet<MeleeSlot>,
    /// The shared models the game's files draw differently from vanilla.
    pub changed_models: HashSet<SharedModel>,
    /// Renders as they arrive; a slot's is under the slot.
    pub renders: HashMap<RenderKey, Arc<RenderImage>>,
    preview: Option<Preview>,
    /// Why the selected slot couldn't be previewed.
    problem: Option<SharedString>,
    pub notice: Option<Notice>,
    /// The game skins install into, beside the title.
    pub chip: Option<GameChip>,
}

impl EventEmitter<CostumesEvent> for GamePage {}

impl GamePage {
    pub fn new(
        references: Rc<References>,
        fighters: Vec<Fighter>,
        skins: Vec<Skin>,
        states: HashMap<MeleeSlot, SlotState>,
        undoable: HashSet<MeleeSlot>,
        renders: HashMap<RenderKey, Arc<RenderImage>>,
        split: Entity<SplitState>,
    ) -> Self {
        let stages = references
            .game()
            .slots()
            .into_iter()
            .filter_map(|slot| match slot {
                MeleeSlot::Stage(stage) => Some(stage),
                MeleeSlot::Costume { .. } | MeleeSlot::FighterData(_) | MeleeSlot::Effects(_) => {
                    None
                }
            })
            .collect();
        Self {
            references,
            split,
            fighters,
            stages,
            tab: Tab::Fighters,
            fighter: 0,
            stage: 0,
            open: false,
            slots: HashMap::new(),
            models: HashMap::new(),
            skins,
            states,
            undoable,
            changed_models: HashSet::new(),
            renders,
            preview: None,
            problem: None,
            notice: None,
            chip: None,
        }
    }

    /// The fighter or stage on show, by name; `None` on the roster.
    pub fn subject(&self) -> Option<String> {
        if !self.open {
            return None;
        }
        match self.tab {
            Tab::Fighters => self
                .fighters
                .get(self.fighter)
                .map(|fighter| fighter.character.name().to_owned()),
            Tab::Stages => self
                .stages
                .get(self.stage)
                .map(|stage| stage.name().to_owned()),
        }
    }

    /// Back to the roster, letting the preview go.
    pub fn show_roster(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.preview = None;
        self.problem = None;
        self.notice = None;
        cx.notify();
    }

    /// Show the fighter or stage that has `slot`, with that slot selected.
    /// Returns whether the game has it.
    pub fn show_slot(&mut self, slot: MeleeSlot, cx: &mut Context<Self>) -> bool {
        match slot {
            MeleeSlot::Costume { .. } | MeleeSlot::FighterData(_) | MeleeSlot::Effects(_) => {
                // A shared effects file shows under the fighter on show when
                // it's one of the file's, else the first.
                let has = |fighter: &Fighter| fighter.slots().any(|has| has == slot);
                let Some(index) = self
                    .fighters
                    .get(self.fighter)
                    .filter(|fighter| self.tab == Tab::Fighters && has(fighter))
                    .map(|_| self.fighter)
                    .or_else(|| self.fighters.iter().position(has))
                else {
                    return false;
                };
                self.tab = Tab::Fighters;
                self.fighter = index;
                self.slots.insert(self.fighters[index].character, slot);
            }
            MeleeSlot::Stage(stage) => {
                let Some(index) = self.stages.iter().position(|has| *has == stage) else {
                    return false;
                };
                self.tab = Tab::Stages;
                self.stage = index;
            }
        }
        self.open = true;
        self.show_selected(cx);
        true
    }

    /// The slot on show: the selected fighter's selected slot, or the stage.
    fn selected_slot(&self) -> Option<MeleeSlot> {
        match self.tab {
            Tab::Fighters => self.shown_slot(self.fighters.get(self.fighter)?),
            Tab::Stages => self.stages.get(self.stage).copied().map(MeleeSlot::Stage),
        }
    }

    /// The slot a fighter shows: the one last selected, or its first.
    fn shown_slot(&self, fighter: &Fighter) -> Option<MeleeSlot> {
        self.slots
            .get(&fighter.character)
            .copied()
            .or_else(|| fighter.slots().next())
    }

    /// The shared model selected for the fighter on show, when its selected
    /// slot is a shared file: the one last chosen in it, else its first.
    fn selected_model(&self) -> Option<SharedModel> {
        if self.tab != Tab::Fighters {
            return None;
        }
        let fighter = self.fighters.get(self.fighter)?;
        let file = self.selected_slot()?;
        self.models
            .get(&fighter.character)
            .copied()
            .filter(|model| model.slot() == file)
            .or_else(|| fighter.models(file).next())
    }

    /// The slot whose model shows for `fighter`'s `slot`: itself, or for a
    /// shared file, which is no costume, the fighter's costume on show, else
    /// its first.
    fn model_for(&self, fighter: &Fighter, slot: MeleeSlot) -> Option<MeleeSlot> {
        if has_model(slot) {
            return Some(slot);
        }
        let showing = self
            .preview
            .as_ref()
            .and_then(|preview| preview.file.slot)
            .filter(|shown| has_model(*shown) && shown.character() == Some(fighter.character));
        showing.or_else(|| {
            fighter
                .costumes
                .first()
                .map(|costume| costume.slot(fighter))
        })
    }

    /// Put the selected slot's model in the preview, unless it already is.
    fn show_selected(&mut self, cx: &mut Context<Self>) {
        let slot = match self.tab {
            Tab::Fighters => self.fighters.get(self.fighter).and_then(|fighter| {
                self.shown_slot(fighter)
                    .and_then(|slot| self.model_for(fighter, slot))
            }),
            Tab::Stages => self.selected_slot(),
        };
        let Some(slot) = slot else {
            return;
        };
        if self.previews(slot) {
            return;
        }
        self.load(slot, cx);
    }

    /// Whether the preview shows `slot`.
    fn previews(&self, slot: MeleeSlot) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|preview| preview.file.slot == Some(slot))
    }

    /// Load `slot` from the game into the preview (again, after it changed).
    fn load(&mut self, slot: MeleeSlot, cx: &mut Context<Self>) {
        let references = self.references.clone();
        let name = slot.file_name();
        let shown = references
            .game()
            .read_slot(slot)
            .and_then(|bytes| OpenFile::show(&name, bytes, Some(&references), cx));
        match shown {
            Ok(mut shown) => {
                shown.slot = Some(slot);
                let timeline = cx.new(|cx| Timeline::new(shown.viewport.clone(), cx));
                self.preview = Some(Preview {
                    file: shown,
                    timeline,
                });
                self.problem = None;
            }
            Err(error) => {
                self.preview = None;
                self.problem = Some(format!("Couldn't show {name}: {error}").into());
            }
        }
        cx.notify();
    }

    /// The preview of `slot`, for the editor to take over as it is.
    pub fn handoff(&self, slot: MeleeSlot) -> Option<OpenFile> {
        let preview = self.preview.as_ref()?;
        (preview.file.slot == Some(slot)).then(|| preview.file.clone())
    }

    /// Take the preview back from the editor: load the file afresh (leaving
    /// behind any unsaved edits and the selection tint), looking and moving
    /// as the editor's view did.
    pub fn back_from_edit(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = &self.preview else {
            return;
        };
        let state = preview.file.viewport.read(cx).view_state();
        let Some(slot) = preview.file.slot else {
            return;
        };
        self.load(slot, cx);
        if let Some(preview) = &self.preview {
            preview
                .file
                .viewport
                .update(cx, |viewport, cx| viewport.apply_view_state(state, cx));
        }
    }

    /// Show `slot` afresh after what it holds changed.
    pub fn reload(&mut self, slot: MeleeSlot, cx: &mut Context<Self>) {
        if self.previews(slot) {
            self.load(slot, cx);
        }
    }

    fn changed(&self, slot: MeleeSlot) -> bool {
        !matches!(self.states.get(&slot), None | Some(SlotState::Vanilla))
    }

    /// The slot on show, when a fighter or stage is open and it has a model:
    /// what Edit textures opens.
    pub fn editable(&self) -> Option<MeleeSlot> {
        self.open
            .then(|| self.selected_slot())
            .flatten()
            .filter(|slot| has_model(*slot))
    }
}
