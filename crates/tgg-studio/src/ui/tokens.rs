//! Design tokens: the launch look (`tgg-melee-goals/studio/build-brief.md`,
//! "The shell"), dark, calm, almost no drawn borders. Surfaces are washes of
//! white over the content's near-black, so a card or a hovered row reads as
//! a lift rather than an outline.
//!
//! Colors are written as sRGB hex and converted to Rune's linear colors;
//! Rune blends in sRGB, so a 3.5% white wash looks as it does in CSS.
//! Lengths are in points, which Rune draws at the display's scale.
#![allow(dead_code, unused_imports)]

use rune::Color;
use rune::tok::{Family, TextStyle};
use tgg_app::settings::Appearance;

/// An sRGB color from `0xRRGGBB`, as the design system writes it.
pub fn rgb(hex: u32) -> Color {
    let channel = |shift: u32| rune::color::srgb_decode(((hex >> shift) & 0xff) as f32 / 255.0);
    Color {
        r: channel(16),
        g: channel(8),
        b: channel(0),
        a: 1.0,
    }
}

/// White at `alpha`: the washes surfaces are made of.
fn white(alpha: f32) -> Color {
    rgb(0xFFFFFF).alpha(alpha)
}

/// Whether the sidebar is glass: the window is see-through there and the
/// desktop shows through it. On macOS the system blurs what's behind it
/// (`SystemMaterial::Popover`); on Linux the compositor does where it blurs
/// every see-through window (Hyprland, SwayFX), and elsewhere (GNOME, KWin,
/// which blurs only windows that ask) the smoke alone shows. Windows keeps it
/// solid.
pub const GLASS: bool = cfg!(any(target_os = "macos", target_os = "linux"));

/// The sidebar's smoked glass over the blur of what's behind the window,
/// where it's glass (`GLASS`): `rgba(36,36,34,.8)`.
pub fn glass() -> Color {
    rgb(0x242422).alpha(0.8)
}

/// One palette's colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The sidebar, where it isn't glass.
    pub sunk: Color,
    /// The content, to the window's edge.
    pub bg: Color,
    /// A card, and a hovered row: a wash over `bg`.
    pub surface: Color,
    /// What floats: dialogs, menus, toasts. Opaque.
    pub raise: Color,
    pub ink: Color,
    pub muted: Color,
    pub faint: Color,
    /// A card's inner hairline.
    pub line: Color,
    /// A rule that has to show: under a dialog's head, a field's edge.
    pub line_strong: Color,
    /// Yellow, for small things only: "Using" and "In it", switches, "Has
    /// to match in netplay", a waiting dot. Never a large fill.
    pub accent: Color,
    /// Accent-colored text and lines: focus rings, selected edges.
    pub accent_text: Color,
    pub danger: Color,
    pub success: Color,
    /// Text on an ink fill (the main button).
    pub on_ink: Color,
    /// Under a hovered row: the card's wash.
    pub hover: Color,
    /// The yellow wash under a "Using" chip or a chosen choice.
    pub selected: Color,
    /// Every button but the main one.
    pub fill: Color,
    /// The sidebar row of the place showing.
    pub current: Color,
    /// Behind a model on the stage: hsd-render clears to this.
    pub plate: Color,
    /// Over the window behind a dialog.
    pub scrim: Color,
    /// A dialog's shadow.
    pub shadow: Color,
}

impl Palette {
    /// The palette for `appearance`. The app is dark only for launch
    /// (`shell::view` draws in [`Palette::gallery`]); Paper stays for later.
    pub fn of(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Gallery => Self::gallery(),
            Appearance::Paper => Self::paper(),
        }
    }

    /// The dark default.
    pub fn gallery() -> Self {
        Self {
            sunk: rgb(0x161615),
            bg: rgb(0x0C0C0B),
            surface: white(0.035),
            raise: rgb(0x1C1C1A),
            ink: rgb(0xF2EFE8),
            muted: rgb(0xA8A39A),
            faint: rgb(0x6F6B64),
            line: white(0.05),
            line_strong: white(0.09),
            accent: rgb(0xF5C84B),
            accent_text: rgb(0xF5C84B),
            danger: rgb(0xF07167),
            success: rgb(0x7BD88F),
            on_ink: rgb(0x111110),
            hover: white(0.035),
            selected: rgb(0xF5C84B).alpha(0.12),
            fill: white(0.09),
            current: white(0.08),
            plate: rgb(0x050509),
            scrim: Color::BLACK.alpha(0.55),
            shadow: Color::BLACK.alpha(0.55),
        }
    }

    /// The light palette, kept for after launch.
    pub fn paper() -> Self {
        Self {
            sunk: rgb(0xEBE6DA),
            bg: rgb(0xF3EFE6),
            surface: rgb(0xFAF7F0),
            raise: rgb(0xFFFDF8),
            ink: rgb(0x1C1B19),
            muted: rgb(0x5C574E),
            faint: rgb(0x8A8377),
            line: rgb(0xE0D9CA),
            line_strong: rgb(0xB8AF9C),
            accent: rgb(0xF5C84B),
            accent_text: rgb(0x7A5A00),
            danger: rgb(0xB42E24),
            success: rgb(0x27713B),
            on_ink: rgb(0xFAF7F0),
            hover: rgb(0x1C1B19).alpha(0.04),
            selected: rgb(0xF5C84B).alpha(0.2),
            fill: rgb(0x1C1B19).alpha(0.07),
            current: rgb(0x1C1B19).alpha(0.06),
            plate: rgb(0x050509),
            scrim: Color::BLACK.alpha(0.6),
            shadow: Color::BLACK.alpha(0.55),
        }
    }
}

/// Spacing in points (`space`). `TIGHT` and `SNUG` are half-steps, for
/// parts of one thing: lines of an item stacked, and a dot beside its words.
pub mod space {
    /// Between stacked lines of one item: a name over its detail.
    pub const TIGHT: f32 = 2.0;
    pub const XXS: f32 = 4.0;
    /// Between inline parts of one thing: a dot and its words, chips.
    pub const SNUG: f32 = 6.0;
    pub const XS: f32 = 8.0;
    /// A list row's padding down its sides.
    pub const ROW_Y: f32 = 10.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 16.0;
    pub const LG: f32 = 24.0;
    pub const XL: f32 = 32.0;
    pub const XXL: f32 = 48.0;
    /// A place's page in from the place's sides.
    pub const PAGE: f32 = 28.0;
}

/// Corners in points.
pub mod radius {
    /// Rows: the sidebar's, a list's, a menu's.
    pub const CONTROL: f32 = 7.0;
    /// Cards and dialogs.
    pub const CARD: f32 = 12.0;
    /// A pill: buttons, chips, fields. Rune holds a corner to half the
    /// shorter side, so this is round at any height.
    pub const PILL: f32 = 999.0;
}

/// The window's fixed measures, in points.
pub mod size {
    /// The top bar, inside the content.
    pub const TOP_BAR: f32 = 52.0;
    /// The sidebar, shown.
    pub const SIDEBAR: f32 = 232.0;
    /// A place in the sidebar, and a row in a menu or pane.
    pub const ROW: f32 = 30.0;
    /// A glyph's side.
    pub const ICON: f32 = 16.0;
    /// A glyph's stroke.
    pub const ICON_STROKE: f32 = 1.4;
    /// A toast saying what just happened.
    pub const TOAST: f32 = 440.0;
    pub const TOAST_HEIGHT: f32 = 44.0;
    /// A tab's words, centered in this, over its underline; what sits at
    /// the end of a row of tabs centers in it too.
    pub const TAB: f32 = 34.0;
    /// A button's height, and a field's.
    pub const CONTROL: f32 = 30.0;
    /// A small button's.
    pub const CONTROL_SMALL: f32 = 26.0;
    /// A page's main action.
    pub const CONTROL_MAIN: f32 = 40.0;
    /// The search in the top bar.
    pub const SEARCH: f32 = 260.0;
    /// The pane column beside the stage.
    pub const PANE_COLUMN: f32 = 320.0;
    /// A list's details beside the list, at their narrowest and widest;
    /// between, a third of the width (`ui::split`).
    pub const DETAILS_MIN: f32 = 360.0;
    pub const DETAILS_MAX: f32 = 520.0;
    /// The narrowest a place lays a list and its details side by side;
    /// narrower, the details slide in over the list.
    pub const SIDE_BY_SIDE: f32 = 960.0;
    /// Work's progress shown in a row of a list, in place of its action.
    pub const INLINE_PROGRESS: f32 = 80.0;
    /// A card's width: a fighter's, a stage's, a skin's. Its plate is 4:5.
    pub const CARD: f32 = 148.0;
    /// A texture's thumbnail in a list.
    pub const THUMBNAIL: f32 = 32.0;
    /// A color swatch's side.
    pub const SWATCH: f32 = 20.0;
    /// A rule, a border, an outline.
    pub const LINE: f32 = 1.0;
    /// An emphasized outline: the chosen swatch.
    pub const LINE_STRONG: f32 = 2.0;
    /// A status dot's side.
    pub const DOT: f32 = 6.0;
    /// An account's mark, in a row.
    pub const AVATAR: f32 = 20.0;
    /// An account's picture, heading You.
    pub const PORTRAIT: f32 = 72.0;
    /// A dialog asking a question, and one holding a list to look over.
    pub const DIALOG: f32 = 460.0;
    pub const DIALOG_WIDE: f32 = 560.0;
    /// A skin's sheet: its picture beside what it does.
    pub const SHEET: f32 = 760.0;
    /// The tallest a dialog grows; what it holds scrolls past that.
    pub const DIALOG_MOST_HEIGHT: f32 = 640.0;
}

/// Shadows: how far below and how blurred, in points.
pub mod elevation {
    /// A dialog over the window.
    pub const DIALOG: (f32, f32) = (18.0, 50.0);
    /// A toast over the place.
    pub const TOAST: (f32, f32) = (8.0, 24.0);
}

const SANS: Family = Family::Named("Schibsted Grotesk");
const MONO: Family = Family::Named("DM Mono");

/// Type styles (`text`). Schibsted Grotesk 13 for body, 12 for secondary
/// and section labels, 24 bold for a page's one big title; DM Mono only for
/// counts and codes.
pub mod text {
    use super::{MONO, SANS, TextStyle};

    /// The one thing a page is about, bigger than its title: Play's set.
    pub const DISPLAY: TextStyle = TextStyle::new(34.0, 700.0).family(SANS).tracking(-0.9);
    /// A page's one big title, where it has one.
    pub const H1: TextStyle = TextStyle::new(24.0, 700.0).family(SANS).tracking(-0.4);
    /// A code to read out.
    pub const H2: TextStyle = TextStyle::new(20.0, 700.0).family(SANS).tracking(-0.3);
    /// A dialog's or sheet's title.
    pub const H3: TextStyle = TextStyle::new(17.0, 700.0).family(SANS).tracking(-0.2);
    pub const BODY: TextStyle = TextStyle::new(13.0, 400.0).family(SANS);
    pub const STRONG: TextStyle = TextStyle::new(13.0, 600.0).family(SANS);
    /// The place's name in the top bar.
    pub const TITLE: TextStyle = TextStyle::new(14.0, 600.0).family(SANS);
    /// A section label: muted, over a group.
    pub const LABEL: TextStyle = TextStyle::new(12.0, 500.0).family(SANS);
    pub const SMALL: TextStyle = TextStyle::new(12.0, 400.0).family(SANS);
    pub const WORDMARK: TextStyle = TextStyle::new(13.0, 600.0).family(SANS);
    pub const MONO_TEXT: TextStyle = TextStyle::new(12.0, 400.0)
        .family(MONO)
        .features("'zero' 1, 'tnum' 1");
    pub const MONO_SMALL: TextStyle = TextStyle::new(11.5, 400.0)
        .family(MONO)
        .features("'zero' 1, 'tnum' 1");
    /// A code read out loud, such as a room's.
    pub const MONO_CODE: TextStyle = TextStyle::new(19.0, 500.0)
        .family(MONO)
        .features("'zero' 1, 'tnum' 1");
}

/// Motion. Two things move: the sidebar (180 ms) and a page or sheet
/// coming in (a 220 ms fade and a 6-point rise). The rest lands at once,
/// apart from Rune's color springs on hover.
pub mod motion {
    pub use rune::tok::motion::{
        INSTANT as FADE, REFLOW as CALM, SETTLE as DEFAULT, SNAP as SNAPPY,
    };
    use rune::{Spring, Transition};

    /// The sidebar showing or hiding.
    pub const SIDEBAR: Spring = Spring::new(0.18, 0.0);
    /// How far a page or sheet rises as it comes in.
    pub const RISE: f32 = 6.0;
    const ENTER: Spring = Spring::new(0.22, 0.0);

    /// A page or sheet coming in: it fades in and rises.
    pub fn enter() -> Transition {
        Transition::offset(0.0, RISE)
            .combined(Transition::opacity())
            .springs(rune::SpringPair::new(ENTER, ENTER))
    }
}
