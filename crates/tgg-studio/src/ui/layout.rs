//! How a place lays out a list beside its chosen item's details on a wide
//! window, the same on every place.
//!
//! A split fills the page under its head with two columns, each scrolling
//! on its own: the list takes two shares of the width, its details one,
//! held between `size::DETAILS_MIN` and `size::DETAILS_MAX`, so both
//! stretch with the window and every place's details sit at the same edge,
//! the same width.

use super::tokens::{Palette, radius, size, space, text};
use rune::{El, ScrollSpec, col, row, text as label};

/// The list's share of a split's width, to its details' one.
const LIST_SHARE: f32 = 2.0;

/// A page that fills the place, for a split: the caller's head stays, the
/// split under it grows into the rest.
pub fn split_page() -> El {
    col()
        .px(space::PAGE)
        .pt(space::XS)
        .pb(space::LG)
        .gap(space::LG)
        .grow(1.0)
        .min_h(0.0)
}

/// A list beside its chosen item's details, filling what's left of the
/// page: `list` from [`split_list`], `details` from [`split_details`].
pub fn split(list: El, details: El) -> El {
    row()
        .gap(space::LG)
        .grow(1.0)
        .min_h(0.0)
        .child(list)
        .child(details)
}

/// A split's list: what stays above it (a search, filters), if anything,
/// and its rows scrolling under that. `key` names the scroll.
pub fn split_list(key: &str, top: Option<El>, rows: El) -> El {
    col()
        .grow(LIST_SHARE)
        .min_w(0.0)
        .min_h(0.0)
        .gap(space::SM)
        .children(top)
        .child(
            col()
                .key(key)
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(col().pb(space::XL).child(rows)),
        )
}

/// A split's details: a panel the list's height, its body scrolling. The
/// caller keys it.
pub fn split_details(body: El, p: &Palette) -> El {
    col()
        .grow(1.0)
        .min_w(size::DETAILS_MIN)
        .max_w(size::DETAILS_MAX)
        .min_h(0.0)
        .fill(p.surface)
        .border(size::LINE, p.line)
        .radius(radius::CONTROL)
        .clip_rounded(radius::CONTROL)
        .child(
            col()
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(body),
        )
}

/// What a split's details say while nothing is chosen. The caller keys it.
pub fn split_empty(words: &str, p: &Palette) -> El {
    col().px(space::MD).py(space::LG).child(
        label(words.to_owned())
            .font(text::BODY)
            .color(p.muted)
            .wrap(),
    )
}
