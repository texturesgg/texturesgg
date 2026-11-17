//! Small marks: glyphs, the status dot and line, and color swatches.

use super::icons::{Icon, icon};
use super::tokens::{Palette, radius, size, space, text as type_};
use rune::{Color, El, el, row, text};

/// The sidebar toggle's glyph: a window with its sidebar drawn in.
pub fn sidebar_glyph(p: &Palette) -> El {
    icon(Icon::Sidebar, p.muted)
}

/// The close glyph: two strokes crossed.
pub fn close_glyph(p: &Palette) -> El {
    icon(Icon::Close, p.muted)
}

/// The play glyph: a triangle pointing on, 10 × 11 points.
pub fn play_glyph(ink: Color) -> El {
    rune::canvas(
        rune::Shape::new()
            .view_box(10.0, 11.0)
            .fill([[1.0, 0.0], [10.0, 5.5], [1.0, 11.0]], ink),
    )
    .size(10.0, 11.0)
}

/// The pause glyph: two bars, 9 × 11 points.
pub fn pause_glyph(ink: Color) -> El {
    row()
        .gap(3.0)
        .child(el().size(3.0, 11.0).fill(ink))
        .child(el().size(3.0, 11.0).fill(ink))
}

/// A status dot: gold for what's the player's.
pub fn dot(color: Color) -> El {
    el().size(size::DOT, size::DOT)
        .radius(size::DOT / 2.0)
        .fill(color)
}

/// A status line: `words`, after a gold dot when `marked` (something here
/// is the player's), in which case they're a step brighter.
pub fn status(words: impl Into<String>, marked: bool, p: &Palette) -> El {
    row()
        .gap(space::SNUG)
        .items_center()
        .children(marked.then(|| dot(p.accent)))
        .child(
            text(words.into())
                .font(type_::SMALL)
                .color(if marked { p.muted } else { p.faint })
                .truncate(),
        )
}

/// A swatch of `rgba`, as the game stores it, edged in gold while chosen.
pub fn swatch(rgba: [u8; 4], chosen: bool, p: &Palette) -> El {
    el().size(size::SWATCH, size::SWATCH)
        .radius(radius::CONTROL / 2.0)
        .fill(srgb(rgba))
        .border(
            if chosen {
                size::LINE_STRONG
            } else {
                size::LINE
            },
            if chosen { p.accent_text } else { p.line_strong },
        )
}

/// A color as the game stores it (sRGB bytes), to fill with.
pub fn srgb([r, g, b, _]: [u8; 4]) -> Color {
    let channel = |value: u8| rune::color::srgb_decode(f32::from(value) / 255.0);
    Color {
        r: channel(r),
        g: channel(g),
        b: channel(b),
        a: 1.0,
    }
}

/// What a badge says about something: that it's the player's, that there's
/// news (an update), that something's wrong, or just a fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Mine,
    News,
    Danger,
}

/// A badge: a word or two on a small pill ("Using", "In it", "Update").
/// Yellow for what's the player's.
pub fn badge(words: impl Into<String>, tone: Tone, p: &Palette) -> El {
    const HEIGHT: f32 = 20.0;
    let (ink, fill) = match tone {
        Tone::Plain => (p.muted, p.fill),
        Tone::Mine => (p.accent_text, p.selected),
        Tone::News => (p.ink, p.fill),
        Tone::Danger => (p.danger, p.danger.alpha(0.12)),
    };
    row()
        .h(HEIGHT)
        .px(space::XS)
        .items_center()
        .radius(radius::PILL)
        .fill(fill)
        .child(text(words.into()).font(type_::SMALL).color(ink))
}

/// How far along long work is: a track filled in off-white to `fraction`.
pub fn progress(fraction: f32, p: &Palette) -> El {
    const BAR: f32 = 4.0;
    let fraction = fraction.clamp(0.0, 1.0);
    row()
        .h(BAR)
        .w_full()
        .radius(BAR / 2.0)
        .fill(p.fill)
        .clip_rounded(BAR / 2.0)
        .child(
            el().w_frac(fraction)
                .min_w(0.0)
                .h(BAR)
                .fill(p.ink)
                .motion(super::tokens::motion::DEFAULT),
        )
}

/// A placeholder for a line of text still on its way: a muted bar, this
/// fraction of its container wide. Still, never pulsing: nothing loops.
pub fn skeleton(fraction: f32, p: &Palette) -> El {
    const LINE: f32 = 10.0;
    el().w_frac(fraction.clamp(0.0, 1.0))
        .h(LINE)
        .radius(LINE / 2.0)
        .fill(p.fill)
}

/// A mod's icon, `side` points square: the picture once it's arrived, a
/// quiet tile until then (or when it has none).
pub fn icon_tile(pixels: Option<&rune::Pixels>, side: f32, p: &Palette) -> El {
    let corner = (side / 4.0).round();
    let tile = el()
        .size(side, side)
        .radius(corner)
        .clip_rounded(corner)
        .fill(p.fill);
    match pixels {
        Some(pixels) => tile.pixels_fit(pixels, rune::SurfaceFit::Fill),
        None => tile,
    }
}

/// The settings glyph: a gear.
pub fn settings_glyph(p: &Palette) -> El {
    icon(Icon::Gear, p.faint)
}

/// An account's mark: its first letter on a round yellow face.
pub fn avatar(name: &str, p: &Palette) -> El {
    let initial: String = name
        .chars()
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default();
    el().size(size::AVATAR, size::AVATAR)
        .radius(size::AVATAR / 2.0)
        .fill(p.accent)
        .center()
        .child(text(initial).font(type_::LABEL).color(p.on_ink))
}
