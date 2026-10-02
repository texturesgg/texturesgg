//! The player's settings: which ISO skins go into, the image editor textures
//! open in, the theme, and where the app keeps their files.

use gpui::{
    Context, Div, EventEmitter, FontWeight, IntoElement, ParentElement, PathPromptOptions, Render,
    Styled, Window, div,
};
use std::path::PathBuf;
use tgg_ui::tokens::{density, font, space};
use tgg_ui::{Appearance, Button, ButtonSize, ButtonVariant, MenuButton, MenuItem, Theme, rem};

pub(crate) enum SettingsEvent {
    ChangeGame,
    /// Open textures in this program; the system's default when `None`.
    SetEditor(Option<String>),
    SetAppearance(Appearance),
    ShowFolder,
    /// Look for a newer version at launch, or never.
    CheckForUpdates(bool),
    ReportProblem,
    StressTest,
}

pub(crate) struct SettingsPage {
    pub game: Option<PathBuf>,
    pub editor: Option<String>,
    pub check_for_updates: bool,
}

impl EventEmitter<SettingsEvent> for SettingsPage {}

impl SettingsPage {
    fn choose_editor(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose an image editor".into()),
        });
        cx.spawn(async move |page, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let program = path.display().to_string();
                page.update(cx, |_, cx| cx.emit(SettingsEvent::SetEditor(Some(program))))
                    .ok();
            }
        })
        .detach();
    }
}

/// A setting: its name, what it's set to, and ways to change it.
fn setting(name: &str, value: String, mono: bool, actions: Div, cx: &Context<SettingsPage>) -> Div {
    let palette = Theme::global(cx).palette;
    let value = div()
        .text_size(rem(density::DETAIL_TEXT))
        .text_color(palette.muted.to_gpui())
        .truncate()
        .child(value);
    div()
        .flex()
        .items_center()
        .gap(rem(space::MD))
        .py(rem(space::SM))
        .border_b_1()
        .border_color(palette.line.to_gpui())
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(rem(space::XXS))
                .child(div().font_weight(FontWeight::MEDIUM).child(name.to_owned()))
                .child(if mono {
                    value.font_family(font::MONO)
                } else {
                    value
                }),
        )
        .child(actions.flex_none().flex().gap(rem(space::XS)))
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = Theme::global(cx).appearance;
        let emit = |event: fn() -> SettingsEvent, cx: &mut Context<Self>| {
            let this = cx.entity();
            move |_: &mut Window, cx: &mut gpui::App| this.update(cx, |_, cx| cx.emit(event()))
        };
        let choose = cx.entity();
        let theme = [Appearance::Gallery, Appearance::Paper].into_iter().fold(
            MenuButton::new(
                "theme",
                match appearance {
                    Appearance::Gallery => "Gallery",
                    Appearance::Paper => "Paper",
                },
            )
            .select()
            .size(ButtonSize::Sm),
            |menu, option| {
                let this = cx.entity();
                menu.item(
                    MenuItem::new(
                        match option {
                            Appearance::Gallery => "Gallery",
                            Appearance::Paper => "Paper",
                        },
                        move |_, cx| {
                            this.update(cx, |_, cx| cx.emit(SettingsEvent::SetAppearance(option)))
                        },
                    )
                    .checked(appearance == option),
                )
            },
        );
        let when = |check: bool| if check { "At launch" } else { "Never" };
        let updates = [true, false].into_iter().fold(
            MenuButton::new("updates", when(self.check_for_updates))
                .select()
                .size(ButtonSize::Sm),
            |menu, option| {
                let this = cx.entity();
                menu.item(
                    MenuItem::new(when(option), move |_, cx| {
                        this.update(cx, |_, cx| cx.emit(SettingsEvent::CheckForUpdates(option)))
                    })
                    .checked(self.check_for_updates == option),
                )
            },
        );
        let rows = [
            setting(
                "Your Melee",
                self.game
                    .as_ref()
                    .map_or_else(|| "None chosen".into(), |path| path.display().to_string()),
                true,
                div().child(
                    Button::new("change-game", "Change…")
                        .size(ButtonSize::Sm)
                        .on_press(emit(|| SettingsEvent::ChangeGame, cx)),
                ),
                cx,
            ),
            setting(
                "Image editor",
                self.editor
                    .clone()
                    .unwrap_or_else(|| "Your system's default".into()),
                self.editor.is_some(),
                div()
                    .child(
                        Button::new("choose-editor", "Choose…")
                            .size(ButtonSize::Sm)
                            .on_press(move |_, cx| {
                                choose.update(cx, |page, cx| page.choose_editor(cx))
                            }),
                    )
                    .children(self.editor.is_some().then(|| {
                        Button::new("default-editor", "Use default")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .on_press(emit(|| SettingsEvent::SetEditor(None), cx))
                    })),
                cx,
            ),
            setting("Theme", String::new(), false, div().child(theme), cx),
            setting(
                "Your files",
                "Skins and install history".into(),
                false,
                div().child(
                    Button::new("show-folder", "Show folder")
                        .size(ButtonSize::Sm)
                        .on_press(emit(|| SettingsEvent::ShowFolder, cx)),
                ),
                cx,
            ),
            setting(
                "Check for updates",
                concat!("You have ", env!("CARGO_PKG_VERSION")).into(),
                false,
                div().child(updates),
                cx,
            ),
            setting(
                "Problems",
                "See what a report sends before sending it".into(),
                false,
                div().child(
                    Button::new("report-problem", "Report…")
                        .size(ButtonSize::Sm)
                        .on_press(emit(|| SettingsEvent::ReportProblem, cx)),
                ),
                cx,
            ),
            setting(
                "Stress test",
                "Every costume in your game, animated at once".into(),
                false,
                div().child(
                    Button::new("stress-test", "Run")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_press(emit(|| SettingsEvent::StressTest, cx)),
                ),
                cx,
            ),
        ];
        // The settings dialog gives the rows their frame.
        div().flex().flex_col().children(rows)
    }
}
