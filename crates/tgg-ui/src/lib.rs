//! The editor's theme and components on gpui-ce, in the web's design language.
//!
//! Tokens copy the web design system; [`Theme`] carries the active palette as
//! a gpui global. Sizes are web pixels expressed in rems against a 16 px
//! reference, so changing the window's rem size zooms the whole UI while
//! operating-system display scaling stays separate.

pub mod breadcrumbs;
pub mod button;
pub mod card;
pub mod chip;
pub mod dialog;
pub mod fonts;
pub mod icon;
pub mod icon_button;
pub mod keys;
pub mod menu;
pub mod option_list;
pub mod page_header;
pub mod pane;
pub mod pressable;
pub mod sidebar;
pub mod slider;
pub mod split;
pub mod tabs;
pub mod thumbnail_list;
pub mod title_bar;
pub mod tokens;
pub mod tooltip;

pub use breadcrumbs::Breadcrumbs;
pub use button::{Button, ButtonSize, ButtonVariant};
pub use card::{Card, CardLayout, CardStatus};
pub use chip::Chip;
pub use dialog::Dialog;
use gpui::{App, Global, InteractiveElement, KeyBinding, Pixels, Rems, Window, actions, px, rems};
pub use icon::{Icon, IconName};
pub use icon_button::IconButton;
pub use keys::shortcut;
pub use menu::{MenuButton, MenuItem, Submenu};
pub use option_list::{OptionList, OptionRow};
pub use page_header::PageHeader;
pub use pane::{Pane, PaneColumn, PaneFit};
pub use pressable::pressable;
pub use sidebar::{Sidebar, SidebarEntry};
pub use slider::Slider;
pub use split::{PaneSize, Split, SplitState};
pub use tabs::Tabs;
pub use thumbnail_list::{
    ThumbnailItem, ThumbnailList, checkerboard, drop_images, over_checkerboard, render_image,
};
pub use tokens::{Color, GALLERY, PAPER, Palette, Shadow};
pub use tooltip::Tooltip;

/// The web's root font size, against which its pixel tokens are authored.
const REFERENCE_REM: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Gallery,
    Paper,
}

/// The active palette and UI scale.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub appearance: Appearance,
    pub palette: Palette,
    /// The window's rem size. 16 px renders tokens at their web size; users
    /// change this to zoom the UI.
    pub ui_scale: Pixels,
}

impl Global for Theme {}

impl Theme {
    pub fn new(appearance: Appearance) -> Self {
        Self {
            appearance,
            palette: match appearance {
                Appearance::Gallery => GALLERY,
                Appearance::Paper => PAPER,
            },
            ui_scale: px(REFERENCE_REM),
        }
    }

    /// Install the theme for the app.
    pub fn init(appearance: Appearance, cx: &mut App) {
        cx.set_global(Self::new(appearance));
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Size a window's rems from the theme; call when opening a window and
    /// after changing `ui_scale`.
    pub fn apply(&self, window: &mut Window) {
        window.set_rem_size(self.ui_scale);
        title_bar::place_buttons(window);
    }
}

actions!(tgg_ui, [FocusNext, FocusPrevious]);

/// Bind the keys tgg-ui components rely on. Call once at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
}

/// Let Tab and Shift-Tab move between focusable components; apply to each
/// window's root element.
pub fn focus_navigation<E: InteractiveElement>(root: E) -> E {
    root.on_action(|_: &FocusNext, window, cx| window.focus_next(cx))
        .on_action(|_: &FocusPrevious, window, cx| window.focus_prev(cx))
}

/// A web pixel token as rems, so it follows the UI scale.
pub fn rem(web_px: f32) -> Rems {
    rems(web_px / REFERENCE_REM)
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn css_rgba_rounds_alpha_to_eight_bits() {
        assert_eq!(Color::rgba(0, 0, 0, 0.6), Color(0x0000_0099));
        assert_eq!(Color::rgba(28, 27, 25, 0.35), Color(0x1C1B_1959));
        assert_eq!(Color::rgb(0xF5C84B), Color(0xF5C8_4BFF));
    }
}
