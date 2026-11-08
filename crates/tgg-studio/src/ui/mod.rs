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
mod layout;
mod marks;
mod surfaces;
pub mod tokens;

pub use controls::{
    ButtonKind, PLACE_INSET, TEXT_ACTION_INSET, button, button_off, chip, icon_button, place_row,
    play_action, search_field, select, select_item, select_menu, select_rule, slider, switch, tab,
    text_action, text_field,
};
pub use layout::{split, split_details, split_empty, split_list, split_page};
pub use marks::{
    Tone, avatar, badge, close_glyph, dot, icon_tile, pause_glyph, play_glyph, progress,
    settings_glyph, sidebar_glyph, skeleton, status, swatch,
};
pub use surfaces::{
    Dialog, DialogWidth, callout, card, card_plate, dialog, page_head, pane, pane_header, pane_row,
    panel, plate, rule, setting_row, setting_row_with, spacer,
};

#[cfg(test)]
mod tests;
