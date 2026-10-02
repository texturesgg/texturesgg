//! A virtualized list of one-line options under section headings, for long
//! lists such as a fighter's moves. Only visible rows render.
//!
//! Selection belongs to the caller, as in [`crate::ThumbnailList`]: clicking
//! an option or pressing Up or Down while the list has focus asks the caller
//! to select it, skipping headings and disabled options, and the selection
//! scrolls into view. A disabled option shows why on hover.

use crate::tokens::{density, font, radius, space, text};
use crate::{Theme, Tooltip, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, RenderOnce, ScrollStrategy, SharedString, StatefulInteractiveElement, Styled,
    UniformListScrollHandle, Window, div, uniform_list,
};
use std::rc::Rc;

/// One row of an [`OptionList`].
#[derive(Clone)]
pub enum OptionRow {
    /// A section title above the options that follow it.
    Heading(SharedString),
    Option {
        title: SharedString,
        /// A muted hint at the row's end.
        detail: SharedString,
        /// Why the option can't be chosen, shown on hover; `None` when it can.
        disabled: Option<SharedString>,
    },
}

impl OptionRow {
    fn selectable(&self) -> bool {
        matches!(self, Self::Option { disabled: None, .. })
    }
}

/// The next selectable row from `from` in `step` direction, without wrapping;
/// `from` itself when there is none.
fn next_option(rows: &[OptionRow], from: Option<usize>, step: isize) -> Option<usize> {
    let mut index = match from {
        Some(index) => index as isize,
        None if step > 0 => -1,
        None => rows.len() as isize,
    };
    loop {
        index += step;
        let Some(row) = usize::try_from(index).ok().and_then(|at| rows.get(at)) else {
            return from;
        };
        if row.selectable() {
            return Some(index as usize);
        }
    }
}

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

struct ListState {
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The selection last rendered, to notice when the caller changes it.
    shown: Option<usize>,
}

#[derive(IntoElement)]
pub struct OptionList {
    id: ElementId,
    rows: Rc<[OptionRow]>,
    selected: Option<usize>,
    on_select: SelectHandler,
}

impl OptionList {
    pub fn new(
        id: impl Into<ElementId>,
        rows: impl Into<Rc<[OptionRow]>>,
        selected: Option<usize>,
        on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            rows: rows.into(),
            selected,
            on_select: Rc::new(on_select),
        }
    }
}

impl RenderOnce for OptionList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ListState {
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            shown: None,
        });
        if state.read(cx).shown != self.selected {
            state.update(cx, |state, _| state.shown = self.selected);
            if let Some(selected) = self.selected {
                state
                    .read(cx)
                    .scroll
                    .scroll_to_item(selected, ScrollStrategy::Nearest);
            }
        }
        let (focus, scroll) = {
            let state = state.read(cx);
            (state.focus.clone(), state.scroll.clone())
        };

        let selected = self.selected;
        let rows = self.rows.clone();
        let on_select = self.on_select.clone();
        let list = uniform_list((self.id.clone(), "rows"), rows.len(), move |range, _, _| {
            range
                .map(|index| match &rows[index] {
                    OptionRow::Heading(title) => div()
                        .id(("option-heading", index))
                        .h(rem(density::CONTROL_MD))
                        .px(rem(space::XS))
                        .pt(rem(space::XS))
                        .flex()
                        .items_end()
                        .text_size(rem(density::DETAIL_TEXT))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(palette.muted.to_gpui())
                        .child(title.clone()),
                    OptionRow::Option {
                        title,
                        detail,
                        disabled,
                    } => {
                        let select = on_select.clone();
                        let is_selected = selected == Some(index);
                        div()
                            .id(("option", index))
                            .w_full()
                            .h(rem(density::CONTROL_MD))
                            .px(rem(space::XS))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(rem(space::SM))
                            .rounded(rem(radius::SM))
                            .text_size(rem(density::CONTROL_TEXT))
                            .when(is_selected, |row| {
                                row.bg(palette.raise.to_gpui())
                                    .font_weight(FontWeight::MEDIUM)
                            })
                            .map(|row| match disabled {
                                Some(reason) => {
                                    let reason = reason.clone();
                                    row.text_color(palette.muted.to_gpui())
                                        .tooltip(move |_, cx| Tooltip::new(reason.clone()).view(cx))
                                }
                                None => row
                                    .cursor_pointer()
                                    .when(!is_selected, |row| {
                                        row.hover(|style| style.bg(palette.raise.to_gpui()))
                                    })
                                    .on_click(move |_, window, cx| select(index, window, cx)),
                            })
                            .child(div().min_w_0().truncate().child(title.clone()))
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(font::MONO)
                                    .font_weight(FontWeight::NORMAL)
                                    .text_size(rem(text::XS))
                                    .text_color(palette.muted.to_gpui())
                                    .child(detail.clone()),
                            )
                    }
                })
                .collect()
        })
        .track_scroll(&scroll)
        .size_full();

        let key_rows = self.rows.clone();
        let key_scroll = scroll.clone();
        let key_select = self.on_select.clone();
        div()
            .id(self.id)
            .track_focus(&crate::pressable::tab_stop(&focus))
            .tab_index(0)
            .size_full()
            .p(rem(space::XXS))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let step = match event.keystroke.key.as_str() {
                    "down" => 1,
                    "up" => -1,
                    _ => return,
                };
                cx.stop_propagation();
                if let Some(next) = next_option(&key_rows, selected, step)
                    && Some(next) != selected
                {
                    key_scroll.scroll_to_item(next, ScrollStrategy::Center);
                    key_select(next, window, cx);
                }
            })
            .child(list)
    }
}

#[cfg(test)]
mod tests {
    use super::{OptionRow, next_option};

    fn rows() -> Vec<OptionRow> {
        let option = |disabled: Option<&str>| OptionRow::Option {
            title: "option".into(),
            detail: "".into(),
            disabled: disabled.map(Into::into),
        };
        vec![
            OptionRow::Heading("A".into()),
            option(None),
            option(Some("no")),
            OptionRow::Heading("B".into()),
            option(None),
        ]
    }

    #[test]
    fn keys_skip_headings_and_disabled_options_and_stop_at_the_ends() {
        let rows = rows();
        assert_eq!(next_option(&rows, None, 1), Some(1));
        assert_eq!(next_option(&rows, Some(1), 1), Some(4));
        assert_eq!(next_option(&rows, Some(4), 1), Some(4));
        assert_eq!(next_option(&rows, Some(4), -1), Some(1));
        assert_eq!(next_option(&rows, Some(1), -1), Some(1));
        assert_eq!(next_option(&rows, None, -1), Some(4));
        assert_eq!(next_option(&[], None, 1), None);
    }
}
