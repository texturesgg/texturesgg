//! The editor's panes: which are open and folded, and the textures, texture
//! and moves panes themselves.

use super::view::dispatch;
use super::{EditExternally, ExportPng, ImportPng};
use super::{Editor, PaneKind, keys};
use crate::settings::{PaneState, Settings};
use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, Window, div};
use tgg_ui::tokens::{density, font, radius, space, text};
use tgg_ui::{
    Button, ButtonSize, ButtonVariant, IconButton, IconName, OptionList, Pane, PaneFit, PaneSize,
    SplitState, Theme, ThumbnailList, Tooltip, rem, shortcut,
};

impl Editor {
    pub(super) fn pane(&self, kind: PaneKind) -> PaneState {
        match kind {
            PaneKind::Textures => self.panes.textures,
            PaneKind::Texture => self.panes.texture,
            PaneKind::Moves => self.panes.moves,
            PaneKind::Colors => self.panes.colors,
        }
    }

    fn pane_mut(&mut self, kind: PaneKind) -> &mut PaneState {
        match kind {
            PaneKind::Textures => &mut self.panes.textures,
            PaneKind::Texture => &mut self.panes.texture,
            PaneKind::Moves => &mut self.panes.moves,
            PaneKind::Colors => &mut self.panes.colors,
        }
    }

    /// Open or close a pane; opening one unfolds it.
    pub(crate) fn toggle_pane(&mut self, kind: PaneKind, cx: &mut Context<Self>) {
        let pane = self.pane_mut(kind);
        pane.open = !pane.open;
        pane.folded = false;
        self.settings.panes = Some(self.panes);
        self.persist_settings(cx);
        cx.notify();
    }

    fn fold_pane(&mut self, kind: PaneKind, cx: &mut Context<Self>) {
        let pane = self.pane_mut(kind);
        pane.folded = !pane.folded;
        self.settings.panes = Some(self.panes);
        self.persist_settings(cx);
        cx.notify();
    }

    /// A pane of `kind` around `body`, with its fold and close wired.
    pub(crate) fn pane_card(
        &self,
        kind: PaneKind,
        title: impl Into<SharedString>,
        fit: PaneFit,
        cx: &mut Context<Self>,
    ) -> Pane {
        let state = self.pane(kind);
        let fold = cx.entity();
        let close = cx.entity();
        Pane::new(("pane", kind as usize), title, fit)
            .folded(state.folded, move |_, cx| {
                fold.update(cx, |editor, cx| editor.fold_pane(kind, cx))
            })
            .on_close(move |_, cx| close.update(cx, |editor, cx| editor.toggle_pane(kind, cx)))
    }

    /// The open panes, top to bottom.
    pub(super) fn panes(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut panes = Vec::new();
        if self.panes.textures.open {
            panes.push(self.textures_pane(cx).into_any_element());
        }
        if self.panes.texture.open {
            panes.push(self.texture_pane(cx).into_any_element());
        }
        if self.panes.colors.open {
            panes.push(self.colors_pane(window, cx).into_any_element());
        }
        if self.panes.moves.open && !self.moves.rows.is_empty() {
            panes.push(self.moves_pane(cx).into_any_element());
        }
        panes
    }

    fn textures_pane(&self, cx: &mut Context<Self>) -> Pane {
        let palette = Theme::global(cx).palette;
        let this = cx.entity();
        let list = ThumbnailList::new(
            "textures",
            self.thumbnails.clone(),
            self.selected,
            move |index, _, cx| {
                this.update(cx, |editor, cx| {
                    editor.select(Some(index), cx);
                    cx.notify();
                })
            },
        );
        self.pane_card(PaneKind::Textures, "Textures", PaneFit::Fill, cx)
            .count(self.thumbnails.len().to_string())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(div().flex_1().min_h_0().child(list))
                    .when_some(self.document.as_ref().err(), |body, error| {
                        body.child(
                            div()
                                .p(rem(space::SM))
                                .text_size(rem(text::XS))
                                .text_color(palette.danger.to_gpui())
                                .child(format!("Textures unavailable: {error}")),
                        )
                    }),
            )
    }

    /// The selected texture: its picture and facts, and the ways to change
    /// it.
    fn texture_pane(&self, cx: &mut Context<Self>) -> Pane {
        let palette = Theme::global(cx).palette;
        let selected =
            self.selected
                .zip(self.document.as_ref().ok())
                .and_then(|(index, document)| {
                    let texture = document.textures().get(index)?;
                    Some((index, texture.clone()))
                });
        let title = selected.as_ref().map_or_else(
            || "Texture".to_owned(),
            |(index, _)| self.texture_name(*index),
        );
        let body = match selected {
            None => div()
                .px(rem(space::MD))
                .pb(rem(space::MD))
                .text_size(rem(text::XS))
                .text_color(palette.muted.to_gpui())
                .child("Select a texture, in the list or on the model.")
                .into_any_element(),
            Some((index, texture)) => {
                let format = texture.format.name();
                let fact = |label: &'static str, value: String| {
                    div()
                        .flex()
                        .gap(rem(space::SM))
                        .text_size(rem(density::DETAIL_TEXT))
                        .child(
                            div()
                                .w(rem(64.0))
                                .flex_none()
                                .text_color(palette.muted.to_gpui())
                                .child(label),
                        )
                        .child(div().font_family(font::MONO).child(value))
                };
                let editor = self
                    .settings
                    .external_editor
                    .as_deref()
                    .and_then(editor_name);
                div()
                    .px(rem(space::MD - 2.0))
                    .pb(rem(space::MD - 2.0))
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(
                        div()
                            .flex()
                            .gap(rem(space::MD - 2.0))
                            .child(
                                div()
                                    .flex_none()
                                    .size(rem(96.0))
                                    .rounded(rem(radius::SM))
                                    .overflow_hidden()
                                    .relative()
                                    .bg(palette.bg.to_gpui())
                                    .when_some(
                                        self.thumbnails[index].image.clone(),
                                        |preview, image| {
                                            preview.child(tgg_ui::over_checkerboard(
                                                image,
                                                palette,
                                                rem(radius::SM),
                                            ))
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(rem(space::XXS))
                                    .child(fact("Format", format.to_owned()))
                                    .child(fact(
                                        "Size",
                                        format!("{} × {}", texture.width, texture.height),
                                    ))
                                    .child(fact("Offset", format!("{:#x}", texture.data_offset)))
                                    .child(fact("Used by", texture.uses.len().to_string()))
                                    .when_some(texture.frame.as_ref(), |facts, frame| {
                                        facts.child(fact(
                                            "Frame",
                                            format!("{} of {}", frame.frame + 1, frame.frames),
                                        ))
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rem(space::XXS))
                            .child(
                                Button::new(
                                    "edit-externally",
                                    match &editor {
                                        Some(name) => format!("Edit in {name}"),
                                        None => "Edit in image editor".to_owned(),
                                    },
                                )
                                .variant(ButtonVariant::Primary)
                                .size(ButtonSize::Sm)
                                .tooltip(
                                    Tooltip::new("Each save there updates it here")
                                        .shortcut(shortcut(keys::EDIT_EXTERNALLY)),
                                )
                                .on_press(dispatch(EditExternally)),
                            )
                            .child(
                                IconButton::new("import-png", IconName::Import, "Import PNG…")
                                    .shortcut(shortcut(keys::IMPORT))
                                    .on_press(dispatch(ImportPng)),
                            )
                            .child(
                                IconButton::new("export-png", IconName::Export, "Export PNG…")
                                    .shortcut(shortcut(keys::EXPORT))
                                    .on_press(dispatch(ExportPng)),
                            ),
                    )
                    .into_any_element()
            }
        };
        self.pane_card(PaneKind::Texture, title, PaneFit::Content, cx)
            .child(body)
    }

    fn moves_pane(&self, cx: &mut Context<Self>) -> Pane {
        let this = cx.entity();
        let playing = self
            .viewport
            .read(cx)
            .playback()
            .and_then(|playback| self.moves.row(playback.current()));
        // The list scrolls inside a height of its own: its rows, up to a cap.
        let height =
            (self.moves.rows.len() as f32 * density::CONTROL_MD + space::XS).min(MOVES_HEIGHT);
        self.pane_card(PaneKind::Moves, "Moves", PaneFit::Content, cx)
            .count(self.moves.playable.to_string())
            .child(
                div()
                    .h(rem(height))
                    .px(rem(space::XXS))
                    .child(OptionList::new(
                        "moves",
                        self.moves.rows.clone(),
                        playing,
                        move |row, _, cx| this.update(cx, |editor, cx| editor.play_move(row, cx)),
                    )),
            )
    }
}

/// The tallest the moves list grows before it scrolls, in web pixels.
const MOVES_HEIGHT: f32 = 260.0;

/// The pane column's width: its default and range in web pixels, and the
/// width the player last dragged it to.
pub(crate) fn pane_split(settings: &Settings) -> SplitState {
    let mut split = SplitState {
        // The column has one edge; the left size goes unused.
        left: PaneSize::new(0.0, 0.0, 0.0),
        right: PaneSize::new(320.0, 260.0, 520.0),
    };
    if let Some(width) = settings.inspector_width {
        split.right.set(width);
    }
    split
}

/// The name of the image editor a command runs, as a player knows it:
/// "Aseprite" for `aseprite`, "Photos" for `/System/Applications/Photos.app`
/// or `open -a Photos`.
fn editor_name(command: &str) -> Option<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let program = match words.as_slice() {
        ["open", "-a", app, ..] => app,
        [program, ..] => program,
        [] => return None,
    };
    let program = program.trim_end_matches('/');
    let program = program.rsplit('/').next().unwrap_or(program);
    let program = program.strip_suffix(".app").unwrap_or(program);
    let mut name = program.to_owned();
    if let Some(first) = name.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::editor_name;

    #[test]
    fn editors_are_named_as_players_know_them() {
        assert_eq!(editor_name("aseprite").as_deref(), Some("Aseprite"));
        assert_eq!(
            editor_name("/System/Applications/Photos.app").as_deref(),
            Some("Photos")
        );
        assert_eq!(
            editor_name("open -a Photoshop.app").as_deref(),
            Some("Photoshop")
        );
        assert_eq!(editor_name("gimp --new-instance").as_deref(), Some("Gimp"));
        assert_eq!(editor_name("  "), None);
    }
}
