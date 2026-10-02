//! A square button showing only an icon, for toolbars and pane headers. It
//! names itself in a tooltip, and a toggle shows whether it's on.

use crate::icon::{Icon, IconName};
use crate::tokens::{density, radius};
use crate::{Theme, Tooltip, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, FocusHandle, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window,
};
use std::rc::Rc;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    icon: IconName,
    label: SharedString,
    shortcut: Option<SharedString>,
    /// A toggle's state; `None` for a plain button.
    pressed: Option<bool>,
    small: bool,
    focus: Option<FocusHandle>,
    on_press: Option<PressHandler>,
}

impl IconButton {
    /// A button showing `icon`, named `label` in its tooltip.
    pub fn new(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            icon,
            label: label.into(),
            shortcut: None,
            pressed: None,
            small: false,
            focus: None,
            on_press: None,
        }
    }

    /// Show whether the toggle is on.
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = Some(pressed);
        self
    }

    /// The small size, for pane headers.
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }

    pub fn shortcut(mut self, keys: impl Into<SharedString>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }

    /// Track the caller's focus handle, so the caller can move focus here.
    pub fn focus_handle(mut self, handle: FocusHandle) -> Self {
        self.focus = Some(handle);
        self
    }

    pub fn on_press(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_press = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let (size, icon) = if self.small {
            (density::CONTROL_SM - 2.0, 14.0)
        } else {
            (density::CONTROL_MD, 16.0)
        };
        let on = self.pressed == Some(true);
        let label = self.label.clone();
        let shortcut = self.shortcut.clone();
        match self.focus {
            Some(focus) => crate::pressable::pressable_with(self.id, focus, self.on_press, cx),
            None => crate::pressable::pressable(self.id, self.on_press, window, cx),
        }
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(rem(size))
        .rounded(rem(radius::SM))
        .when(on, |button| {
            button
                .bg(palette.raise.to_gpui())
                .border_1()
                .border_color(palette.line_strong.to_gpui())
        })
        .hover(|style| style.bg(palette.raise.to_gpui()))
        .tooltip(move |_, cx| {
            let tooltip = Tooltip::new(label.clone());
            match &shortcut {
                Some(keys) => tooltip.shortcut(keys.clone()),
                None => tooltip,
            }
            .view(cx)
        })
        .child(Icon::new(self.icon).size(rem(icon)).color(if on {
            palette.text
        } else {
            palette.muted
        }))
    }
}
