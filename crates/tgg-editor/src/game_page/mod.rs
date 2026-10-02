//! Melee: what's in the player's game. It opens on the roster, a grid
//! of fighters (or stages) showing what each has installed; opening one
//! shows it moving on the stage, with the Costumes pane floating at the
//! right, as the editor's panes do: its slots, and what the selected one can
//! do. Edit textures sits in the top row.

mod view;

use crate::References;
use crate::costumes::stage_name;
use crate::costumes::{CostumesEvent, Fighter, Notice};
use crate::install::SlotState;
use crate::library::Skin;
use crate::open_file::OpenFile;
use crate::page::GameChip;
use crate::timeline::Timeline;
use gpui::{AppContext, Context, Entity, EventEmitter, RenderImage, SharedString};
use melee_dat::Stage;
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
    /// The versus stages' files the game has.
    stages: Vec<&'static str>,
    tab: Tab,
    fighter: usize,
    stage: usize,
    /// Showing one fighter or stage, rather than the roster.
    open: bool,
    /// The selected slot of each fighter, by fighter code.
    slots: HashMap<&'static str, String>,
    pub skins: Vec<Skin>,
    pub states: HashMap<String, SlotState>,
    pub undoable: HashSet<String>,
    /// Each slot's render, by file, as they arrive.
    pub renders: HashMap<String, Arc<RenderImage>>,
    preview: Option<Preview>,
    /// Why the selected file couldn't be previewed.
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
        states: HashMap<String, SlotState>,
        undoable: HashSet<String>,
        renders: HashMap<String, Arc<RenderImage>>,
        split: Entity<SplitState>,
    ) -> Self {
        let files = references.game().file_names();
        let stages = Stage::all()
            .map(Stage::file_name)
            .filter(|file| files.iter().any(|name| name == file))
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
            skins,
            states,
            undoable,
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
            Tab::Fighters => self.fighters.get(self.fighter).map(|f| f.name.to_owned()),
            Tab::Stages => self
                .stages
                .get(self.stage)
                .map(|file| stage_name(file).to_owned()),
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

    /// Show the fighter or stage with `file` among its slots, with that slot
    /// selected. Returns whether the game has it.
    pub fn show_file(&mut self, file: &str, cx: &mut Context<Self>) -> bool {
        if let Some(index) = self
            .fighters
            .iter()
            .position(|fighter| fighter.costumes.iter().any(|costume| costume.file == file))
        {
            self.tab = Tab::Fighters;
            self.fighter = index;
            self.slots
                .insert(self.fighters[index].code, file.to_owned());
        } else if let Some(index) = self.stages.iter().position(|stage| *stage == file) {
            self.tab = Tab::Stages;
            self.stage = index;
        } else {
            return false;
        }
        self.open = true;
        self.show_selected(cx);
        true
    }

    /// The file on show: the selected fighter's selected slot, or the stage.
    fn selected_file(&self) -> Option<String> {
        match self.tab {
            Tab::Fighters => {
                let fighter = self.fighters.get(self.fighter)?;
                Some(self.shown_slot(fighter)?.to_owned())
            }
            Tab::Stages => self.stages.get(self.stage).map(|file| (*file).to_owned()),
        }
    }

    /// The slot a fighter shows: the one last selected, or its first.
    fn shown_slot<'a>(&'a self, fighter: &'a Fighter) -> Option<&'a str> {
        self.slots
            .get(fighter.code)
            .map(String::as_str)
            .or_else(|| {
                fighter
                    .costumes
                    .first()
                    .map(|costume| costume.file.as_str())
            })
    }

    /// Put the selected file in the preview, unless it already is.
    fn show_selected(&mut self, cx: &mut Context<Self>) {
        let Some(file) = self.selected_file() else {
            return;
        };
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.file.name == file)
        {
            return;
        }
        self.load(file, cx);
    }

    /// Load `file` from the game into the preview (again, after it changed).
    fn load(&mut self, file: String, cx: &mut Context<Self>) {
        let references = self.references.clone();
        let shown = references
            .game()
            .read(&file)
            .and_then(|bytes| OpenFile::show(&file, bytes, Some(&references), cx));
        match shown {
            Ok(mut shown) => {
                shown.slot = Some(file);
                let timeline = cx.new(|cx| Timeline::new(shown.viewport.clone(), cx));
                self.preview = Some(Preview {
                    file: shown,
                    timeline,
                });
                self.problem = None;
            }
            Err(error) => {
                self.preview = None;
                self.problem = Some(format!("Couldn't show {file}: {error}").into());
            }
        }
        cx.notify();
    }

    /// The preview of `file`, for the editor to take over as it is.
    pub fn handoff(&self, file: &str) -> Option<OpenFile> {
        let preview = self.preview.as_ref()?;
        (preview.file.name == file).then(|| preview.file.clone())
    }

    /// Take the preview back from the editor: load the file afresh (leaving
    /// behind any unsaved edits and the selection tint), looking and moving
    /// as the editor's view did.
    pub fn back_from_edit(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = &self.preview else {
            return;
        };
        let state = preview.file.viewport.read(cx).view_state();
        let file = preview.file.name.clone();
        self.load(file, cx);
        if let Some(preview) = &self.preview {
            preview
                .file
                .viewport
                .update(cx, |viewport, cx| viewport.apply_view_state(state, cx));
        }
    }

    /// Show `file` afresh after its slot changed.
    pub fn reload(&mut self, file: &str, cx: &mut Context<Self>) {
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.file.name == file)
        {
            self.load(file.to_owned(), cx);
        }
    }

    fn changed(&self, file: &str) -> bool {
        !matches!(self.states.get(file), None | Some(SlotState::Vanilla))
    }

    /// The file on show, when a fighter or stage is open: what Edit
    /// textures opens.
    pub fn editable(&self) -> Option<String> {
        self.open.then(|| self.selected_file()).flatten()
    }
}
