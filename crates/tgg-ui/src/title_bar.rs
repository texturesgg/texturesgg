//! The window's title bar, drawn by the app. On macOS the platform's bar is
//! transparent, so the app's top row (the sidebar's head beside each page's
//! bar) is the title bar: the window buttons sit at its left, the sidebar
//! toggle just after them, and the row's empty stretches move the window.
//! Elsewhere the menu button takes the window buttons' place.

use crate::tokens::{density, space};
use gpui::prelude::FluentBuilder;
use gpui::{InteractiveElement, MouseButton, SharedString, Styled, TitlebarOptions, Window, div};

/// The top row's height, in web pixels: every bar in the app is this tall,
/// so nothing moves between screens.
pub const HEIGHT: f32 = density::CONTROL_MD + space::XS * 2.0;

/// Whether the app draws the title bar (macOS); elsewhere the platform does.
pub const DRAWN: bool = cfg!(target_os = "macos");

/// Where the window buttons start from the window's left edge, in points.
const BUTTONS_LEFT: f32 = 16.0;

/// A window button's height in points, to center the buttons in the row.
const BUTTON_HEIGHT: f32 = 14.0;

/// How much of the top row the window buttons cover from the window's left
/// edge, in points: none where the platform draws the title bar.
pub const BUTTONS_WIDTH: f32 = if DRAWN { 78.0 } else { 0.0 };

/// Whether the app's menus sit in a button at the top row's left, where
/// the platform has no menu bar (all but macOS).
pub const MENU_BUTTON: bool = !cfg!(target_os = "macos");

/// Where the menu button sits from the window's left edge, in web pixels.
pub const MENU_LEFT: f32 = space::XS;

/// Where the sidebar toggle sits from the window's left edge, in web
/// pixels: just after the window buttons or the menu button, in both of
/// the sidebar's states.
pub const TOGGLE_LEFT: f32 = BUTTONS_WIDTH
    + if DRAWN { 4.0 } else { space::XS }
    + if MENU_BUTTON {
        density::CONTROL_MD + space::XXS
    } else {
        0.0
    };

/// How far a bar starts from the window's left edge while the sidebar is
/// hidden, in web pixels: clear of the window buttons and the toggle.
pub const CLEARANCE: f32 = TOGGLE_LEFT + density::CONTROL_MD + space::XS;

/// The window's title bar options: transparent where the app draws it.
pub fn options(title: impl Into<SharedString>) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: DRAWN,
        // At the reference rem size; `place_buttons` follows the UI's scale.
        traffic_light_position: DRAWN.then(|| {
            gpui::point(
                gpui::px(BUTTONS_LEFT),
                gpui::px((HEIGHT - BUTTON_HEIGHT) / 2.0),
            )
        }),
    }
}

/// Center the window buttons in the top row at the window's rem size, so
/// they follow the UI's scale. [`crate::Theme::apply`] calls it.
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub fn place_buttons(window: &mut Window) {
    // gpui only has the call on macOS, so a runtime `DRAWN` branch would
    // not compile elsewhere.
    #[cfg(target_os = "macos")]
    {
        let height = f32::from(crate::rem(HEIGHT).to_pixels(window.rem_size()));
        window.set_traffic_light_position(gpui::point(
            gpui::px(BUTTONS_LEFT),
            gpui::px(((height - BUTTON_HEIGHT) / 2.0).max(0.0)),
        ));
    }
}

/// An empty stretch of the top row that moves the window, and zooms it on a
/// double-click, as the platform's title bar would. Only the stretch moves
/// the window, so presses on the controls beside it stay theirs.
pub fn drag_area() -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .when(DRAWN, |area| {
            area.on_mouse_down(MouseButton::Left, |event, window, _| {
                if event.click_count == 2 {
                    window.titlebar_double_click();
                } else {
                    window.start_window_move();
                }
            })
        })
}
