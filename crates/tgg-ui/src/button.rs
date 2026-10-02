//! A pressable pill, in the web `Button`'s variants at desktop density.

use crate::tokens::{density, radius, space};
use crate::tooltip::Tooltip;
use crate::{Color, Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement, ParentElement,
    PathBuilder, RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, canvas, div,
    point, px,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    /// The one main action in a view: an accent fill.
    Primary,
    /// The default: a pill in a quiet fill.
    #[default]
    Secondary,
    /// No box until hovered, for toolbars and dense rows.
    Ghost,
    /// Confirms a destructive action; the label names the action.
    Danger,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
}

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A button. Keyboard focus shows the `accent_text` ring; Enter and Space
/// press it like a click.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    disabled: bool,
    on_press: Option<PressHandler>,
    focus: Option<FocusHandle>,
    tooltip: Option<Tooltip>,
    /// Show a chevron after the label: the button opens a choice.
    chevron: bool,
    /// Fill the parent's width, truncating the label to fit.
    fill: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            disabled: false,
            on_press: None,
            focus: None,
            tooltip: None,
            chevron: false,
            fill: false,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }

    /// Use the caller's focus handle, so the caller can move focus back to
    /// the button (a menu closing returns focus to its trigger).
    pub fn focus_handle(mut self, focus: FocusHandle) -> Self {
        self.focus = Some(focus);
        self
    }

    /// Show a tooltip on hover.
    /// End the label with a chevron, as a select's trigger does.
    pub fn chevron(mut self) -> Self {
        self.chevron = true;
        self
    }

    /// Fill the parent's width, truncating the label to fit, with any
    /// chevron at the far end.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn tooltip(mut self, tooltip: Tooltip) -> Self {
        self.tooltip = Some(tooltip);
        self
    }
}

impl RenderOnce for Button {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let focus = match self.focus {
            Some(focus) => focus,
            None => window
                .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone(),
        };
        let (background, hover_background, border, text) = match self.variant {
            ButtonVariant::Primary => (
                palette.accent,
                palette.accent,
                palette.accent,
                palette.on_accent,
            ),
            // Filled, not outlined: a quiet fill that deepens on hover.
            ButtonVariant::Secondary => (
                palette.line,
                palette.line_strong,
                Color::CLEAR,
                palette.text,
            ),
            ButtonVariant::Ghost => (Color::CLEAR, palette.line, Color::CLEAR, palette.text),
            ButtonVariant::Danger => (
                palette.danger,
                palette.danger,
                palette.danger,
                palette.on_danger,
            ),
        };
        let (height, padding) = match (self.size, self.chevron) {
            (ButtonSize::Sm, false) => (density::CONTROL_SM, space::SM - 2.0),
            (ButtonSize::Md, false) => (density::CONTROL_MD, space::SM + 2.0),
            // A select's trigger, as the web's: roomier, label and chevron.
            (ButtonSize::Sm, true) => (density::CONTROL_SM, space::SM),
            (ButtonSize::Md, true) => (density::CONTROL_MD, space::MD),
        };

        // gpui-ce turns Enter and Space on the focused element into a click.
        let press = self.on_press.filter(|_| !self.disabled);
        div()
            .id(self.id)
            .track_focus(&crate::pressable::tab_stop(&focus))
            .tab_index(0)
            .flex()
            .flex_none()
            .items_center()
            .map(|button| {
                if self.fill {
                    button.w_full().justify_between()
                } else {
                    button.justify_center()
                }
            })
            .h(rem(height))
            .px(rem(padding))
            .rounded(rem(radius::PILL))
            .border_1()
            .border_color(border.to_gpui())
            .bg(background.to_gpui())
            .text_color(text.to_gpui())
            .text_size(rem(density::CONTROL_TEXT))
            .font_weight(FontWeight::NORMAL)
            .whitespace_nowrap()
            .focus_visible(|style| {
                style
                    .ring(px(2.0))
                    .ring_color(palette.accent_text.to_gpui())
            })
            .when(self.disabled, |button| button.opacity(0.5))
            .when(!self.disabled, |button| {
                button
                    .cursor_pointer()
                    .hover(|style| style.bg(hover_background.to_gpui()))
            })
            .when_some(press, |button, press| {
                button.on_click(move |_, window, cx| press(window, cx))
            })
            .when_some(self.tooltip, |button, tooltip| {
                button.tooltip(move |_, cx| tooltip.clone().view(cx))
            })
            .map(|button| {
                if self.fill {
                    button.child(div().min_w_0().truncate().child(self.label))
                } else {
                    button.child(self.label)
                }
            })
            .when(self.chevron, |button| {
                button.gap(rem(space::XS)).child(chevron(text))
            })
    }
}

/// A small downward chevron in `color`, drawn as a stroked path.
fn chevron(color: Color) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let (width, height) = (bounds.size.width, bounds.size.height);
            let mut path = PathBuilder::stroke(px(1.5));
            path.move_to(bounds.origin + point(width * 0.1, height * 0.3));
            path.line_to(bounds.origin + point(width * 0.5, height * 0.7));
            path.line_to(bounds.origin + point(width * 0.9, height * 0.3));
            if let Ok(path) = path.build() {
                window.paint_path(path, color.to_gpui());
            }
        },
    )
    .size(rem(10.0))
    .flex_none()
}
