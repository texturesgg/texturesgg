//! Glyphs: 16 points square, 1.4-point strokes, drawn on a 24-unit grid
//! (the mockups' SVG paths). Each takes the color it's drawn in.

use super::tokens::size;
use rune::{Color, El, LineCap, Path, Shape};

/// The grid glyphs are drawn on.
const GRID: f32 = 24.0;

/// What a glyph shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Play,
    Mods,
    Skins,
    Replays,
    Sidebar,
    Search,
    Gear,
    Close,
    Chevron,
    Plus,
    Check,
    Download,
    More,
}

/// `icon` in `color`.
pub fn icon(icon: Icon, color: Color) -> El {
    let shape = Shape::new().view_box(GRID, GRID);
    let stroke = |shape: Shape, points: &[[f32; 2]], closed: bool| {
        shape.stroke_with(
            points.to_vec(),
            closed,
            size::ICON_STROKE,
            LineCap::Round,
            color,
        )
    };
    let shape = match icon {
        Icon::Play => stroke(shape, &[[7.0, 4.5], [18.5, 12.0], [7.0, 19.5]], true),
        Icon::Mods => [[4.0, 4.0], [13.0, 4.0], [4.0, 13.0], [13.0, 13.0]]
            .into_iter()
            .fold(shape, |shape, [x, y]| {
                stroke(
                    shape,
                    &[[x, y], [x + 7.0, y], [x + 7.0, y + 7.0], [x, y + 7.0]],
                    true,
                )
            }),
        Icon::Skins => stroke(
            shape,
            &[
                [8.0, 3.0],
                [12.0, 6.0],
                [16.0, 3.0],
                [21.0, 7.0],
                [18.0, 11.0],
                [16.0, 10.0],
                [16.0, 20.0],
                [8.0, 20.0],
                [8.0, 10.0],
                [6.0, 11.0],
                [3.0, 7.0],
            ],
            true,
        ),
        Icon::Replays => {
            // Most of a circle, from its left round underneath to its upper
            // left, and the arrowhead at its start.
            let arc = arc([12.0, 12.0], 8.0, 180.0, -129.0);
            stroke(
                stroke(shape, &arc, false),
                &[[4.0, 4.0], [4.0, 8.0], [8.0, 8.0]],
                false,
            )
        }
        Icon::Sidebar => stroke(
            stroke(shape, &rounded_rect([3.0, 4.0], [18.0, 16.0], 3.0), true),
            &[[9.0, 4.0], [9.0, 20.0]],
            false,
        ),
        Icon::Search => stroke(
            stroke(shape, &arc([11.0, 11.0], 6.0, 0.0, 360.0), true),
            &[[16.0, 16.0], [20.0, 20.0]],
            false,
        ),
        Icon::Gear => stroke(
            stroke(shape, &cogwheel([12.0, 12.0], 6.6, 9.4), true),
            &arc([12.0, 12.0], 3.0, 0.0, 360.0),
            true,
        ),
        Icon::Close => stroke(
            stroke(shape, &[[6.0, 6.0], [18.0, 18.0]], false),
            &[[18.0, 6.0], [6.0, 18.0]],
            false,
        ),
        Icon::Chevron => stroke(shape, &[[7.0, 10.0], [12.0, 15.0], [17.0, 10.0]], false),
        Icon::Plus => stroke(
            stroke(shape, &[[12.0, 5.0], [12.0, 19.0]], false),
            &[[5.0, 12.0], [19.0, 12.0]],
            false,
        ),
        Icon::Check => stroke(shape, &[[5.0, 12.5], [10.0, 17.0], [19.0, 7.0]], false),
        // An arrow down onto a tray.
        Icon::Download => stroke(
            stroke(
                stroke(shape, &[[12.0, 4.0], [12.0, 15.0]], false),
                &[[7.5, 10.5], [12.0, 15.0], [16.5, 10.5]],
                false,
            ),
            &[[5.0, 15.0], [5.0, 19.5], [19.0, 19.5], [19.0, 15.0]],
            false,
        ),
        // Three dots in a row.
        Icon::More => [6.0, 12.0, 18.0].into_iter().fold(shape, |shape, x| {
            stroke(shape, &arc([x, 12.0], 0.8, 0.0, 360.0), true)
        }),
    };
    rune::canvas(shape).size(size::ICON, size::ICON)
}

/// The account glyph, a head and shoulders: its own, as it needs a curve.
pub fn person(color: Color) -> El {
    let shoulders =
        Path::new()
            .move_to([4.0, 21.0])
            .cubic_to([5.0, 16.0], [19.0, 16.0], [20.0, 21.0]);
    let shape = Shape::new()
        .view_box(GRID, GRID)
        .stroke_with(
            arc([12.0, 8.0], 4.0, 0.0, 360.0),
            true,
            size::ICON_STROKE,
            LineCap::Round,
            color,
        )
        .stroke_path(&shoulders, size::ICON_STROKE, LineCap::Round, color);
    rune::canvas(shape).size(size::ICON, size::ICON)
}

/// Points along a circle about `center`, from `from` to `to` degrees
/// (0 is right, 90 is down, as the screen's y runs).
fn arc(center: [f32; 2], radius: f32, from: f32, to: f32) -> Vec<[f32; 2]> {
    const STEPS: usize = 32;
    (0..=STEPS)
        .map(|step| {
            let degrees = from + (to - from) * step as f32 / STEPS as f32;
            let (sin, cos) = degrees.to_radians().sin_cos();
            [center[0] + radius * cos, center[1] + radius * sin]
        })
        .collect()
}

/// A connected 6-tooth mechanical cogwheel outline centered at `center`.
fn cogwheel(center: [f32; 2], inner: f32, outer: f32) -> Vec<[f32; 2]> {
    const TEETH: usize = 6;
    const PITCH: f32 = 360.0 / TEETH as f32;
    const BASE_HALF: f32 = 14.0;
    const TIP_HALF: f32 = 9.0;
    const VALLEY_STEPS: usize = 5;
    let point = |radius: f32, degrees: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        [center[0] + radius * cos, center[1] + radius * sin]
    };
    let mut points = Vec::with_capacity(TEETH * (4 + VALLEY_STEPS));
    for tooth in 0..TEETH {
        let mid = -90.0 + tooth as f32 * PITCH;
        points.push(point(inner, mid - BASE_HALF));
        points.push(point(outer, mid - TIP_HALF));
        points.push(point(outer, mid + TIP_HALF));
        points.push(point(inner, mid + BASE_HALF));
        let valley_start = mid + BASE_HALF;
        let valley_end = mid + PITCH - BASE_HALF;
        for step in 1..VALLEY_STEPS {
            let degrees =
                valley_start + (valley_end - valley_start) * step as f32 / VALLEY_STEPS as f32;
            points.push(point(inner, degrees));
        }
    }
    points
}

/// A rectangle's outline from `at`, `size` big, its corners rounded.
fn rounded_rect(at: [f32; 2], size: [f32; 2], corner: f32) -> Vec<[f32; 2]> {
    let [x, y] = at;
    let [w, h] = size;
    let mut points = Vec::new();
    for (center, from) in [
        ([x + w - corner, y + corner], -90.0),
        ([x + w - corner, y + h - corner], 0.0),
        ([x + corner, y + h - corner], 90.0),
        ([x + corner, y + corner], 180.0),
    ] {
        points.extend(arc(center, corner, from, from + 90.0));
    }
    points
}
