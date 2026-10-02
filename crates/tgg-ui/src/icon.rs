//! Icons: outline glyphs on a 24 px grid, 1.75 px strokes with round ends,
//! drawn for textures.gg and built into the binary, in the color given (the
//! theme's text color by default).

use crate::{Color, Theme};
use gpui::{App, IntoElement, Rems, RenderOnce, Styled, Window, svg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    /// The player's game: a controller.
    Game,
    /// Their skins: books on a shelf.
    Library,
    /// textures.gg online.
    Globe,
    /// Settings: two sliders.
    Settings,
    /// Show or hide the sidebar.
    Sidebar,
    /// A costume's textures: a grid.
    Textures,
    /// One texture: a picture.
    Texture,
    /// A fighter's moves: a list.
    Moves,
    /// Colors outside textures: a painter's palette.
    Colors,
    Close,
    ChevronDown,
    ChevronRight,
    Plus,
    /// Bring a file in.
    Import,
    /// Send a file out.
    Export,
    /// More actions.
    More,
    /// The app's menu, where the platform has no menu bar.
    Menu,
}

impl IconName {
    fn svg(self) -> &'static [u8] {
        match self {
            Self::Game => include_bytes!("../icons/game.svg"),
            Self::Library => include_bytes!("../icons/library.svg"),
            Self::Globe => include_bytes!("../icons/globe.svg"),
            Self::Settings => include_bytes!("../icons/settings.svg"),
            Self::Sidebar => include_bytes!("../icons/sidebar.svg"),
            Self::Textures => include_bytes!("../icons/textures.svg"),
            Self::Texture => include_bytes!("../icons/texture.svg"),
            Self::Moves => include_bytes!("../icons/moves.svg"),
            Self::Colors => include_bytes!("../icons/colors.svg"),
            Self::Close => include_bytes!("../icons/close.svg"),
            Self::ChevronDown => include_bytes!("../icons/chevron-down.svg"),
            Self::ChevronRight => include_bytes!("../icons/chevron-right.svg"),
            Self::Plus => include_bytes!("../icons/plus.svg"),
            Self::Import => include_bytes!("../icons/import.svg"),
            Self::Export => include_bytes!("../icons/export.svg"),
            Self::More => include_bytes!("../icons/more.svg"),
            Self::Menu => include_bytes!("../icons/menu.svg"),
        }
    }
}

#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Rems,
    color: Option<Color>,
}

impl Icon {
    /// `name` at 18 web pixels, the size beside control text.
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: crate::rem(18.0),
            color: None,
        }
    }

    pub fn size(mut self, size: Rems) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        // An svg paints only in its own text color; it doesn't inherit one.
        let color = self.color.unwrap_or(Theme::global(cx).palette.text);
        svg()
            .data(self.name.svg())
            .size(self.size)
            .flex_none()
            .text_color(color.to_gpui())
    }
}
