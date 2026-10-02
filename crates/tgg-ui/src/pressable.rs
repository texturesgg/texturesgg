//! What makes anything pressable, the same way everywhere: a focus handle
//! and a tab stop, so Tab reaches it; the accent focus ring when the
//! keyboard put focus there; and a press on click. gpui-ce turns Enter and
//! Space on the focused element into a click, so the keyboard presses it
//! too. Every clickable component builds on this rather than on a bare
//! `on_click`, so none is out of the keyboard's reach.

use crate::Theme;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, Div, ElementId, FocusHandle, InteractiveElement, Stateful, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use std::rc::Rc;

type PressHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A focus handle that lasts while an element with `id` is drawn, and is a
/// tab stop.
pub fn focus_handle(id: &ElementId, window: &mut Window, cx: &mut App) -> FocusHandle {
    tab_stop(
        &window
            .use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone(),
    )
}

/// `handle`, as a tab stop. gpui-ce takes an element's place in the tab
/// order from the handle it tracks, not from the element's `tab_index`: a
/// tracked handle that isn't a tab stop is never reached by Tab. Every
/// element meant for the keyboard tracks its handle through this.
pub fn tab_stop(handle: &FocusHandle) -> FocusHandle {
    handle.clone().tab_stop(true)
}

/// A pressable element: focusable, ringed on keyboard focus, and calling
/// `on_press` on a click, Enter, or Space. Without `on_press` it's still a
/// tab stop, for elements whose children do the pressing.
pub fn pressable(
    id: impl Into<ElementId>,
    on_press: Option<PressHandler>,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id = id.into();
    let focus = focus_handle(&id, window, cx);
    pressable_with(id, focus, on_press, cx)
}

/// [`pressable`], with the caller's focus handle, so the caller can move
/// focus to it.
pub fn pressable_with(
    id: impl Into<ElementId>,
    focus: FocusHandle,
    on_press: Option<PressHandler>,
    cx: &App,
) -> Stateful<Div> {
    let palette = Theme::global(cx).palette;
    div()
        .id(id)
        .track_focus(&tab_stop(&focus))
        .tab_index(0)
        .focus_visible(|style| {
            style
                .ring(px(2.0))
                .ring_color(palette.accent_text.to_gpui())
        })
        .when_some(on_press, |element, press| {
            element
                .cursor_pointer()
                .on_click(move |_, window, cx| press(window, cx))
        })
}
