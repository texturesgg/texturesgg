//! Design tokens, from the textures.gg site's design system as tgg-ui copies
//! them, in the Catalog direction's use (`spikes/editor-next/mocks/b-language.html`).
//!
//! The site is the source. Colors are written as the site writes them (sRGB
//! hex) and converted to Rune's linear colors; lengths are in points, which
//! Rune draws at the display's scale.
//!
//! The whole scale is here, as the site defines it, though not every step is
//! used yet.
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

/// One palette's colors (the web's `color` group, and the editor's
/// additions: `sunk` behind the sidebar, `plate` behind a model).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub sunk: Color,
    pub bg: Color,
    pub surface: Color,
    pub raise: Color,
    pub ink: Color,
    pub muted: Color,
    pub faint: Color,
    pub line: Color,
    pub line_strong: Color,
    /// A fill: what's yours (installed, changed, selected). Never text.
    pub accent: Color,
    /// Accent-colored text and lines: focus rings, selected edges.
    pub accent_text: Color,
    pub danger: Color,
    pub success: Color,
    /// Text on an ink fill (the primary button).
    pub on_ink: Color,
    /// The 4% wash under a hovered row.
    pub hover: Color,
    /// The gold wash under a selected row.
    pub selected: Color,
    /// Behind a model on the stage: hsd-render clears to this.
    pub plate: Color,
    /// Over the window behind a dialog.
    pub scrim: Color,
    /// A dialog's shadow.
    pub shadow: Color,
}

impl Palette {
    /// The palette for `appearance`.
    pub fn of(appearance: Appearance) -> Self {
        match appearance {
            Appearance::Gallery => Self::gallery(),
            Appearance::Paper => Self::paper(),
        }
    }

    /// The dark default.
    pub fn gallery() -> Self {
        Self {
            sunk: rgb(0x0C0C0B),
            bg: rgb(0x111110),
            surface: rgb(0x1B1B19),
            raise: rgb(0x262522),
            ink: rgb(0xF2EFE8),
            muted: rgb(0xA8A39A),
            faint: rgb(0x77726A),
            line: rgb(0x2F2E2A),
            line_strong: rgb(0x3A3934),
            accent: rgb(0xF5C84B),
            accent_text: rgb(0xF5C84B),
            danger: rgb(0xF07167),
            success: rgb(0x7BD88F),
            on_ink: rgb(0x111110),
            hover: rgb(0xFFF8E6).alpha(0.035),
            selected: rgb(0xF5C84B).alpha(0.12),
            plate: rgb(0x050509),
            scrim: Color::BLACK.alpha(0.6),
            shadow: Color::BLACK.alpha(0.55),
        }
    }

    /// The light palette; surfaces lighten toward the viewer.
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
            plate: rgb(0x050509),
            // As Gallery's for now: Paper's dialogs are yet to be designed.
            scrim: Color::BLACK.alpha(0.6),
            shadow: Color::BLACK.alpha(0.55),
        }
    }
}

/// Spacing in points (`space`). `TIGHT` and `SNUG` are the desktop's
/// half-steps, for parts of one thing: lines of an item stacked, and a dot
/// beside its words.
pub mod space {
    /// Between stacked lines of one item: a name over its detail.
    pub const TIGHT: f32 = 2.0;
    pub const XXS: f32 = 4.0;
    /// Between inline parts of one thing: a dot and its words, chips.
    pub const SNUG: f32 = 6.0;
    pub const XS: f32 = 8.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 16.0;
    pub const LG: f32 = 24.0;
    pub const XL: f32 = 32.0;
    pub const XXL: f32 = 48.0;
    /// A place's page in from the place's sides.
    pub const PAGE: f32 = LG + XXS;
}

/// Corners in points. Tables and plates are square; controls and panes take
/// `CONTROL`. No pills: a deliberate break from the site.
pub mod radius {
    pub const CONTROL: f32 = 4.0;
}

/// The window's fixed measures, in points.
pub mod size {
    /// The title row, and the sidebar's head beside it.
    pub const TITLE_ROW: f32 = 44.0;
    pub const SIDEBAR: f32 = 220.0;
    /// A place in the sidebar, and a row in a pane.
    pub const ROW: f32 = 32.0;
    /// A game's card on Play, and a card like it.
    pub const GAME_CARD: f32 = 340.0;
    /// A tab's words, centered in this, over its underline; what sits at
    /// the end of a row of tabs centers in it too.
    pub const TAB: f32 = 34.0;
    /// A control's height.
    pub const CONTROL: f32 = 28.0;
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
    pub const AVATAR: f32 = 22.0;
    /// A dialog asking a question, and one holding a list to look over.
    pub const DIALOG: f32 = 440.0;
    pub const DIALOG_WIDE: f32 = 560.0;
    /// The tallest a dialog grows; what it holds scrolls past that.
    pub const DIALOG_MOST_HEIGHT: f32 = 640.0;
}

/// Shadows: how far below and how blurred, in points.
pub mod elevation {
    /// A dialog over the window.
    pub const DIALOG: (f32, f32) = (24.0, 60.0);
}

const SANS: Family = Family::Named("Schibsted Grotesk");
const MONO: Family = Family::Named("DM Mono");

/// Type styles (`text`), at desktop density. Headings are heavy and tight;
/// facts (versions, sizes, frames, file names) are mono.
pub mod text {
    use super::{MONO, SANS, TextStyle};

    pub const DISPLAY: TextStyle = TextStyle::new(56.0, 800.0).family(SANS).tracking(-2.5);
    pub const H1: TextStyle = TextStyle::new(40.0, 800.0).family(SANS).tracking(-1.6);
    pub const H2: TextStyle = TextStyle::new(28.0, 700.0).family(SANS).tracking(-0.85);
    pub const H3: TextStyle = TextStyle::new(18.0, 700.0).family(SANS).tracking(-0.27);
    pub const BODY: TextStyle = TextStyle::new(13.5, 400.0).family(SANS);
    pub const STRONG: TextStyle = TextStyle::new(13.5, 600.0).family(SANS);
    pub const TITLE: TextStyle = TextStyle::new(13.5, 700.0).family(SANS);
    pub const LABEL: TextStyle = TextStyle::new(12.5, 600.0).family(SANS);
    pub const SMALL: TextStyle = TextStyle::new(12.5, 400.0).family(SANS);
    pub const WORDMARK: TextStyle = TextStyle::new(15.0, 800.0).family(SANS).tracking(-0.45);
    pub const MONO_TEXT: TextStyle = TextStyle::new(12.5, 400.0)
        .family(MONO)
        .features("'zero' 1, 'tnum' 1");
    pub const MONO_SMALL: TextStyle = TextStyle::new(11.5, 400.0)
        .family(MONO)
        .features("'zero' 1, 'tnum' 1");
}

/// Motion: Rune's springs, which the Catalog's motion rules name (fade 150,
/// snappy 250, default 300, calm 400 ms). One curve; nothing loops but the
/// fighter.
pub mod motion {
    pub use rune::tok::motion::{
        INSTANT as FADE, REFLOW as CALM, SETTLE as DEFAULT, SNAP as SNAPPY,
    };
}
