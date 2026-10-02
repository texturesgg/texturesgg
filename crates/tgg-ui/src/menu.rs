//! A button that opens a floating menu of actions.
//!
//! The menu floats above everything (gpui's `deferred` + `anchored`), opens
//! below its trigger, or above it when only there does it fit (a trigger at
//! the window's foot), and closes on Escape, on an outside click, or after an
//! item runs. Focus moves into the menu when it opens, so Up, Down, and Enter
//! work at once, and returns to the trigger when it closes.
//!
//! A menu may hold submenus, one level deep, as a menu bar's menus: each
//! opens beside its row on hover, Right, or Enter, and Left or Escape closes
//! it again. [`MenuButton::menus`] builds one from the app's menu bar, for
//! platforms without one.

use crate::button::{Button, ButtonSize, ButtonVariant};
use crate::icon::{Icon, IconName};
use crate::icon_button::IconButton;
use crate::tokens::{density, font, layer, radius, space};
use crate::{Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    Action, Anchor, AnyElement, App, Bounds, Div, ElementId, Entity, FocusHandle,
    InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Pixels, RenderOnce, SharedString,
    Stateful, StatefulInteractiveElement, Styled, WeakFocusHandle, Window, anchored, canvas,
    deferred, div, px,
};
use std::rc::Rc;

type SelectHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// What choosing an item does.
enum Run {
    Handler(SelectHandler),
    /// Dispatch the action where focus was before the menu opened.
    Action(Rc<dyn Action>),
}

/// One action in a menu.
pub struct MenuItem {
    label: SharedString,
    shortcut: Option<SharedString>,
    checked: Option<bool>,
    disabled: bool,
    run: Run,
}

impl MenuItem {
    pub fn new(
        label: impl Into<SharedString>,
        on_select: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            checked: None,
            disabled: false,
            run: Run::Handler(Rc::new(on_select)),
        }
    }

    /// An item that dispatches `action` where focus was before the menu
    /// opened, as a menu bar's items do. It shows as disabled where nothing
    /// there handles the action, and hints the key bound to it there.
    pub fn action(label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        Self {
            label: label.into(),
            shortcut: None,
            checked: None,
            disabled: false,
            run: Run::Action(action.into()),
        }
    }

    /// Make this one of a set of options, marked when `checked`. Every row in
    /// a menu with options leaves room for the mark, so labels line up.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    /// Show the item dimmed; it can't be highlighted or chosen.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// A key hint shown at the row's end; see [`crate::shortcut`].
    pub fn shortcut(mut self, keys: impl Into<SharedString>) -> Self {
        self.shortcut = Some(keys.into());
        self
    }
}

/// A row that opens a menu of its own beside it.
pub struct Submenu {
    label: SharedString,
    entries: Vec<Entry>,
}

impl Submenu {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            entries: Vec::new(),
        }
    }

    pub fn item(mut self, item: MenuItem) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }
}

enum Entry {
    Item(MenuItem),
    Submenu(Submenu),
    Separator,
}

impl Entry {
    /// Whether the keyboard can highlight it.
    fn reachable(&self) -> bool {
        match self {
            Entry::Item(item) => !item.disabled,
            Entry::Submenu(_) => true,
            Entry::Separator => false,
        }
    }
}

/// Which list in the menu: its own, or the open submenu's.
#[derive(Clone, Copy)]
enum Level {
    Top,
    Sub(usize),
}

fn level_entries(entries: &[Entry], level: Level) -> &[Entry] {
    match level {
        Level::Top => entries,
        Level::Sub(index) => match entries.get(index) {
            Some(Entry::Submenu(submenu)) => &submenu.entries,
            _ => &[],
        },
    }
}

struct MenuState {
    open: bool,
    /// The keyboard or pointer highlight, as an index into the entries.
    highlighted: Option<usize>,
    /// The open submenu, as an index into the entries.
    submenu: Option<usize>,
    /// The highlight in the open submenu; while there is one, the keyboard
    /// works there.
    sub_highlighted: Option<usize>,
    trigger: FocusHandle,
    menu: FocusHandle,
    /// The trigger as last painted, to choose which side the menu opens on.
    trigger_bounds: Bounds<Pixels>,
    /// What last had focus outside the menu: action items run there.
    target: Option<WeakFocusHandle>,
}

impl MenuState {
    fn open_submenu(&mut self, index: usize, entries: &[Entry], keyboard: bool) {
        self.highlighted = Some(index);
        self.submenu = Some(index);
        self.sub_highlighted = if keyboard {
            next_item(level_entries(entries, Level::Sub(index)), None, 1)
        } else {
            None
        };
    }

    fn close_submenu(&mut self) {
        self.submenu = None;
        self.sub_highlighted = None;
    }
}

#[derive(IntoElement)]
pub struct MenuButton {
    id: ElementId,
    label: SharedString,
    /// Shown instead of the label, which becomes the tooltip.
    icon: Option<IconName>,
    variant: ButtonVariant,
    size: ButtonSize,
    /// Styled as a select: its label is the current choice.
    select: bool,
    /// The trigger fills the parent's width.
    fill: bool,
    entries: Vec<Entry>,
}

impl MenuButton {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            variant: ButtonVariant::default(),
            size: ButtonSize::default(),
            select: false,
            fill: false,
            entries: Vec::new(),
        }
    }

    /// A trigger showing only `icon`, as an [`IconButton`], named `label` in
    /// its tooltip.
    pub fn icon(id: impl Into<ElementId>, icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            icon: Some(icon),
            ..Self::new(id, label)
        }
    }

    /// The app's menu bar as one menu: each of `menus` becomes a submenu of
    /// action items. The platform's own menus (macOS's Services) are left
    /// out, as are submenus nested deeper than one level.
    pub fn menus(mut self, menus: Vec<gpui::Menu>) -> Self {
        for menu in menus {
            let mut submenu = Submenu::new(menu.name);
            for item in menu.items {
                submenu = match item {
                    gpui::MenuItem::Action {
                        name,
                        action,
                        checked,
                        disabled,
                        ..
                    } => {
                        let item = MenuItem::action(name, action).disabled(disabled);
                        submenu.item(if checked { item.checked(true) } else { item })
                    }
                    gpui::MenuItem::Separator => submenu.separator(),
                    gpui::MenuItem::Submenu(_) | gpui::MenuItem::SystemMenu(_) => submenu,
                };
            }
            self.entries.push(Entry::Submenu(submenu));
        }
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Style the trigger as a select, as the web's `Select`: its label shows
    /// the current choice, with a chevron.
    pub fn select(mut self) -> Self {
        self.select = true;
        self.variant = ButtonVariant::Secondary;
        self
    }

    /// Fill the parent's width, truncating the trigger's label to fit.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn item(mut self, item: MenuItem) -> Self {
        self.entries.push(Entry::Item(item));
        self
    }

    pub fn submenu(mut self, submenu: Submenu) -> Self {
        self.entries.push(Entry::Submenu(submenu));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(Entry::Separator);
        self
    }
}

/// The mark beside a menu option: a filled dot when checked, an empty slot
/// otherwise.
fn check_mark(checked: bool, palette: crate::Palette) -> impl IntoElement {
    div()
        .size(rem(space::SM))
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |slot| {
            slot.child(
                div()
                    .size(rem(6.0))
                    .rounded_full()
                    .bg(palette.accent.to_gpui()),
            )
        })
}

/// The tallest a menu grows before it scrolls, in web pixels (the web's
/// select panel caps at the same).
const MAX_HEIGHT: f32 = 360.0;

/// How far a menu keeps from the window's edges, in window pixels.
const MARGIN: f32 = 8.0;

/// The height of `entries`' rows, in window pixels at `window`'s rem size:
/// every size in them is a token, but a separator's line is a hairline.
fn rows_height(entries: &[Entry], window: &Window) -> f32 {
    let (web, hairlines) = entries
        .iter()
        .fold((0.0, 0.0), |(web, hairlines), entry| match entry {
            Entry::Separator => (web + space::XXS * 2.0, hairlines + 1.0),
            _ => (web + density::CONTROL_MD, hairlines),
        });
    web * f32::from(window.rem_size()) / 16.0 + hairlines
}

/// Whether a menu of `entries` fits only above a trigger at `trigger`: it
/// opens below unless it would cross the window's foot and has room above.
fn opens_upward(entries: &[Entry], trigger: Bounds<Pixels>, window: &Window) -> bool {
    // The rows, then the padding, border, and gap around them.
    let chrome = (space::XXS * 3.0) * f32::from(window.rem_size()) / 16.0 + 2.0;
    let cap = MAX_HEIGHT * f32::from(window.rem_size()) / 16.0;
    let height = (rows_height(entries, window) + chrome).min(cap);
    let (top, bottom) = (f32::from(trigger.top()), f32::from(trigger.bottom()));
    let window_height = f32::from(window.viewport_size().height);
    bottom + height > window_height - MARGIN && top - height >= MARGIN
}

/// The next reachable index from `from` in `step` direction, wrapping and
/// skipping separators and disabled items.
fn next_item(entries: &[Entry], from: Option<usize>, step: isize) -> Option<usize> {
    let count = entries.len() as isize;
    if count == 0 {
        return None;
    }
    let mut index = match from {
        Some(index) => index as isize,
        None if step > 0 => -1,
        None => count,
    };
    for _ in 0..count {
        index = (index + step).rem_euclid(count);
        if entries[index as usize].reachable() {
            return Some(index as usize);
        }
    }
    None
}

fn close(state: &Entity<MenuState>, restore_focus: bool, window: &mut Window, cx: &mut App) {
    let trigger = state.update(cx, |state, cx| {
        state.open = false;
        state.highlighted = None;
        state.close_submenu();
        cx.notify();
        state.trigger.clone()
    });
    if restore_focus {
        window.focus(&trigger, cx);
    }
}

/// Choose the item at `index` in `level`: a submenu opens, an item runs.
fn select(
    entries: &[Entry],
    level: Level,
    index: usize,
    keyboard: bool,
    state: &Entity<MenuState>,
    window: &mut Window,
    cx: &mut App,
) {
    match level_entries(entries, level).get(index) {
        Some(Entry::Submenu(_)) => state.update(cx, |state, cx| {
            state.open_submenu(index, entries, keyboard);
            cx.notify();
        }),
        Some(Entry::Item(item)) if !item.disabled => match &item.run {
            Run::Handler(on_select) => {
                let on_select = on_select.clone();
                close(state, true, window, cx);
                on_select(window, cx);
            }
            // Back where it was, focus takes the action as a key would.
            Run::Action(action) => {
                let action = action.clone();
                let target = state.read(cx).target.as_ref().and_then(|t| t.upgrade());
                close(state, target.is_none(), window, cx);
                if let Some(target) = target {
                    window.focus(&target, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }
        },
        _ => {}
    }
}

/// How the key bound to `action` reads, where `target` has focus.
fn binding_hint(
    action: &dyn Action,
    target: &FocusHandle,
    window: &Window,
) -> Option<SharedString> {
    let binding = window.highest_precedence_binding_for_action_in(action, target)?;
    let keys: Vec<String> = binding
        .keystrokes()
        .iter()
        .map(|keystroke| crate::shortcut(&keystroke.unparse()).to_string())
        .collect();
    Some(keys.join(" ").into())
}

/// Settle each action item's state where it would run: dimmed where
/// nothing handles it, with the key bound to it.
fn resolve(entries: &mut [Entry], target: &FocusHandle, window: &Window) {
    for entry in entries {
        match entry {
            Entry::Item(item) => {
                if let Run::Action(action) = &item.run {
                    item.disabled |= !window.is_action_available_in(action.as_ref(), target);
                    if item.shortcut.is_none() {
                        item.shortcut = binding_hint(action.as_ref(), target, window);
                    }
                }
            }
            Entry::Submenu(submenu) => resolve(&mut submenu.entries, target, window),
            Entry::Separator => {}
        }
    }
}

/// A floating list of rows.
fn panel(id: impl Into<ElementId>, palette: crate::Palette) -> Stateful<Div> {
    div()
        .id(id)
        // Floating above the window, it must also take the pointer: without
        // this, a click on an item reaches what's beneath too.
        .occlude()
        .min_w(rem(192.0))
        // A long menu scrolls rather than leave the window.
        .max_h(rem(MAX_HEIGHT))
        .overflow_y_scroll()
        .p(rem(space::XXS))
        .flex()
        .flex_col()
        .rounded(rem(radius::MD))
        .border_1()
        .border_color(palette.line_strong.to_gpui())
        .bg(palette.raise.to_gpui())
        .shadow(vec![palette.popover_shadow.to_gpui()])
}

/// The rows of `level`, with `highlighted` shaded.
fn rows(
    entries: &Rc<[Entry]>,
    level: Level,
    highlighted: Option<usize>,
    state: &Entity<MenuState>,
    palette: crate::Palette,
) -> Vec<AnyElement> {
    let list = level_entries(entries, level);
    let has_options = list
        .iter()
        .any(|entry| matches!(entry, Entry::Item(item) if item.checked.is_some()));
    let id_base = match level {
        Level::Top => "menu-item",
        Level::Sub(_) => "submenu-item",
    };
    list.iter()
        .enumerate()
        .map(|(index, entry)| {
            let (label, disabled) = match entry {
                Entry::Separator => {
                    return div()
                        .h(px(1.0))
                        .my(rem(space::XXS))
                        .bg(palette.line.to_gpui())
                        .into_any_element();
                }
                Entry::Item(item) => (item.label.clone(), item.disabled),
                Entry::Submenu(submenu) => (submenu.label.clone(), false),
            };
            let hover_state = state.clone();
            let click_state = state.clone();
            let click_entries = entries.clone();
            let trailing = match entry {
                Entry::Item(item) => item.shortcut.clone().map(|keys| {
                    div()
                        .font_family(font::MONO)
                        .text_color(palette.muted.to_gpui())
                        .child(keys)
                        .into_any_element()
                }),
                _ => Some(
                    Icon::new(IconName::ChevronRight)
                        .size(rem(14.0))
                        .color(palette.muted)
                        .into_any_element(),
                ),
            };
            let checked = match entry {
                Entry::Item(item) => item.checked == Some(true),
                _ => false,
            };
            let opens = matches!(entry, Entry::Submenu(_));
            div()
                .id((id_base, index))
                .flex()
                .items_center()
                .justify_between()
                .gap(rem(space::LG))
                .h(rem(density::CONTROL_MD))
                .px(rem(space::SM))
                .rounded(rem(radius::SM))
                .when(!disabled, |row| row.cursor_pointer())
                .when(disabled, |row| row.text_color(palette.muted.to_gpui()))
                .when(highlighted == Some(index), |row| {
                    row.bg(palette.surface.to_gpui())
                })
                .on_mouse_move(move |_, _, cx| {
                    if disabled {
                        return;
                    }
                    let current = hover_state.read(cx);
                    let unchanged = match level {
                        Level::Top => current.highlighted == Some(index),
                        Level::Sub(_) => current.sub_highlighted == Some(index),
                    };
                    if unchanged {
                        return;
                    }
                    hover_state.update(cx, |state, cx| {
                        match level {
                            Level::Top if opens => state.submenu = Some(index),
                            Level::Top => state.close_submenu(),
                            Level::Sub(_) => {}
                        }
                        match level {
                            Level::Top => {
                                state.highlighted = Some(index);
                                state.sub_highlighted = None;
                            }
                            Level::Sub(_) => state.sub_highlighted = Some(index),
                        }
                        cx.notify();
                    });
                })
                .on_click(move |_, window, cx| {
                    select(
                        &click_entries,
                        level,
                        index,
                        false,
                        &click_state,
                        window,
                        cx,
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rem(space::XS))
                        .when(has_options, |label| {
                            label.child(check_mark(checked, palette))
                        })
                        .child(label),
                )
                .children(trailing)
                .into_any_element()
        })
        .collect()
}

impl RenderOnce for MenuButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| MenuState {
            open: false,
            highlighted: None,
            submenu: None,
            sub_highlighted: None,
            trigger: cx.focus_handle(),
            menu: cx.focus_handle(),
            trigger_bounds: Bounds::default(),
            target: None,
        });
        // Remember where focus is while it's outside the menu (a click on
        // the trigger focuses the trigger first, so it doesn't count).
        if let Some(focused) = window.focused(cx) {
            state.update(cx, |state, _| {
                if !state.open && focused != state.trigger && focused != state.menu {
                    state.target = Some(focused.downgrade());
                }
            });
        }
        let (open, highlighted, submenu, sub_highlighted, trigger_focus, menu_focus) = {
            let state = state.read(cx);
            (
                state.open,
                state.highlighted,
                state.submenu,
                state.sub_highlighted,
                state.trigger.clone(),
                state.menu.clone(),
            )
        };
        let (trigger_bounds, target) = {
            let state = state.read(cx);
            let target = state.target.as_ref().and_then(|target| target.upgrade());
            (state.trigger_bounds, target)
        };
        let mut entries = self.entries;
        if open {
            resolve(
                &mut entries,
                target.as_ref().unwrap_or(&trigger_focus),
                window,
            );
        }
        let entries: Rc<[Entry]> = entries.into();
        let upward = opens_upward(&entries, trigger_bounds, window);

        let toggle_state = state.clone();
        let on_press = move |window: &mut Window, cx: &mut App| {
            let (open, menu) = toggle_state.update(cx, |state, cx| {
                state.open = !state.open;
                state.highlighted = None;
                state.close_submenu();
                cx.notify();
                (state.open, state.menu.clone())
            });
            if open {
                window.focus(&menu, cx);
            }
        };
        let trigger = match self.icon {
            Some(icon) => IconButton::new(self.id.clone(), icon, self.label)
                .focus_handle(trigger_focus)
                .on_press(on_press)
                .into_any_element(),
            None => Button::new(self.id.clone(), self.label)
                .variant(self.variant)
                .size(self.size)
                .when(self.select, Button::chevron)
                .when(self.fill, Button::fill)
                .focus_handle(trigger_focus)
                .on_press(on_press)
                .into_any_element(),
        };

        let flyout = submenu.map(|index| {
            // Beside its row: level with it, or, opening upward, with the
            // menu's foot kept where it is.
            let (before, after) = entries.split_at(index.min(entries.len()));
            let after = after.get(1..).unwrap_or_default();
            let flyout = panel((self.id.clone(), "submenu"), palette).children(rows(
                &entries,
                Level::Sub(index),
                sub_highlighted,
                &state,
                palette,
            ));
            if upward {
                flyout.mb(px(rows_height(after, window)))
            } else {
                flyout.mt(px(rows_height(before, window)))
            }
        });

        let panel = open.then(|| {
            let keys_state = state.clone();
            let keys_entries = entries.clone();
            let outside_state = state.clone();
            div()
                .id((self.id.clone(), "menus"))
                .track_focus(&menu_focus)
                .flex()
                .when(upward, |menus| menus.items_end())
                .when(!upward, |menus| menus.items_start())
                .text_color(palette.text.to_gpui())
                .text_size(rem(density::CONTROL_TEXT))
                .font_family(font::SANS)
                .on_mouse_down_out(move |_, window, cx| close(&outside_state, false, window, cx))
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    let handled = on_key(&keys_entries, &keys_state, event, window, cx);
                    if handled {
                        cx.stop_propagation();
                    }
                })
                .child(panel((self.id.clone(), "menu"), palette).children(rows(
                    &entries,
                    Level::Top,
                    highlighted,
                    &state,
                    palette,
                )))
                .children(flyout)
        });

        // `anchored` is positioned absolutely, so it lands at its parent's
        // origin. A zero-height slot after the trigger puts that origin just
        // below it; one before it, anchored by the menu's foot, just above.
        let slot = panel.map(|panel| {
            let (corner, gap) = if upward {
                (Anchor::BottomLeft, div().pb(rem(space::XXS)))
            } else {
                (Anchor::TopLeft, div().pt(rem(space::XXS)))
            };
            div().h_0().child(
                deferred(
                    anchored()
                        .anchor(corner)
                        .snap_to_window_with_margin(px(MARGIN))
                        .child(gap.child(panel)),
                )
                .priority(layer::MENU),
            )
        });
        let bounds_state = state.clone();
        let trigger = div()
            .relative()
            .child(trigger)
            .when(self.fill, |slot| slot.w_full())
            .child(
                canvas(
                    move |bounds, _, cx| {
                        bounds_state.update(cx, |state, _| state.trigger_bounds = bounds);
                    },
                    |_, (), _, _| {},
                )
                .absolute()
                .inset_0(),
            );
        let (before, after) = if upward { (slot, None) } else { (None, slot) };
        div()
            .flex()
            .flex_col()
            .when(self.fill, |menu| menu.w_full())
            .children(before)
            .child(trigger)
            .children(after)
    }
}

/// Handle a key in the open menu; whether it was the menu's.
fn on_key(
    entries: &[Entry],
    state: &Entity<MenuState>,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let (highlighted, submenu, sub_highlighted) = {
        let state = state.read(cx);
        (state.highlighted, state.submenu, state.sub_highlighted)
    };
    // The keyboard works in the submenu once something there is highlighted.
    let in_submenu = submenu.filter(|_| sub_highlighted.is_some());
    let opens = |index: Option<usize>| {
        index.filter(|&index| matches!(entries.get(index), Some(Entry::Submenu(_))))
    };
    match event.keystroke.key.as_str() {
        key @ ("down" | "up") => {
            let step = if key == "down" { 1 } else { -1 };
            state.update(cx, |state, cx| {
                match in_submenu {
                    Some(index) => {
                        state.sub_highlighted = next_item(
                            level_entries(entries, Level::Sub(index)),
                            sub_highlighted,
                            step,
                        );
                    }
                    None => {
                        state.close_submenu();
                        state.highlighted = next_item(entries, highlighted, step);
                    }
                }
                cx.notify();
            });
        }
        "right" => {
            let Some(index) = opens(highlighted).filter(|_| in_submenu.is_none()) else {
                return false;
            };
            state.update(cx, |state, cx| {
                state.open_submenu(index, entries, true);
                cx.notify();
            });
        }
        "left" => {
            if submenu.is_none() {
                return false;
            }
            state.update(cx, |state, cx| {
                state.close_submenu();
                cx.notify();
            });
        }
        "enter" | "space" => match (in_submenu, sub_highlighted, highlighted) {
            (Some(index), Some(item), _) => {
                select(entries, Level::Sub(index), item, true, state, window, cx)
            }
            (None, _, Some(item)) => select(entries, Level::Top, item, true, state, window, cx),
            _ => return false,
        },
        "escape" => {
            if submenu.is_some() {
                state.update(cx, |state, cx| {
                    state.close_submenu();
                    cx.notify();
                });
            } else {
                close(state, true, window, cx);
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{Entry, MenuItem, Submenu, next_item};

    fn entries() -> Vec<Entry> {
        let item = || Entry::Item(MenuItem::new("item", |_, _| {}));
        let disabled = Entry::Item(MenuItem::new("disabled", |_, _| {}).disabled(true));
        vec![
            item(),
            Entry::Separator,
            disabled,
            Entry::Submenu(Submenu::new("submenu")),
            item(),
        ]
    }

    #[test]
    fn keyboard_highlight_wraps_and_skips_separators_and_disabled_items() {
        let entries = entries();
        assert_eq!(next_item(&entries, None, 1), Some(0));
        assert_eq!(
            next_item(&entries, Some(0), 1),
            Some(3),
            "submenus are reachable"
        );
        assert_eq!(next_item(&entries, Some(4), 1), Some(0));
        assert_eq!(next_item(&entries, None, -1), Some(4));
        assert_eq!(next_item(&entries, Some(3), -1), Some(0));
        assert_eq!(next_item(&[], None, 1), None);
    }
}
