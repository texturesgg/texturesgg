//! The editor's window: the top bar, the More menu, and the stage with the
//! panes over it.

use super::{
    EditExternally, ExportPng, ImportPng, OpenDat, Redo, ResetCamera, Save, SaveAndInstall, SaveAs,
    ToggleHighlight, ToggleMovesPane, TogglePlayback, ToggleTexturePane, ToggleTexturesPane, Undo,
};
use super::{Editor, EditorEvent, PaneKind, Pending, Unsaved, keys};
use crate::stage;
use crate::viewport::Viewport;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Context, Entity, ExternalPaths, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, div,
};
use tgg_ui::pane::PANE_MARGIN;
use tgg_ui::tokens::{density, font, radius, space, text};
use tgg_ui::{
    Appearance, Breadcrumbs, Button, ButtonSize, ButtonVariant, Dialog, IconButton, IconName,
    MenuButton, MenuItem, PaneColumn, Theme, Tooltip, rem, shortcut, title_bar,
};

/// The status line: the viewport's frame stats. It redraws with each
/// viewport frame, so the rest of the window redraws only when something
/// changes.
pub(crate) struct StatusLine {
    viewport: Entity<Viewport>,
}

impl StatusLine {
    pub fn new(viewport: Entity<Viewport>, cx: &mut Context<Self>) -> Self {
        cx.observe(&viewport, |_, _, cx| cx.notify()).detach();
        Self { viewport }
    }
}

impl Render for StatusLine {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .font_family(font::MONO)
            .text_size(rem(text::XS))
            .text_color(Theme::global(cx).palette.muted.to_gpui())
            .truncate()
            .child(self.viewport.read(cx).status())
    }
}

/// A menu or button handler that dispatches `action` from the focused
/// element, so it reaches the editor's action handlers.
pub(crate) fn dispatch(action: impl gpui::Action) -> impl Fn(&mut Window, &mut App) + 'static {
    move |window, cx| window.dispatch_action(action.boxed_clone(), cx)
}

impl Editor {
    /// The bar across the top: where the costume came from and what it is,
    /// whether it's saved, the pane toggles, and Done and the saves.
    fn top_bar(&self, modified: bool, cx: &mut Context<Self>) -> gpui::Div {
        let palette = Theme::global(cx).palette;
        // The notice fills the bar's free stretch, which moves the window.
        let notice = title_bar::drag_area()
            .px(rem(space::XS))
            .text_size(rem(text::XS))
            .truncate()
            .when_some(self.notice.as_ref(), |line, notice| {
                line.text_color(if notice.error {
                    palette.danger.to_gpui()
                } else {
                    palette.accent_text.to_gpui()
                })
                .child(notice.text.clone())
            });
        let done = cx.entity();
        let leave = move |_: &mut Window, cx: &mut App| {
            done.update(cx, |editor, cx| {
                if editor.proceed_or_ask(Pending::Leave, cx) {
                    cx.emit(EditorEvent::Leave);
                }
            })
        };
        // "Bowser / Red" for a slot of the game, which Done goes back to;
        // the file's name otherwise.
        let crumbs = match self.slot.as_deref().and_then(slot_parts) {
            Some((fighter, color)) if self.home() => Breadcrumbs::new()
                .level(fighter, {
                    let leave = leave.clone();
                    move |window, cx| leave(window, cx)
                })
                .page(color),
            _ => Breadcrumbs::new().page(self.name.clone()),
        };
        let toggle = |kind: PaneKind,
                      icon: IconName,
                      label: &'static str,
                      cx: &mut Context<Self>| {
            let this = cx.entity();
            IconButton::new(("pane-toggle", kind as usize), icon, label)
                .pressed(self.pane(kind).open)
                .on_press(move |_, cx| this.update(cx, |editor, cx| editor.toggle_pane(kind, cx)))
        };
        let textures_toggle = toggle(PaneKind::Textures, IconName::Textures, "Textures", cx);
        let texture_toggle = toggle(PaneKind::Texture, IconName::Texture, "Texture", cx);
        let colors_toggle = toggle(PaneKind::Colors, IconName::Colors, "Colors", cx);
        let moves_toggle = (!self.moves.rows.is_empty())
            .then(|| toggle(PaneKind::Moves, IconName::Moves, "Moves", cx));
        // Where the platform has a menu bar, these live there instead.
        let more = (!cfg!(target_os = "macos")).then(|| self.more_menu(cx));
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(rem(space::XS))
            .h(rem(title_bar::HEIGHT))
            .pl(rem(if self.clear_toggle {
                title_bar::CLEARANCE
            } else {
                space::MD
            }))
            .pr(rem(space::SM))
            .bg(palette.bg.to_gpui())
            .child(
                div()
                    .flex_none()
                    .text_size(rem(density::CONTROL_TEXT + 0.5))
                    .whitespace_nowrap()
                    .child(crumbs),
            )
            .when(modified, |bar| {
                bar.child(
                    div()
                        .flex_none()
                        .px(rem(space::XS))
                        .rounded(rem(radius::PILL))
                        .border_1()
                        .border_color(palette.line.to_gpui())
                        .bg(palette.raise.to_gpui())
                        .font_family(font::MONO)
                        .text_size(rem(11.0))
                        .text_color(palette.accent_text.to_gpui())
                        .child("Unsaved"),
                )
            })
            .child(notice)
            .child(
                div()
                    .flex_none()
                    .flex()
                    .gap(rem(2.0))
                    .child(textures_toggle)
                    .child(texture_toggle)
                    .child(colors_toggle)
                    .children(moves_toggle)
                    .children(more),
            )
            .child(
                div()
                    .flex_none()
                    .w(gpui::px(1.0))
                    .h(rem(18.0))
                    .mx(rem(space::XXS))
                    .bg(palette.line.to_gpui()),
            )
            .when(self.home(), |bar| {
                bar.child(
                    Button::new("done", "Done")
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .on_press(leave.clone()),
                )
            })
            .map(|bar| {
                if self.slot.is_some() {
                    bar.child(
                        Button::new("save", "Save")
                            .size(ButtonSize::Sm)
                            .disabled(!modified)
                            .tooltip(
                                Tooltip::new("Keep this version in your library")
                                    .shortcut(shortcut(keys::SAVE)),
                            )
                            .on_press(dispatch(Save)),
                    )
                    .child(
                        Button::new("save-install", "Save and install")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Sm)
                            .disabled(!modified)
                            .tooltip(
                                Tooltip::new("Save, then put it into your game's slot")
                                    .shortcut(shortcut(keys::SAVE_AND_INSTALL)),
                            )
                            .on_press(dispatch(SaveAndInstall)),
                    )
                } else {
                    bar.child(
                        Button::new("save", "Save")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Sm)
                            .disabled(!modified)
                            .tooltip(Tooltip::new("Save the file").shortcut(shortcut(keys::SAVE)))
                            .on_press(dispatch(Save)),
                    )
                }
            })
    }

    /// Everything else the editor does: files, history, and the view.
    fn more_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (can_undo, can_redo) = self.document.as_ref().map_or((false, false), |document| {
            (document.can_undo(), document.can_redo())
        });
        let current = Theme::global(cx).appearance;
        let highlight = self.highlights_selection();
        let (paused, animated) = {
            let viewport = self.viewport.read(cx);
            (viewport.is_paused(), viewport.is_animated())
        };
        let option = |label: &'static str, appearance: Appearance| {
            let this = cx.entity();
            MenuItem::new(label, move |window, cx| {
                this.update(cx, |editor, cx| {
                    editor.set_appearance(appearance, window, cx)
                })
            })
            .checked(current == appearance)
        };
        let viewport = self.viewport.clone();
        let this = cx.entity();
        MenuButton::new("more-menu", "More")
            .variant(ButtonVariant::Ghost)
            .size(ButtonSize::Sm)
            .item(
                MenuItem::new("Undo", dispatch(Undo))
                    .shortcut(shortcut(keys::UNDO))
                    .disabled(!can_undo),
            )
            .item(
                MenuItem::new("Redo", dispatch(Redo))
                    .shortcut(shortcut(keys::REDO))
                    .disabled(!can_redo),
            )
            .separator()
            .item(MenuItem::new("Open DAT…", dispatch(OpenDat)).shortcut(shortcut(keys::OPEN)))
            .item(MenuItem::new("Save As…", dispatch(SaveAs)).shortcut(shortcut(keys::SAVE_AS)))
            .item(
                MenuItem::new("Import PNG…", dispatch(ImportPng)).shortcut(shortcut(keys::IMPORT)),
            )
            .item(
                MenuItem::new("Export PNG…", dispatch(ExportPng)).shortcut(shortcut(keys::EXPORT)),
            )
            .separator()
            .item(
                MenuItem::new("Highlight selection", move |_, cx| {
                    this.update(cx, |editor, cx| editor.toggle_highlight(cx))
                })
                .checked(highlight),
            )
            .item(MenuItem::new("Reset camera", move |_, cx| {
                viewport.update(cx, |viewport, cx| viewport.reset_camera(cx))
            }))
            .item(
                MenuItem::new(
                    if paused {
                        "Play animation"
                    } else {
                        "Pause animation"
                    },
                    dispatch(TogglePlayback),
                )
                .shortcut(shortcut(keys::PLAYBACK))
                .disabled(!animated),
            )
            .separator()
            .item(option("Gallery palette", Appearance::Gallery))
            .item(option("Paper palette", Appearance::Paper))
    }
}

/// "Bowser" and "Red" for `PlKpRe.dat`.
fn slot_parts(slot: &str) -> Option<(String, String)> {
    let label = crate::costumes::slot_label(Some(slot));
    let (fighter, color) = label.split_once(" · ")?;
    Some((fighter.to_owned(), color.to_owned()))
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let modified = self.is_modified();
        let top_bar = self.top_bar(modified, cx);
        let panes = self.panes(window, cx);
        let width = self.split.read(cx).right.width;
        // The stage keeps clear of the column and its margins.
        let clear = if panes.is_empty() {
            0.0
        } else {
            width + PANE_MARGIN * 2.0
        };
        let stage = stage::stage(&self.viewport, &self.timeline, rem(clear)).child(
            div()
                .absolute()
                .top(rem(space::SM))
                .left(rem(space::MD))
                .max_w(rem(480.0))
                .opacity(0.7)
                .child(self.status.clone()),
        );
        let this = cx.entity();
        let answer = move |answer: Unsaved| {
            let this = this.clone();
            move |window: &mut Window, cx: &mut App| {
                this.update(cx, |editor, cx| editor.resolve_unsaved(answer, window, cx))
            }
        };

        tgg_ui::focus_navigation(div().id("editor"))
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.bg.to_gpui())
            .text_color(palette.text.to_gpui())
            .font_family(font::SANS)
            // A dragged file rings the window in the accent, so it's clear
            // a drop is accepted.
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style
                    .inset_ring(gpui::px(2.0))
                    .inset_ring_color(palette.accent_text.to_gpui())
            })
            .on_drop(cx.listener(|editor, paths: &ExternalPaths, _, cx| {
                editor.drop_paths(paths.paths(), cx)
            }))
            .on_action(cx.listener(|editor, _: &OpenDat, window, cx| editor.open_dat(window, cx)))
            .on_action(cx.listener(|editor, _: &Save, window, cx| editor.save(window, cx)))
            .on_action(cx.listener(|editor, _: &SaveAs, window, cx| editor.save_as(window, cx)))
            .on_action(
                cx.listener(|editor, _: &ImportPng, window, cx| editor.import_png(window, cx)),
            )
            .on_action(
                cx.listener(|editor, _: &ExportPng, window, cx| editor.export_png(window, cx)),
            )
            .on_action(cx.listener(|editor, _: &EditExternally, _, cx| editor.edit_externally(cx)))
            .on_action(cx.listener(|editor, _: &Undo, _, cx| editor.step_history(false, cx)))
            .on_action(cx.listener(|editor, _: &TogglePlayback, _, cx| {
                editor
                    .viewport
                    .update(cx, |viewport, cx| viewport.toggle_playback(cx));
                cx.notify();
            }))
            .on_action(cx.listener(|editor, _: &Redo, _, cx| editor.step_history(true, cx)))
            .when(self.slot.is_some(), |root| {
                root.on_action(cx.listener(|editor, _: &SaveAndInstall, _, cx| {
                    editor.save_to_library(true, None, cx)
                }))
            })
            .on_action(cx.listener(|editor, _: &ResetCamera, _, cx| {
                editor
                    .viewport
                    .update(cx, |viewport, cx| viewport.reset_camera(cx))
            }))
            .on_action(
                cx.listener(|editor, _: &ToggleHighlight, _, cx| editor.toggle_highlight(cx)),
            )
            .on_action(cx.listener(|editor, _: &ToggleTexturesPane, _, cx| {
                editor.toggle_pane(PaneKind::Textures, cx)
            }))
            .on_action(cx.listener(|editor, _: &ToggleTexturePane, _, cx| {
                editor.toggle_pane(PaneKind::Texture, cx)
            }))
            .when(!self.moves.rows.is_empty(), |root| {
                root.on_action(cx.listener(|editor, _: &ToggleMovesPane, _, cx| {
                    editor.toggle_pane(PaneKind::Moves, cx)
                }))
            })
            .child(top_bar)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(stage)
                    .when(!panes.is_empty(), |body| {
                        body.child(PaneColumn::new(self.split.clone(), PANE_MARGIN).children(panes))
                    }),
            )
            .when_some(self.unsaved.as_ref(), |root, pending| {
                let description = match pending {
                    Pending::Close => "Your texture edits are lost if you close without saving.",
                    Pending::Leave => "Your texture edits are lost if you go back without saving.",
                    Pending::Open(_) => {
                        "Your texture edits are lost if you open another file without saving."
                    }
                };
                root.child(
                    Dialog::new(
                        "unsaved",
                        format!("Save changes to {}?", self.name),
                        answer(Unsaved::Cancel),
                    )
                    .description(description)
                    .action("Cancel", ButtonVariant::Ghost, answer(Unsaved::Cancel))
                    .action(
                        "Don't save",
                        ButtonVariant::Danger,
                        answer(Unsaved::Discard),
                    )
                    .action(
                        "Save",
                        ButtonVariant::Primary,
                        answer(Unsaved::Save),
                    ),
                )
            })
    }
}
