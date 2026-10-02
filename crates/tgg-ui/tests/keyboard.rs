//! Every pressable component is reached by Tab and pressed by Enter, from
//! a window whose root holds focus, as the app's does.

use gpui::{
    Context, FocusHandle, InputEvent, InteractiveElement, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, ParentElement, Render, Styled, TestAppContext, VisualTestContext, Window, div,
};
use std::cell::Cell;
use std::rc::Rc;
use tgg_ui::pressable::pressable_with;
use tgg_ui::{Appearance, Theme};

struct Root {
    focus: FocusHandle,
    first: FocusHandle,
    second: FocusHandle,
    pressed: Rc<Cell<usize>>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let first = self.pressed.clone();
        let second = self.pressed.clone();
        tgg_ui::focus_navigation(div().id("root").track_focus(&self.focus))
            .size_full()
            .child(pressable_with(
                "first",
                self.first.clone(),
                Some(Rc::new(move |_, _| first.set(1))),
                cx,
            ))
            .child(pressable_with(
                "second",
                self.second.clone(),
                Some(Rc::new(move |_, _| second.set(2))),
                cx,
            ))
    }
}

#[gpui::test]
fn tab_reaches_each_pressable_and_enter_presses_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::init(Appearance::Gallery, cx);
        tgg_ui::init(cx);
    });
    let pressed = Rc::new(Cell::new(0));
    let window = cx.add_window({
        let pressed = pressed.clone();
        move |window, cx| {
            let root = Root {
                focus: cx.focus_handle(),
                // As the components' handles are, through `focus_handle`.
                first: tgg_ui::pressable::tab_stop(&cx.focus_handle()),
                second: tgg_ui::pressable::tab_stop(&cx.focus_handle()),
                pressed,
            };
            window.focus(&root.focus, cx);
            root
        }
    });
    let root = window.root(cx).expect("the root view");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert!(
            root.read(cx).first.is_focused(window),
            "Tab reaches the first"
        );
    });
    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| {
        assert!(
            root.read(cx).second.is_focused(window),
            "and then the second"
        );
    });
    press(cx, "enter");
    assert_eq!(pressed.get(), 2, "Enter presses the focused one");
    cx.simulate_keystrokes("shift-tab");
    press(cx, "space");
    assert_eq!(
        pressed.get(),
        1,
        "Shift-Tab goes back, and Space presses too"
    );
}

/// Press and release `key`: gpui-ce turns a release of Enter or Space on
/// the element that saw the press into a click.
fn press(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("a key");
    cx.update(|window, cx| {
        window.dispatch_event(
            KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
    });
}
