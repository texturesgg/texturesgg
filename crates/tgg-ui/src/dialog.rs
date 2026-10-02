//! A modal dialog: a scrim over the window and a centered panel with a title,
//! a description or custom body, and actions.
//!
//! While open it keeps keyboard focus inside: it focuses its primary (last)
//! action, Tab and Shift-Tab cycle through everything focusable in it (its
//! body's controls and its actions), and Escape or a click on the scrim
//! dismisses it. The caller decides whether it's open.

use crate::button::{Button, ButtonVariant};
use crate::tokens::{density, font, layer, radius, space, text};
use crate::{FocusNext, FocusPrevious, Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement, Styled,
    Window, anchored, deferred, div, point, px,
};
use std::rc::Rc;

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

struct DialogAction {
    label: SharedString,
    variant: ButtonVariant,
    on_press: Handler,
}

/// Focus handles for the actions, kept while the dialog stays open.
struct DialogFocus {
    /// The panel's own handle, to tell whether focus is inside it.
    panel: FocusHandle,
    actions: Vec<FocusHandle>,
    focused_on_open: bool,
}

#[derive(IntoElement)]
pub struct Dialog {
    id: ElementId,
    title: SharedString,
    description: Option<SharedString>,
    body: Option<AnyElement>,
    actions: Vec<DialogAction>,
    on_dismiss: Handler,
    /// The panel's width, in web pixels.
    width: f32,
}

impl Dialog {
    /// A dialog that calls `on_dismiss` on Escape or a scrim click.
    pub fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            body: None,
            actions: Vec::new(),
            on_dismiss: Rc::new(on_dismiss),
            width: 420.0,
        }
    }

    /// A wider panel, for a dialog holding a form such as settings.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Custom content between the description and the actions.
    pub fn body(mut self, body: impl IntoElement) -> Self {
        self.body = Some(body.into_any_element());
        self
    }

    /// Add an action. The last one is the primary action and takes focus when
    /// the dialog opens.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        variant: ButtonVariant,
        on_press: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.actions.push(DialogAction {
            label: label.into(),
            variant,
            on_press: Rc::new(on_press),
        });
        self
    }
}

/// The most tab stops a dialog steps past looking for its next one inside:
/// more than any window of this app holds.
const MAX_STEPS: usize = 256;

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let count = self.actions.len();
        let focus = window.use_keyed_state(self.id.clone(), cx, |_, cx| DialogFocus {
            panel: cx.focus_handle(),
            actions: (0..count).map(|_| cx.focus_handle()).collect(),
            focused_on_open: false,
        });
        let handles = focus.read(cx).actions.clone();
        let panel_focus = focus.read(cx).panel.clone();
        if !focus.read(cx).focused_on_open
            && let Some(primary) = handles.last().cloned()
        {
            focus.update(cx, |focus, _| focus.focused_on_open = true);
            window.defer(cx, move |window, cx| window.focus(&primary, cx));
        }

        // Step through the window's tab stops until one inside the panel.
        let step_focus = |forward: bool| {
            let panel = panel_focus.clone();
            move |window: &mut Window, cx: &mut App| {
                for _ in 0..MAX_STEPS {
                    if forward {
                        window.focus_next(cx);
                    } else {
                        window.focus_prev(cx);
                    }
                    if panel.contains_focused(window, cx) {
                        break;
                    }
                }
                cx.stop_propagation();
            }
        };
        let next = step_focus(true);
        let previous = step_focus(false);
        let escape = self.on_dismiss.clone();
        let scrim_dismiss = self.on_dismiss.clone();

        let panel = div()
            .id((self.id.clone(), "panel"))
            .track_focus(&panel_focus)
            .w(rem(self.width))
            .max_w_full()
            .p(rem(space::LG))
            .flex()
            .flex_col()
            .gap(rem(space::SM))
            .rounded(rem(radius::LG))
            .border_1()
            .border_color(palette.line_strong.to_gpui())
            .bg(palette.raise.to_gpui())
            .shadow(vec![palette.popover_shadow.to_gpui()])
            .text_color(palette.text.to_gpui())
            .font_family(font::SANS)
            // Clicks inside the panel don't reach the scrim.
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_action(move |_: &FocusNext, window, cx| next(window, cx))
            .on_action(move |_: &FocusPrevious, window, cx| previous(window, cx))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    escape(window, cx);
                }
            })
            .child(
                div()
                    .text_size(rem(text::XL))
                    .font_weight(FontWeight::BOLD)
                    .child(self.title),
            )
            .when_some(self.description, |panel, description| {
                panel.child(
                    div()
                        .text_size(rem(text::SM))
                        .text_color(palette.muted.to_gpui())
                        .child(description),
                )
            })
            .children(self.body)
            .child(
                div()
                    .mt(rem(space::XS))
                    .flex()
                    .justify_end()
                    .gap(rem(space::XS))
                    .children(
                        self.actions
                            .into_iter()
                            .zip(handles.clone())
                            .enumerate()
                            .map(|(index, (action, handle))| {
                                let press = action.on_press;
                                Button::new(("dialog-action", index), action.label)
                                    .variant(action.variant)
                                    .focus_handle(handle)
                                    .on_press(move |window, cx| press(window, cx))
                            }),
                    ),
            );

        let viewport = window.viewport_size();
        deferred(
            anchored().position(point(px(0.0), px(0.0))).child(
                div()
                    .id((self.id, "scrim"))
                    // The scrim covers the window and takes every click, so
                    // nothing beneath a modal is reachable.
                    .occlude()
                    .w(viewport.width)
                    .h(viewport.height)
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(rem(space::LG))
                    .bg(palette.scrim.to_gpui())
                    .text_size(rem(density::CONTROL_TEXT))
                    .on_click(move |_, window, cx| scrim_dismiss(window, cx))
                    .child(panel),
            ),
        )
        .priority(layer::DIALOG)
    }
}
