//! The first screens: an introduction, then finding the player's Melee. Every
//! Melee NTSC 1.02 ISO in their game folders is listed for them to choose,
//! since skins will be installed into it, with what sets each apart: the one
//! Slippi plays, and how much of it is already changed. A file they can't
//! use, or no game at all, is explained with a way forward.

use crate::game::GameChoice;
use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Div, EventEmitter, ExternalPaths, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, Styled, Window, div,
};
use std::path::PathBuf;
use tgg_ui::tokens::{font, radius, space, text};
use tgg_ui::{Button, ButtonSize, ButtonVariant, Palette, Theme, rem};

/// What the search for the player's game turned up.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Status {
    /// The Melee ISOs in the player's game folders, for them to choose.
    Games(Vec<GameChoice>),
    /// No game where the player might keep one.
    Missing,
    /// A chosen file that can't be the game, and why.
    Problem(SharedString),
}

pub(crate) enum WelcomeEvent {
    /// Install skins into the disc image at this path.
    Use(PathBuf),
}

pub(crate) struct Welcome {
    /// Show the introduction first (a first launch).
    pub intro: bool,
    pub status: Status,
}

impl EventEmitter<WelcomeEvent> for Welcome {}

impl Welcome {
    fn choose(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose your Melee ISO".into()),
        });
        cx.spawn(async move |welcome, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(path) = paths.into_iter().next() {
                welcome
                    .update(cx, |_, cx| cx.emit(WelcomeEvent::Use(path)))
                    .ok();
            }
        })
        .detach();
    }

    /// The introduction: what the app does, and a way in.
    fn intro(&self, cx: &mut Context<Self>) -> Div {
        let palette = Theme::global(cx).palette;
        let this = cx.entity();
        let point = |text: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(rem(space::SM))
                .child(
                    div()
                        .size(rem(space::XS))
                        .flex_none()
                        .rounded_full()
                        .bg(palette.accent.to_gpui()),
                )
                .child(text)
        };
        div()
            .flex()
            .flex_col()
            .gap(rem(space::LG))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(
                        div()
                            .text_size(rem(text::H2))
                            .font_weight(FontWeight::BOLD)
                            .child("Welcome to textures.gg"),
                    )
                    .child(
                        div()
                            .text_size(rem(text::LG))
                            .text_color(palette.muted.to_gpui())
                            .child("New looks for your Melee characters, in a few clicks."),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(point("Put skins into the Melee you play on Slippi"))
                    .child(point("See every costume in 3D, moving, before you play"))
                    .child(point(
                        "Touch up textures in the image editor you already use",
                    )),
            )
            .child(
                div().child(
                    Button::new("get-started", "Get started")
                        .variant(ButtonVariant::Primary)
                        .on_press(move |_, cx| {
                            this.update(cx, |welcome, cx| {
                                welcome.intro = false;
                                cx.notify();
                            })
                        }),
                ),
            )
    }

    /// One ISO to choose: its name and folder, what sets it apart, and Use.
    fn game_row(
        &self,
        index: usize,
        game: &GameChoice,
        primary: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let palette = Theme::global(cx).palette;
        let this = cx.entity();
        let path = game.path.clone();
        let name = game
            .path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let folder = game
            .path
            .parent()
            .map_or_else(String::new, |folder| folder.display().to_string());
        let changed = match game.changed {
            0 => badge("Vanilla", palette.success, palette),
            1 => badge("1 file changed", palette.muted, palette),
            changed => badge(&format!("{changed} files changed"), palette.muted, palette),
        };
        div()
            .flex()
            .items_center()
            .gap(rem(space::MD))
            .py(rem(space::SM))
            .when(index > 0, |row| {
                row.border_t_1().border_color(palette.line.to_gpui())
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(rem(space::XXS))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .font_family(font::MONO)
                            .text_size(rem(text::XS))
                            .text_color(palette.muted.to_gpui())
                            .truncate()
                            .child(folder),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(rem(space::XS))
                            .when(game.slippi_plays, |badges| {
                                badges.child(badge(
                                    "Slippi plays this one",
                                    palette.accent_text,
                                    palette,
                                ))
                            })
                            .child(changed),
                    ),
            )
            .child(
                Button::new(
                    SharedString::from(format!("use-game-{index}")),
                    "Use this ISO",
                )
                .variant(if primary {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Secondary
                })
                .size(ButtonSize::Sm)
                .on_press(move |_, cx| {
                    let path = path.clone();
                    this.update(cx, |_, cx| cx.emit(WelcomeEvent::Use(path)))
                }),
            )
    }
}

/// A small label in `color`, such as "Vanilla".
fn badge(label: &str, color: tgg_ui::Color, palette: Palette) -> Div {
    div()
        .px(rem(space::XS))
        .rounded(rem(radius::SM))
        .border_1()
        .border_color(palette.line.to_gpui())
        .text_size(rem(text::XS))
        .text_color(color.to_gpui())
        .child(label.to_owned())
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let page = div()
            .id("welcome")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(rem(space::XL))
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                if let Some(path) = paths.paths().first() {
                    cx.emit(WelcomeEvent::Use(path.clone()));
                }
            }));
        if self.intro {
            let intro = self.intro(cx);
            return page.child(div().max_w(rem(560.0)).child(intro));
        }

        let this = cx.entity();
        let choose = move |label: &'static str, variant: ButtonVariant| {
            let this = this.clone();
            Button::new("choose-iso", label)
                .variant(variant)
                .size(ButtonSize::Sm)
                .on_press(move |_, cx| this.update(cx, |welcome, cx| welcome.choose(cx)))
        };
        let status = self.status.clone();
        let (headline, body): (String, Div) = match &status {
            Status::Games(games) => {
                let headline = match games.as_slice() {
                    [game] if game.slippi_plays => {
                        "Found your Melee through Slippi Launcher".to_owned()
                    }
                    [_] => "Found your Melee".to_owned(),
                    games => format!(
                        "You have {} Melee ISOs. Which one should skins go into?",
                        games.len()
                    ),
                };
                let rows = games
                    .iter()
                    .enumerate()
                    .map(|(index, game)| self.game_row(index, game, index == 0, cx))
                    .collect::<Vec<_>>();
                (
                    headline,
                    div().flex().flex_col().children(rows).child(
                        div()
                            .pt(rem(space::XS))
                            .child(choose("Choose a different ISO…", ButtonVariant::Ghost)),
                    ),
                )
            }
            Status::Missing => (
                "Where's your Melee?".to_owned(),
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(div().text_color(palette.muted.to_gpui()).child(
                        "If you play on Slippi, it's the ISO Slippi Launcher's settings \
                         point to. You can also drop it on this window.",
                    ))
                    .child(div().child(choose("Choose ISO…", ButtonVariant::Primary))),
            ),
            Status::Problem(problem) => (
                "That ISO won't work".to_owned(),
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(
                        div()
                            .text_color(palette.danger.to_gpui())
                            .child(problem.clone()),
                    )
                    .child(div().child(choose("Choose another ISO…", ButtonVariant::Primary))),
            ),
        };

        page.child(
            div()
                .w_full()
                .max_w(rem(640.0))
                .flex()
                .flex_col()
                .gap(rem(space::LG))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(rem(space::XS))
                        .child(
                            div()
                                .text_size(rem(text::H3))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Find your Melee"),
                        )
                        .child(div().text_color(palette.muted.to_gpui()).child(
                            "Skins go into your copy of Super Smash Bros. Melee: the NTSC \
                             1.02 ISO you play on Slippi. Nothing in it changes until you \
                             install a skin.",
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(rem(space::SM))
                        .p(rem(space::MD))
                        .rounded(rem(radius::MD))
                        .border_1()
                        .border_color(palette.line.to_gpui())
                        .bg(palette.surface.to_gpui())
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(headline))
                        .child(body),
                ),
        )
    }
}
