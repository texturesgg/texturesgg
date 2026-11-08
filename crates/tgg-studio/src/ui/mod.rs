//! The Catalog's design system: its tokens (`tokens`) and the components
//! built from them and Rune's elements. Each component is a function of the
//! palette returning an element; the caller adds its key and what pressing
//! it does.
//!
//! Screens build from these and the tokens alone: no raw lengths or colors
//! outside `ui`. A screen's own geometry (a plate's width, a list's tallest)
//! is a named constant in that screen. `tests::screens_use_the_tokens`
//! holds them to it.

mod controls;
pub mod fonts;
mod marks;
mod surfaces;
pub mod tokens;

pub use controls::{ButtonKind, PLACE_INSET, TEXT_ACTION_INSET, button, chip, icon_button, place_row, search_field, slider, switch, tab, text_action};
pub use marks::{Tone, badge, close_glyph, dot, pause_glyph, play_glyph, progress, sidebar_glyph, status, swatch};
pub use surfaces::{Dialog, DialogWidth, card, card_plate, dialog, page_head, pane, pane_header, pane_row, panel, plate, rule, spacer};

#[cfg(test)]
mod tests;
