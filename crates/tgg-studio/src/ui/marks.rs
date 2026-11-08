//! Small marks: glyphs, the status dot and line, and color swatches.

use super::tokens::{Palette, radius, size, space, text as type_};
use rune::{Color, El, el, row, text};

/// The sidebar toggle's glyph: a window with its sidebar drawn in.
pub fn sidebar_glyph(p: &Palette) -> El {
    el().size(16.0, 13.0)
        .radius(2.5)
        .border(1.5, p.muted)
        .child(el().size(1.5, 13.0).fill(p.muted).absolute(5.0, 0.0))
}

/// The close glyph: two strokes crossed.
pub fn close_glyph(p: &Palette) -> El {
    let stroke = |turn: f32| {
        el().size(13.0, 1.5)
            .radius(0.75)
            .fill(p.muted)
            .rotate(turn)
            .absolute(0.5, 5.75)
    };
    el().size(14.0, 13.0)
        .child(stroke(45.0))
        .child(stroke(-45.0))
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

/// A badge: a word or two in a thin outline ("Installed", "Update 0.2.0").
pub fn badge(words: impl Into<String>, tone: Tone, p: &Palette) -> El {
    let (ink, edge, fill) = match tone {
        Tone::Plain => (p.muted, p.line_strong, Color::TRANSPARENT),
        Tone::Mine => (p.accent_text, p.accent_text, p.selected),
        Tone::News => (p.ink, p.line_strong, p.raise),
        Tone::Danger => (p.danger, p.danger, Color::TRANSPARENT),
    };
    text(words.into())
        .font(type_::LABEL)
        .color(ink)
        .px(space::SNUG)
        .py(1.0)
        .radius(radius::CONTROL)
        .fill(fill)
        .border(size::LINE, edge)
}

/// How far along long work is: a thin track filled in gold to `fraction`.
pub fn progress(fraction: f32, p: &Palette) -> El {
    const BAR: f32 = 3.0;
    let fraction = fraction.clamp(0.0, 1.0);
    row()
        .h(BAR)
        .w_full()
        .radius(BAR / 2.0)
        .fill(p.line_strong)
        .clip_rounded(BAR / 2.0)
        .child(
            el().w_frac(fraction)
                .min_w(0.0)
                .h(BAR)
                .fill(p.accent)
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
        .fill(p.raise)
}

/// A mod's icon, `side` points square: the picture once it's arrived, a
/// quiet tile until then (or when it has none).
pub fn icon_tile(pixels: Option<&rune::Pixels>, side: f32, p: &Palette) -> El {
    let tile = el()
        .size(side, side)
        .radius(radius::CONTROL)
        .clip_rounded(radius::CONTROL)
        .fill(p.raise)
        .border(size::LINE, p.line);
    match pixels {
        Some(pixels) => tile.pixels_fit(pixels, rune::SurfaceFit::Fill),
        None => tile,
    }
}
