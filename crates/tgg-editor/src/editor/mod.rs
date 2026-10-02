//! The editor: one costume open as a document. The 3D stage takes the
//! window; the tools float over it as panes the header toggles (the
//! textures, the selected texture, and the moves), and the header holds
//! Done, Save, and Save and install.

mod colors;
mod document;
pub(crate) mod external;
mod files;
#[cfg(test)]
mod flows;
mod panes;
mod view;

pub(crate) use panes::pane_split;
pub(crate) use view::dispatch;

use crate::References;
use crate::moves::Moves;
use crate::open_file::OpenFile;
use crate::settings::{self, PaneLayout, Settings};
use crate::timeline::Timeline;
use crate::viewport::Viewport;
use colors::{ColorGroup, ColorPick};
use dat_edit::{DocumentError, TextureDocument, Undone};
use document::Listing;
use external::ExternalEdits;
use gpui::{
    AppContext, Context, Entity, EventEmitter, SharedString, Subscription, Task, Window, actions,
};
use melee_dat::fighter::places::TexturePlace;
use melee_dat::{MeleeReferenceStore, MeleeSlot};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use tgg_ui::{Appearance, SplitState, Theme, ThumbnailItem};
use view::StatusLine;

actions!(
    editor,
    [
        OpenDat,
        Save,
        SaveAndInstall,
        SaveAs,
        ImportPng,
        ExportPng,
        EditExternally,
        Undo,
        Redo,
        TogglePlayback,
        ResetCamera,
        ToggleHighlight,
        ToggleTexturesPane,
        ToggleTexturePane,
        ToggleMovesPane
    ]
);

/// Key bindings, shared by the keymap and the hints in menus and tooltips.
/// `secondary` is Cmd on macOS and Ctrl elsewhere.
pub(crate) mod keys {
    pub const OPEN: &str = "secondary-o";
    pub const SAVE: &str = "secondary-s";
    pub const SAVE_AS: &str = "secondary-shift-s";
    pub const SAVE_AND_INSTALL: &str = "secondary-alt-s";
    pub const TOGGLE_SIDEBAR: &str = "secondary-\\";
    pub const QUIT: &str = "secondary-q";
    pub const SETTINGS: &str = "secondary-,";
    pub const EDIT_EXTERNALLY: &str = "secondary-shift-e";
    pub const UNDO: &str = "secondary-z";
    pub const REDO: &str = "secondary-shift-z";
    /// Bound only while the viewport has focus, so Space still presses a
    /// focused button.
    pub const PLAYBACK: &str = "space";
    pub const IMPORT: &str = "secondary-i";
    pub const EXPORT: &str = "secondary-e";
}

/// How long pane drags settle before their widths are written.
const SETTINGS_DEBOUNCE: Duration = Duration::from_millis(500);

/// A one-line report of the last action, such as an import's fidelity.
pub(crate) struct Notice {
    pub text: SharedString,
    pub error: bool,
}

/// What waits on the "Save changes?" dialog.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Pending {
    /// Close the window.
    Close,
    /// Go back to the player's costumes.
    Leave,
    /// Open this DAT in place of the current one.
    Open(PathBuf),
}

/// An answer to the "Save changes?" dialog.
#[derive(Clone, Copy)]
pub(crate) enum Unsaved {
    /// Keep the file and its edits open.
    Cancel,
    /// Continue without saving.
    Discard,
    /// Save, then continue.
    Save,
}

/// One of the editor's panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaneKind {
    Textures,
    Texture,
    Moves,
    Colors,
}

/// What the editor asks of the window around it.
pub(crate) enum EditorEvent {
    /// Go back to the player's costumes; any unsaved edits were let go.
    Leave,
    /// The player chose to keep editing rather than leave.
    Stay,
    /// Keep the document's bytes as a skin in the library, and with
    /// `install` put them into the slot the editor was opened from too, then
    /// carry out `then`.
    SaveToLibrary {
        bytes: Vec<u8>,
        install: bool,
        then: Option<Pending>,
    },
}

impl EventEmitter<EditorEvent> for Editor {}

pub(crate) struct Editor {
    /// The top bar keeps clear of the window buttons and the sidebar toggle,
    /// the sidebar being hidden.
    pub(crate) clear_toggle: bool,
    /// The game slot being edited; saving installs into it.
    pub(crate) slot: Option<MeleeSlot>,
    pub(crate) title: SharedString,
    /// The open file's name, suggested by save and export dialogs.
    pub(crate) name: String,
    /// Where the file was read from, and where Save writes.
    pub(crate) path: Option<PathBuf>,
    pub(crate) references: Option<Rc<References>>,
    /// The open costume's reference files, when it is a stock costume.
    store: Option<Rc<MeleeReferenceStore>>,
    pub(crate) document: Result<TextureDocument, DocumentError>,
    thumbnails: Rc<[ThumbnailItem]>,
    /// The fighter's moves, when the model plays them.
    pub(crate) moves: Moves,
    /// Which panes are open, and folded.
    pub(crate) panes: PaneLayout,
    /// Where a stock fighter draws each document texture, read from the
    /// reference data when the file opens.
    places: Vec<Option<TexturePlace>>,
    /// Each document texture's name: a stock stage's own name for the image
    /// ("BattleWall0"), a fighter's place and number ("Head #14"), or just
    /// the number.
    pub(crate) names: Vec<String>,
    /// The colors the file draws with outside its textures, by the textures
    /// they tint, and the one being edited.
    pub(crate) color_groups: Vec<ColorGroup>,
    pub(crate) color_pick: Option<ColorPick>,
    /// The texture being inspected; change it only through
    /// [`select`](Self::select), which keeps the viewport's tint and frame
    /// preview in step.
    selected: Option<usize>,
    split: Entity<SplitState>,
    pub(crate) viewport: Entity<Viewport>,
    status: Entity<StatusLine>,
    timeline: Entity<Timeline>,
    pub(crate) notice: Option<Notice>,
    /// An action held by the "Save changes?" dialog, shown while it's set.
    pub(crate) unsaved: Option<Pending>,
    pub(crate) settings: Settings,
    /// The settings as this editor last read or wrote them; what differs
    /// from `settings` is what it has yet to save.
    saved_settings: Settings,
    /// Textures open in another app, re-imported on each save there.
    pub(crate) external: ExternalEdits,
    /// The pending settings write; replacing it cancels the previous one.
    save_settings: Option<Task<()>>,
    /// Clicks in the current viewport.
    _picks: Subscription,
    /// The animation frame shown on the model in place of the image it
    /// stands in for.
    previewing: Option<usize>,
}

impl Editor {
    pub fn new(
        file: OpenFile,
        references: Option<Rc<References>>,
        settings: Settings,
        shared_split: Option<Entity<SplitState>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let OpenFile {
            name,
            title,
            bytes,
            path,
            slot,
            store,
            viewport,
        } = file;
        // Inside the app's workspace the pane column shares its width, which
        // the workspace keeps; on its own the editor keeps it.
        let split = shared_split.unwrap_or_else(|| {
            let split = cx.new(|_| pane_split(&settings));
            cx.observe(&split, |editor, split, cx| {
                editor.settings.inspector_width = Some(split.read(cx).right.width);
                editor.persist_settings(cx);
            })
            .detach();
            split
        });
        let panes = settings.panes.unwrap_or_default();
        let listing = Listing::read(bytes, references.as_deref(), store.as_deref());
        let picks = Self::subscribe_picks(&viewport, cx);
        let mut editor = Self {
            clear_toggle: false,
            slot,
            title: title.into(),
            name,
            path,
            references,
            store,
            document: listing.document,
            thumbnails: Rc::from([]),
            moves: Moves::default(),
            panes,
            places: listing.places,
            names: listing.names,
            color_groups: Vec::new(),
            color_pick: None,
            selected: None,
            split,
            status: cx.new(|cx| StatusLine::new(viewport.clone(), cx)),
            timeline: cx.new(|cx| Timeline::new(viewport.clone(), cx)),
            viewport,
            notice: None,
            unsaved: None,
            saved_settings: settings.clone(),
            settings,
            save_settings: None,
            _picks: picks,
            previewing: None,
            external: ExternalEdits::default(),
        };
        editor.read_moves(cx);
        editor.document_opened(cx);
        editor
    }

    /// List the moves of the viewport's fighter.
    fn read_moves(&mut self, cx: &mut Context<Self>) {
        self.moves = self
            .viewport
            .read(cx)
            .playback()
            .map(Moves::read)
            .unwrap_or_default();
    }

    /// Play the move in the move list's `row`.
    pub(crate) fn play_move(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(animation) = self.moves.animation(row) else {
            return;
        };
        if let Err(error) = self
            .viewport
            .update(cx, |viewport, cx| viewport.play(animation, cx))
        {
            self.set_notice(format!("Can't play that move: {error}"), true, cx);
        }
        cx.notify();
    }

    /// Whether there are costumes to go back to: a game is open.
    pub(crate) fn home(&self) -> bool {
        self.references.as_deref().map(References::game).is_some()
    }

    pub(crate) fn set_notice(
        &mut self,
        text: impl Into<SharedString>,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        let text = text.into();
        // Mirrored to stderr so scripted runs can read the result.
        crate::log(&format!("notice: {text}"));
        self.notice = Some(Notice { text, error });
        cx.notify();
    }

    /// Write this editor's changed settings once changes settle. Only what
    /// it changed is written: the shell saves settings too, and this copy
    /// of the rest is as old as the editor.
    fn persist_settings(&mut self, cx: &mut Context<Self>) {
        let before = self.saved_settings.clone();
        let after = self.settings.clone();
        self.save_settings = Some(cx.spawn(async move |editor, cx| {
            cx.background_executor().timer(SETTINGS_DEBOUNCE).await;
            settings::update(|saved| saved.apply_changes(&before, &after));
            let _ = editor.update(cx, |editor, _| editor.saved_settings = after);
        }));
    }

    fn set_appearance(
        &mut self,
        appearance: Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        Theme::init(appearance, cx);
        window.refresh();
        self.settings.set_appearance(appearance);
        self.persist_settings(cx);
    }

    pub(crate) fn is_modified(&self) -> bool {
        self.document
            .as_ref()
            .is_ok_and(|document| document.is_modified())
    }

    /// Undo or redo the latest edit, selecting the texture it touched.
    pub(crate) fn step_history(&mut self, redo: bool, cx: &mut Context<Self>) {
        let Ok(document) = &mut self.document else {
            return;
        };
        let (verb, step) = if redo {
            ("redo", document.redo())
        } else {
            ("undo", document.undo())
        };
        match step {
            None => self.set_notice(format!("Nothing to {verb}."), false, cx),
            Some(Ok(Undone::Colors)) => {
                self.read_colors();
                self.reload_model(cx);
                let done = if redo { "Redid" } else { "Undid" };
                self.set_notice(format!("{done} a color change"), false, cx);
            }
            Some(Ok(Undone::Texture(restored))) => {
                let texture = restored.texture.0;
                self.show_pixels(texture, &restored.decoded, cx);
                self.resync_external(texture, cx);
                self.select(Some(texture), cx);
                let done = if redo { "Redid" } else { "Undid" };
                self.set_notice(
                    format!("{done} the edit to {}", self.texture_name(texture)),
                    false,
                    cx,
                );
            }
            Some(Err(error)) => self.set_notice(format!("Couldn't {verb}: {error}"), true, cx),
        }
    }
}
