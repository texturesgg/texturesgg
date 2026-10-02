//! The shell's window: the frame around a place, the sidebar, and the
//! dialogs over them.

use super::{Place, Screen, Shell};
use crate::costumes::slot_label;
use crate::editor::{self};
use crate::menus;
use crate::page::GameChip;
use crate::settings;
use crate::{OpenSettings, Quit, ReportProblem, ToggleSidebar};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render,
    Styled, Window, div,
};
use tgg_ui::tokens::{density, font, space};
use tgg_ui::{
    Breadcrumbs, Button, ButtonSize, ButtonVariant, Dialog, IconButton, IconName, MenuButton,
    Sidebar, SidebarEntry, Theme, rem, shortcut, title_bar,
};

impl Shell {
    /// The chip naming the game skins install into.
    pub(super) fn game_chip(&self) -> Option<GameChip> {
        let game = self.game()?;
        Some(GameChip {
            slippi: self
                .games
                .iter()
                .any(|choice| same_file(&choice.path, game.path()) && choice.slippi_plays),
            file: game
                .path()
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        })
    }

    /// Whether the sidebar shows now: beside the places as the player left
    /// it, over the editor only when asked for.
    fn sidebar_shown(&self) -> bool {
        if matches!(self.screen, Screen::Editor(_)) {
            self.sidebar_editing
        } else {
            self.sidebar
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        if matches!(self.screen, Screen::Editor(_)) {
            self.sidebar_editing = !self.sidebar_editing;
        } else {
            self.sidebar = !self.sidebar;
            let sidebar = self.sidebar;
            settings::update(|settings| settings.sidebar = Some(sidebar));
        }
        cx.notify();
    }

    /// Where the title starts in a bar: past the window buttons and the
    /// sidebar toggle while the sidebar is hidden.
    fn bar_start(&self) -> f32 {
        if self.sidebar_shown() {
            space::MD
        } else {
            title_bar::CLEARANCE
        }
    }

    /// The frame around a place: the top bar with where you are and what
    /// the place can do, beside the sidebar.
    fn frame(&self, page: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let palette = Theme::global(cx).palette;
        let subject = match &self.screen {
            Screen::Game(page) => page.read(cx).subject(),
            Screen::StressTest(_) => Some("Stress test".to_owned()),
            _ => None,
        };
        let place = match self.place {
            Place::Game => "Melee",
            Place::Library => "Library",
        };
        let up = cx.entity();
        // A top-level page says its own name; the row stays empty.
        let crumbs = subject.map(|subject| {
            Breadcrumbs::new()
                .level(place, move |window, cx| {
                    up.update(cx, |shell, cx| shell.up(window, cx))
                })
                .page(subject)
        });
        // An open fighter or stage: its selected slot opens in the editor.
        let edit = match &self.screen {
            Screen::Game(page) => page.read(cx).editable(),
            _ => None,
        }
        .map(|file| {
            let open = cx.entity();
            Button::new("edit-textures", "Edit textures")
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Sm)
                .on_press(move |window, cx| {
                    let file = file.clone();
                    open.update(cx, |shell, cx| shell.open_costume(file, window, cx))
                })
        });
        let update = self.update.as_ref().map(|version| {
            Button::new("update", format!("Get {version}"))
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .on_press(|_, cx| cx.open_url(crate::update::DOWNLOAD_URL))
        });
        let bar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(space::SM))
            // The editor's bar is the same height, so nothing below moves
            // between modes.
            .h(rem(title_bar::HEIGHT))
            .pl(rem(self.bar_start()))
            .pr(rem(space::MD))
            .bg(palette.bg.to_gpui())
            .children(crumbs)
            .child(title_bar::drag_area())
            .children(update)
            .children(edit);
        self.with_sidebar(
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(palette.bg.to_gpui())
                .child(bar)
                .child(div().flex_1().min_h_0().child(page))
                .into_any_element(),
            cx,
        )
    }

    /// `content` beside the sidebar, when it shows.
    fn with_sidebar(&self, content: AnyElement, cx: &mut Context<Self>) -> AnyElement {
        let sidebar = self.sidebar_shown().then(|| {
            let select = cx.entity();
            let open = cx.entity();
            let selected = match (&self.screen, self.place) {
                (Screen::Editor(_), _) | (_, Place::Game) => 0,
                (_, Place::Library) => 1,
            };
            let entries = self
                .recent
                .iter()
                .map(|file| SidebarEntry {
                    image: self.images.get(file).cloned(),
                    label: slot_label(Some(file)).into(),
                    detail: None,
                })
                .collect();
            Sidebar::new("places", Some(selected), move |index, window, cx| {
                select.update(cx, |shell, cx| match index {
                    0 => shell.go(Place::Game, window, cx),
                    1 => shell.go(Place::Library, window, cx),
                    // Settings opens over the screen rather than replacing it.
                    _ => shell.open_settings(window, cx),
                })
            })
            .place(IconName::Game, "Melee")
            .place_counted(
                IconName::Library,
                "Library",
                self.library.borrow().skins().len(),
            )
            .entries("Recent edits", entries, move |index, window, cx| {
                open.update(cx, |shell, cx| shell.open_recent(index, window, cx))
            })
            .foot(IconName::Settings, "Settings")
        });
        div()
            .size_full()
            .flex()
            .children(sidebar)
            .child(div().flex_1().min_w_0().h_full().child(content))
            .into_any_element()
    }
}

/// Whether `a` and `b` name the same file, however they're written.
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    let canonical =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(a) == canonical(b)
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let welcome = matches!(self.screen, Screen::Welcome(_));
        let screen = match &self.screen {
            Screen::Welcome(welcome) => div()
                .size_full()
                .flex()
                .flex_col()
                // The top row, for the window buttons or the menu button.
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .h(rem(title_bar::HEIGHT))
                        .child(title_bar::drag_area()),
                )
                .child(div().flex_1().min_h_0().child(welcome.clone()))
                .into_any_element(),
            Screen::Editor(editor) => {
                let clear = !self.sidebar_editing;
                editor.update(cx, |editor, _| editor.clear_toggle = clear);
                let editor = editor.clone().into_any_element();
                self.with_sidebar(editor, cx)
            }
            Screen::Game(costumes) => {
                let page = costumes.clone().into_any_element();
                self.frame(page, cx)
            }
            Screen::Library(page) => {
                let page = page.clone().into_any_element();
                self.frame(page, cx)
            }
            Screen::StressTest(test) => {
                let test = test.clone().into_any_element();
                self.frame(test, cx)
            }
        };
        // The sidebar toggle keeps one place beside the window buttons,
        // whether the sidebar shows or not.
        let toggle = (!welcome).then(|| {
            let this = cx.entity();
            div()
                .absolute()
                .top(rem((title_bar::HEIGHT - density::CONTROL_MD) / 2.0))
                .left(rem(title_bar::TOGGLE_LEFT))
                .child(
                    IconButton::new(
                        "sidebar-toggle",
                        IconName::Sidebar,
                        if self.sidebar_shown() {
                            "Hide sidebar"
                        } else {
                            "Show sidebar"
                        },
                    )
                    .shortcut(shortcut(editor::keys::TOGGLE_SIDEBAR))
                    .on_press(move |_, cx| this.update(cx, |shell, cx| shell.toggle_sidebar(cx))),
                )
        });
        // Where the platform has no menu bar, its menus are a button at the
        // top row's left, on every screen.
        let menu = title_bar::MENU_BUTTON.then(|| {
            div()
                .absolute()
                .top(rem((title_bar::HEIGHT - density::CONTROL_MD) / 2.0))
                .left(rem(title_bar::MENU_LEFT))
                .child(MenuButton::icon("app-menu", IconName::Menu, "Menu").menus(menus::menus()))
        });
        // Every screen inherits the theme's colors and type from here.
        tgg_ui::focus_navigation(div().id("shell").track_focus(&self.focus))
            .on_action(cx.listener(|shell, _: &ToggleSidebar, _, cx| shell.toggle_sidebar(cx)))
            .on_action(
                cx.listener(|shell, _: &OpenSettings, window, cx| shell.open_settings(window, cx)),
            )
            .on_action(cx.listener(|shell, _: &ReportProblem, window, cx| {
                shell.report_problem(false, window, cx)
            }))
            // Escape goes up a level: from a fighter to the roster, from the
            // stress test to Melee, from the editor as Done does. Dialogs
            // and menus take it first.
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && shell.escape(window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|shell, _: &Quit, _, cx| {
                if shell.should_close(cx) {
                    cx.quit();
                }
            }))
            .relative()
            .size_full()
            .bg(palette.bg.to_gpui())
            .text_color(palette.text.to_gpui())
            .font_family(font::SANS)
            .child(screen)
            .children(menu)
            .children(toggle)
            .children(self.review.as_ref().map(|(review, _)| review.clone()))
            .children(self.report.as_ref().map(|(report, _)| report.clone()))
            .children(self.settings.as_ref().map(|(page, _)| {
                let close = cx.entity();
                let done = cx.entity();
                Dialog::new("settings", "Settings", move |_, cx| {
                    close.update(cx, |shell, cx| shell.close_settings(cx))
                })
                .width(SETTINGS_WIDTH)
                .body(page.clone())
                .action("Done", ButtonVariant::Primary, move |_, cx| {
                    done.update(cx, |shell, cx| shell.close_settings(cx))
                })
            }))
    }
}

/// The settings dialog's width, in web pixels.
const SETTINGS_WIDTH: f32 = 560.0;
