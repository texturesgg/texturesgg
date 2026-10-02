//! Design tokens, copied from the textures.gg site's design system.
//!
//! The site is the source. Names follow its tokens in snake_case, and lengths
//! keep their web pixel values. The site's tests read this file and fail when
//! a value stops matching, so keep each token a literal on its own line.

/// An sRGB color with straight alpha, packed as `0xRRGGBBAA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    /// Fully transparent.
    pub const CLEAR: Self = Self(0);

    /// An opaque color from `0xRRGGBB`.
    pub const fn rgb(hex: u32) -> Self {
        Self(hex << 8 | 0xff)
    }

    /// A color from 8-bit channels and a 0–1 alpha, as CSS `rgba()` writes it.
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: f32) -> Self {
        let alpha = (alpha * 255.0 + 0.5) as u32;
        Self((red as u32) << 24 | (green as u32) << 16 | (blue as u32) << 8 | alpha)
    }
}

impl Color {
    /// The gpui color, for `bg`, `text_color`, `border_color`, and the like.
    /// gpui-ce takes colors through `palette`'s conversion traits, which a
    /// plain `From` impl can't satisfy.
    pub fn to_gpui(self) -> gpui::Rgba {
        gpui::rgba(self.0)
    }
}

/// A drop shadow: vertical offset and blur radius in pixels, and its color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub offset_y: f32,
    pub blur: f32,
    pub color: Color,
}

impl Shadow {
    /// The gpui shadow, for `.shadow(vec![...])`.
    pub fn to_gpui(self) -> gpui::BoxShadow {
        gpui::BoxShadow {
            color: self.color.to_gpui().into(),
            offset: gpui::point(gpui::px(0.0), gpui::px(self.offset_y)),
            blur_radius: gpui::px(self.blur),
            spread_radius: gpui::px(0.0),
            inset: false,
        }
    }
}

/// One palette's colors (the web's `color` group) and its popover shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub raise: Color,
    pub text: Color,
    pub muted: Color,
    pub line: Color,
    pub line_strong: Color,
    /// A fill: primary actions, checked controls, progress. Never text or a
    /// line on its own.
    pub accent: Color,
    /// Accent-colored text and lines: links, focus rings, active underlines.
    pub accent_text: Color,
    pub on_accent: Color,
    pub danger: Color,
    /// Text and icons on a danger fill.
    pub on_danger: Color,
    pub success: Color,
    pub scrim: Color,
    pub popover_shadow: Shadow,
}

/// The dark default palette.
pub const GALLERY: Palette = Palette {
    bg: Color::rgb(0x111110),
    surface: Color::rgb(0x1B1B19),
    raise: Color::rgb(0x262522),
    text: Color::rgb(0xF2EFE8),
    muted: Color::rgb(0xA8A39A),
    line: Color::rgb(0x2F2E2A),
    line_strong: Color::rgb(0x3A3934),
    accent: Color::rgb(0xF5C84B),
    accent_text: Color::rgb(0xF5C84B),
    on_accent: Color::rgb(0x111110),
    danger: Color::rgb(0xF07167),
    on_danger: Color::rgb(0x111110),
    success: Color::rgb(0x7BD88F),
    scrim: Color::rgba(0, 0, 0, 0.6),
    popover_shadow: Shadow {
        offset_y: 24.0,
        blur: 60.0,
        color: Color::rgba(0, 0, 0, 0.55),
    },
};

/// The light palette; surfaces lighten toward the viewer.
pub const PAPER: Palette = Palette {
    bg: Color::rgb(0xF3EFE6),
    surface: Color::rgb(0xFAF7F0),
    raise: Color::rgb(0xFFFDF8),
    text: Color::rgb(0x1C1B19),
    muted: Color::rgb(0x5C574E),
    line: Color::rgb(0xE0D9CA),
    line_strong: Color::rgb(0xB8AF9C),
    accent: Color::rgb(0xF5C84B),
    accent_text: Color::rgb(0x7A5A00),
    on_accent: Color::rgb(0x1C1B19),
    danger: Color::rgb(0xB42E24),
    on_danger: Color::rgb(0xFFFDF8),
    success: Color::rgb(0x27713B),
    scrim: Color::rgba(28, 27, 25, 0.35),
    popover_shadow: Shadow {
        offset_y: 24.0,
        blur: 60.0,
        color: Color::rgba(28, 27, 25, 0.18),
    },
};

/// Spacing in web pixels (`space`).
pub mod space {
    pub const XXS: f32 = 4.0;
    pub const XS: f32 = 8.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 16.0;
    pub const LG: f32 = 24.0;
    pub const XL: f32 = 32.0;
    pub const XXL: f32 = 48.0;
    pub const XXXL: f32 = 64.0;
}

/// Corner radii in web pixels (`radius`). Anything pressed is a pill,
/// anything typed in is `MD`, surfaces use `MD` and up.
pub mod radius {
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 20.0;
    pub const PILL: f32 = 999.0;
}

/// Type sizes in web pixels (`text`).
pub mod text {
    pub const XS: f32 = 13.0;
    pub const SM: f32 = 14.0;
    pub const MD: f32 = 15.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 18.0;
    pub const H3: f32 = 24.0;
    pub const H2: f32 = 28.0;
    pub const H1: f32 = 40.0;
    pub const DISPLAY: f32 = 56.0;
    pub const HERO: f32 = 96.0;
}

/// Letter spacing in em (`tracking`); multiply by the font size to apply.
pub mod tracking {
    pub const NORMAL: f32 = 0.0;
    pub const TIGHT: f32 = -0.02;
    pub const TIGHTER: f32 = -0.045;
}

/// Motion (`motion`): durations in milliseconds and the cubic-bezier easing.
pub mod motion {
    pub const FAST_MS: u64 = 120;
    pub const NORMAL_MS: u64 = 200;
    pub const EASING: [f32; 4] = [0.2, 0.0, 0.0, 1.0];
}

/// Font families (`font`).
pub mod font {
    pub const SANS: &str = "Schibsted Grotesk";
    pub const MONO: &str = "DM Mono";
}

/// The order floating elements stack in (gpui `deferred` priorities): a
/// menu opened from a dialog must draw above it.
pub mod layer {
    pub const DIALOG: usize = 2;
    pub const MENU: usize = 3;
}

/// Desktop density, in web pixels: the editor is dense where the site is
/// sized for touch (44 px controls, 15–16 px text). Desktop-only, so the site's
/// test doesn't compare these with the web.
pub mod density {
    /// Small controls: toolbar buttons, compact rows.
    pub const CONTROL_SM: f32 = 24.0;
    /// Default control height.
    pub const CONTROL_MD: f32 = 28.0;
    /// Control label size.
    pub const CONTROL_TEXT: f32 = 12.5;
    /// Secondary lines under a label: sizes, formats, counts.
    pub const DETAIL_TEXT: f32 = 11.5;
    /// Width of a resize handle's hit area; the drawn line stays 1 px.
    pub const HANDLE_HIT: f32 = 9.0;
}
