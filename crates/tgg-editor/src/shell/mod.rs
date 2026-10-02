//! The window's root: the welcome screen until the player's game is known;
//! then the collection (the sidebar of places and recent edits beside the
//! place: their game, their library, or settings) and the workbench (the
//! editor over the costume they open, the sidebar out of its way). The
//! sidebar toggle stays beside the window buttons throughout.

mod skins;
mod view;

use crate::Error;
use crate::costumes::{CostumesEvent, Notice, roster};
use crate::editor::pane_split;
use crate::editor::{Editor, EditorEvent, Pending};
use crate::game::{Game, GameChoice};
use crate::game_page::GamePage;
use crate::library::Library;
use crate::library_page::{LibraryEvent, LibraryPage};
use crate::open_file::OpenFile;
use crate::renders::Renders;
use crate::report::{Report, ReportEvent};
use crate::review::Review;
use crate::settings_page::{SettingsEvent, SettingsPage};
use crate::stress_test::StressTest;
use crate::welcome::{Status, Welcome, WelcomeEvent};
use crate::{Loaded, References, discover_games, game_references, remember_game, settings, update};
use gpui::RenderImage;
use gpui::Task;
use gpui::{AppContext, Context, Entity, FocusHandle, Subscription, Window};
use skins::{Change, slot_files, slot_states};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use tgg_ui::drop_images;
use tgg_ui::{SplitState, Theme};

/// A costume to open straight into the editor.
pub struct Costume {
    pub loaded: Loaded,
    /// Its file name, suggested by save and export dialogs.
    pub name: String,
    pub dat: Vec<u8>,
    /// Where it was read from on disk; `None` for a file in the game.
    pub path: Option<PathBuf>,
}

/// Where the app starts.
pub enum Start {
    Costume(Costume),
    /// The player's costumes when their game is known; otherwise the
    /// welcome screen, saying why a given ISO can't be used (`problem`).
    Home {
        problem: Option<String>,
    },
    /// Settings' stress test, on the player's game.
    StressTest,
}

/// The places the app's frame shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Game,
    Library,
}

enum Screen {
    Welcome(Entity<Welcome>),
    Game(Entity<GamePage>),
    Library(Entity<LibraryPage>),
    /// Settings' stress test.
    StressTest(Entity<StressTest>),
    Editor(Entity<Editor>),
}

pub(crate) struct Shell {
    screen: Screen,
    /// The place the frame shows, and returns to from the editor.
    place: Place,
    references: Option<Rc<References>>,
    /// The Melee ISOs in the player's game folders, for switching games.
    games: Vec<GameChoice>,
    /// The player's skins, shared by every screen.
    library: Rc<RefCell<Library>>,
    _screen_events: Subscription,
    /// Files being checked before they join the library.
    review: Option<(Entity<Review>, Subscription)>,
    /// Settings, open over the screen.
    settings: Option<(Entity<SettingsPage>, Subscription)>,
    /// A newer version of the app, once the update check finds one.
    update: Option<String>,
    /// A problem report, open over the screen.
    report: Option<(Entity<Report>, Subscription)>,
    /// The window's root: focused whenever nothing else is, so keys and
    /// menu commands always reach the app's handlers.
    focus: FocusHandle,
    /// The costume render thread, and the renders of the current game's
    /// slots by file.
    renders: Renders,
    images: HashMap<String, Arc<RenderImage>>,
    /// The editor's pane column width, kept between costumes.
    split: Entity<SplitState>,
    save_split: Option<Task<()>>,
    /// Your game, kept while the editor has taken over its preview.
    editing_from: Option<Entity<GamePage>>,
    /// Where to go once the editor lets go, when left through the sidebar.
    after_leave: Option<Leave>,
    /// The sidebar shows beside the player's places.
    sidebar: bool,
    /// The sidebar shows over the editor, which hides it until asked.
    sidebar_editing: bool,
    /// The game's costume files edited lately, the latest first.
    recent: Vec<String>,
}

impl Shell {
    pub fn new(
        start: Start,
        references: Option<References>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let references = references.map(Rc::new);
        let has_game = references.is_some();
        let (welcome, events) = Self::welcome(Status::Missing, true, window, cx);
        let saved = settings::load();
        let mut shell = Self {
            screen: Screen::Welcome(welcome),
            place: Place::Game,
            references,
            games: Vec::new(),
            library: Rc::new(RefCell::new(Library::open())),
            _screen_events: events,
            review: None,
            settings: None,
            report: None,
            update: None,
            focus: cx.focus_handle(),
            renders: Renders::start(),
            images: HashMap::new(),
            split: shared_split(cx),
            save_split: None,
            editing_from: None,
            after_leave: None,
            sidebar: saved.sidebar.unwrap_or(true),
            sidebar_editing: false,
            recent: saved.recent,
        };
        cx.observe(&shell.split, |shell, split, cx| {
            let width = split.read(cx).right.width;
            // Write once a drag settles.
            shell.save_split = Some(cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                settings::update(|settings| settings.inspector_width = Some(width));
            }));
        })
        .detach();
        shell.receive_renders(cx);
        // With nothing focused, keys and menu commands would reach no
        // handler at all: start at the root, and come back to it whenever
        // the focused element goes away (a card, when its page changes).
        window.focus(&shell.focus, cx);
        cx.on_focus_lost(window, |shell, window, cx| window.focus(&shell.focus, cx))
            .detach();
        match start {
            Start::Costume(costume) => {
                let file = OpenFile::of(costume, cx);
                let (editor, events) = shell.editor(file, window, cx);
                shell.screen = Screen::Editor(editor);
                shell._screen_events = events;
            }
            // A known game opens straight to its costumes.
            Start::Home { problem: None } if has_game => shell.show(Place::Game, window, cx),
            Start::StressTest if has_game => {
                shell.show(Place::Game, window, cx);
                shell.stress_test(cx);
            }
            Start::StressTest => {
                let (welcome, events) = Self::welcome(games_found(), true, window, cx);
                shell.screen = Screen::Welcome(welcome);
                shell._screen_events = events;
            }
            Start::Home { problem } => {
                let status = match problem {
                    Some(problem) => Status::Problem(problem.to_string().into()),
                    None => games_found(),
                };
                let (welcome, events) = Self::welcome(status, true, window, cx);
                shell.screen = Screen::Welcome(welcome);
                shell._screen_events = events;
            }
        }
        shell
    }

    fn game(&self) -> Option<&Game> {
        self.references.as_deref().map(References::game)
    }

    fn welcome(
        status: Status,
        intro: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<Welcome>, Subscription) {
        let welcome = cx.new(|_| Welcome { intro, status });
        let events = cx.subscribe_in(
            &welcome,
            window,
            |shell, _, event, window, cx| match event {
                WelcomeEvent::Use(path) => shell.use_game_at(path.clone(), window, cx),
            },
        );
        (welcome, events)
    }

    /// Show `place` in the frame, freshly read from the game and library.
    fn show(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        if self.games.is_empty() {
            self.games = discover_games();
        }
        self.place = place;
        let library = self.library.clone();
        let library = library.borrow();
        let states = self
            .game()
            .map_or_else(HashMap::new, |game| slot_states(game, &library));
        let (screen, events) = match place {
            Place::Game => {
                let game = self.game();
                let fighters = game.map_or_else(Vec::new, |game| {
                    roster(game.file_names().iter().map(String::as_str))
                });
                self.request_renders(&game.map(slot_files).unwrap_or_default());
                let Some(references) = self.references.clone() else {
                    return self.show_welcome(games_found(), window, cx);
                };
                let renders = self.images.clone();
                let skins = library.skins().to_vec();
                let undoable = self.undoable();
                let chip = self.game_chip();
                let split = self.split.clone();
                let page = cx.new(|_| {
                    let mut page = GamePage::new(
                        references, fighters, skins, states, undoable, renders, split,
                    );
                    page.chip = chip;
                    page
                });
                let events = cx.subscribe_in(&page, window, |shell, _, event, window, cx| {
                    shell.costumes_event(event, window, cx)
                });
                (Screen::Game(page), events)
            }
            Place::Library => {
                self.request_skin_renders(&library);
                let renders = library
                    .skins()
                    .iter()
                    .filter_map(|skin| Some((skin.id.clone(), self.images.get(&skin.id)?.clone())))
                    .collect();
                let page = cx.new(|_| LibraryPage::new(library.skins().to_vec(), states, renders));
                let events =
                    cx.subscribe_in(&page, window, |shell, _, event, window, cx| match event {
                        LibraryEvent::Costumes(event) => shell.costumes_event(event, window, cx),
                        LibraryEvent::Remove(id) => shell.remove_skin(id, window, cx),
                    });
                (Screen::Library(page), events)
            }
        };
        self.screen = screen;
        self._screen_events = events;
        cx.notify();
    }

    /// Show `place` with a line saying what just happened, after `changed`
    /// slots changed. Your game keeps what's selected and redraws only them.
    fn show_with(
        &mut self,
        place: Place,
        notice: Option<Notice>,
        changed: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Place::Game, Screen::Game(page)) = (place, &self.screen) {
            let page = page.clone();
            self.refresh_game_page(cx);
            page.update(cx, |page, cx| {
                page.notice = notice;
                for file in changed {
                    page.reload(file, cx);
                }
                cx.notify();
            });
            return;
        }
        self.show(place, window, cx);
        match &self.screen {
            Screen::Game(page) => page.update(cx, |page, _| page.notice = notice),
            Screen::Library(page) => page.update(cx, |page, _| page.notice = notice),
            _ => {}
        }
    }

    fn costumes_event(
        &mut self,
        event: &CostumesEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            CostumesEvent::Add(paths) => self.add_skins(paths, window, cx),
            CostumesEvent::Install { skin, slot } => {
                self.change_slot(slot, Change::Install(skin.clone()), window, cx)
            }
            CostumesEvent::Undo(slot) => self.change_slot(slot, Change::Undo, window, cx),
            CostumesEvent::Restore(slot) => self.change_slot(slot, Change::Restore, window, cx),
            CostumesEvent::Choose => self.choose_skins(window, cx),
            CostumesEvent::Settings => self.open_settings(window, cx),
        }
    }

    /// Settings' stress test: every costume in the game, animated at once.
    fn stress_test(&mut self, cx: &mut Context<Self>) {
        let Some(references) = self.references.clone() else {
            return;
        };
        self.place = Place::Game;
        self.screen = Screen::StressTest(cx.new(|cx| StressTest::new(references, cx)));
        self._screen_events = Subscription::new(|| {});
        cx.notify();
    }

    /// Open Settings over whatever's showing.
    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_some() {
            return;
        }
        let page = cx.new(|_| SettingsPage {
            game: self.game().map(|game| game.path().to_owned()),
            editor: settings::load().external_editor,
            check_for_updates: settings::load().check_for_updates.unwrap_or(true),
        });
        let events = cx.subscribe_in(&page, window, |shell, _, event, window, cx| {
            shell.settings_event(event, window, cx)
        });
        self.settings = Some((page, events));
        cx.notify();
    }

    /// Look for a newer version, unless Settings turned the check off.
    pub(crate) fn check_for_update(&mut self, cx: &mut Context<Self>) {
        if !settings::load().check_for_updates.unwrap_or(true) {
            return;
        }
        cx.spawn(async move |shell, cx| {
            let update = cx
                .background_executor()
                .spawn(async { update::check() })
                .await;
            shell
                .update(cx, |shell, cx| {
                    shell.update = update;
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    /// Show what a problem report would send; `crashed` when the last
    /// session's panic is why.
    pub(crate) fn report_problem(
        &mut self,
        crashed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.report.is_some() {
            return;
        }
        let report = cx.new(|_| Report::new(crashed));
        let events = cx.subscribe_in(&report, window, |shell, _, event, _, cx| match event {
            ReportEvent::Close => {
                shell.report = None;
                cx.notify();
            }
        });
        self.report = Some((report, events));
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings = None;
        cx.notify();
    }

    fn settings_event(
        &mut self,
        event: &SettingsEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsEvent::ChangeGame => {
                self.close_settings(cx);
                self.games = discover_games();
                return self.show_welcome(games_found(), window, cx);
            }
            SettingsEvent::SetEditor(program) => {
                settings::update(|settings| settings.external_editor = program.clone());
                if let Some((page, _)) = &self.settings {
                    page.update(cx, |page, cx| {
                        page.editor = program.clone();
                        cx.notify();
                    });
                }
                // An open editor names and opens the new one at once.
                if let Screen::Editor(editor) = &self.screen {
                    editor.update(cx, |editor, cx| {
                        editor.settings.external_editor = program.clone();
                        cx.notify();
                    });
                }
            }
            SettingsEvent::SetAppearance(appearance) => {
                Theme::init(*appearance, cx);
                window.refresh();
                settings::update(|settings| settings.set_appearance(*appearance));
            }
            SettingsEvent::CheckForUpdates(check) => {
                settings::update(|settings| settings.check_for_updates = Some(*check));
                if let Some((page, _)) = &self.settings {
                    page.update(cx, |page, cx| {
                        page.check_for_updates = *check;
                        cx.notify();
                    });
                }
                if *check {
                    self.check_for_update(cx);
                } else {
                    self.update = None;
                }
            }
            SettingsEvent::ReportProblem => {
                self.close_settings(cx);
                return self.report_problem(false, window, cx);
            }
            SettingsEvent::StressTest => {
                self.close_settings(cx);
                return self.stress_test(cx);
            }
            SettingsEvent::ShowFolder => {
                let folder = dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("textures.gg");
                let _ = std::fs::create_dir_all(&folder);
                return cx.reveal_path(&folder);
            }
        }
        cx.notify();
    }

    fn editor(
        &self,
        file: OpenFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<Editor>, Subscription) {
        let focus = file.viewport.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
        let references = self.references.clone();
        let split = self.split.clone();
        let editor = cx.new(|cx| Editor::new(file, references, settings::load(), Some(split), cx));
        let events = cx.subscribe_in(
            &editor,
            window,
            |shell, editor, event, window, cx| match event {
                EditorEvent::Leave => shell.leave_editor(window, cx),
                // A sidebar click asked about unsaved edits and was called off.
                EditorEvent::Stay => shell.after_leave = None,
                EditorEvent::SaveToLibrary {
                    bytes,
                    install,
                    then,
                } => shell.save_to_library(editor, bytes, *install, then.clone(), window, cx),
            },
        );
        (editor, events)
    }

    /// Back from the editor to where it opened from: Your game takes its
    /// preview back, looking as the editor left it.
    fn leave_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = self.editing_from.take() else {
            return self.show(self.place, window, cx);
        };
        page.update(cx, |page, cx| page.back_from_edit(cx));
        self._screen_events = cx.subscribe_in(&page, window, |shell, _, event, window, cx| {
            shell.costumes_event(event, window, cx)
        });
        self.screen = Screen::Game(page);
        self.show_with(Place::Game, None, &[], window, cx);
        // Left through the sidebar for another place or costume.
        match self.after_leave.take() {
            Some(Leave::Place(place)) if place != Place::Game => self.show(place, window, cx),
            Some(Leave::Open(file)) => self.open_from_game(file, window, cx),
            _ => {}
        }
    }

    /// Use the disc image at `path` if it is the player's Melee; otherwise
    /// say why on the welcome screen.
    fn use_game_at(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let opened = Game::open(&path).map_err(Error::from).and_then(|game| {
            remember_game(&game);
            game_references(game)
        });
        match opened {
            Ok(references) => {
                self.references = Some(Rc::new(references));
                // The renders belong to the game that was open.
                drop_images(std::mem::take(&mut self.images).into_values(), cx);
                self.games = discover_games();
                self.show(self.place, window, cx);
            }
            Err(problem) => {
                self.show_welcome(Status::Problem(problem.to_string().into()), window, cx)
            }
        }
    }

    fn show_welcome(&mut self, status: Status, window: &mut Window, cx: &mut Context<Self>) {
        let (welcome, events) = Self::welcome(status, false, window, cx);
        self.screen = Screen::Welcome(welcome);
        self._screen_events = events;
        cx.notify();
    }

    /// Open the game's costume `file` in the editor, or say why it can't
    /// open.
    fn open_costume(&mut self, file: String, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Game(page) = &self.screen else {
            return;
        };
        let page = page.clone();
        let Some(handoff) = page.read(cx).handoff(&file) else {
            let notice = Notice {
                text: format!("Couldn't open {file}").into(),
                error: true,
            };
            return self.show_with(self.place, Some(notice), &[], window, cx);
        };
        let (editor, events) = self.editor(handoff, window, cx);
        self.recent = settings::update(|settings| settings.remember_edit(&file)).recent;
        self.sidebar_editing = false;
        self.editing_from = Some(page);
        self.screen = Screen::Editor(editor);
        self._screen_events = events;
        cx.notify();
    }

    /// Whether the window may close now; the editor asks about unsaved
    /// edits first.
    pub fn should_close(&mut self, cx: &mut Context<Self>) -> bool {
        match &self.screen {
            Screen::Editor(editor) => {
                editor.update(cx, |editor, cx| editor.proceed_or_ask(Pending::Close, cx))
            }
            _ => true,
        }
    }

    /// Go up a level for Escape; whether there was one.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.review.is_some() || self.settings.is_some() || self.report.is_some() {
            return false;
        }
        match &self.screen {
            Screen::Game(page) if page.read(cx).subject().is_some() => {
                self.up(window, cx);
                true
            }
            Screen::StressTest(_) => {
                self.up(window, cx);
                true
            }
            Screen::Editor(editor) => {
                let editor = editor.clone();
                if editor.read(cx).home()
                    && editor.update(cx, |editor, cx| editor.proceed_or_ask(Pending::Leave, cx))
                {
                    self.leave_editor(window, cx);
                }
                true
            }
            _ => false,
        }
    }

    /// Up a level from a fighter, a stage, or the stress test.
    fn up(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.screen {
            Screen::Game(page) => page.update(cx, |page, cx| page.show_roster(cx)),
            _ => self.show(self.place, window, cx),
        }
        cx.notify();
    }

    /// Go to `place` from the sidebar. From the editor it's like Done:
    /// unsaved edits are asked about first.
    fn go(&mut self, place: Place, window: &mut Window, cx: &mut Context<Self>) {
        if let Screen::Editor(editor) = &self.screen {
            let editor = editor.clone();
            self.after_leave = Some(Leave::Place(place));
            if editor.update(cx, |editor, cx| editor.proceed_or_ask(Pending::Leave, cx)) {
                self.leave_editor(window, cx);
            }
            return;
        }
        self.show(place, window, cx);
    }

    /// Open a recent edit: its slot in Your game, then the editor. From the
    /// editor, unsaved edits are asked about first.
    fn open_recent(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.recent.get(index).cloned() else {
            return;
        };
        if let Screen::Editor(editor) = &self.screen {
            let editor = editor.clone();
            self.after_leave = Some(Leave::Open(file));
            if editor.update(cx, |editor, cx| editor.proceed_or_ask(Pending::Leave, cx)) {
                self.leave_editor(window, cx);
            }
            return;
        }
        self.open_from_game(file, window, cx);
    }

    /// Show the game's `file` in Your game, then open it in the editor.
    fn open_from_game(&mut self, file: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.place != Place::Game || !matches!(self.screen, Screen::Game(_)) {
            self.show(Place::Game, window, cx);
        }
        let Screen::Game(page) = &self.screen else {
            return;
        };
        let page = page.clone();
        if page.update(cx, |page, cx| page.show_file(&file, cx)) {
            self.open_costume(file, window, cx);
        } else {
            let notice = Notice {
                text: format!("Your game has no {file}").into(),
                error: true,
            };
            self.show_with(Place::Game, Some(notice), &[], window, cx);
        }
    }
}

/// Where to go once the editor lets go.
enum Leave {
    Place(Place),
    /// Open this game file in the editor again.
    Open(String),
}

/// The pane column's width as the player left it.
fn shared_split(cx: &mut Context<Shell>) -> Entity<SplitState> {
    let settings = settings::load();
    cx.new(|_| pane_split(&settings))
}

/// The welcome screen's list of the player's games, or that there are none.
fn games_found() -> Status {
    let games = discover_games();
    if games.is_empty() {
        Status::Missing
    } else {
        Status::Games(games)
    }
}
