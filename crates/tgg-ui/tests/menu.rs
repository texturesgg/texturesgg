//! A menu built from the app's menu bar runs each action where focus was
//! before it opened, not in the menu, and the keyboard reaches every item
//! through the submenus.

use gpui::{
    Context, FocusHandle, InputEvent, InteractiveElement, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Menu, MenuItem, ParentElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, actions, div,
};
use std::cell::RefCell;
use std::rc::Rc;
use tgg_ui::pressable::pressable_with;
use tgg_ui::{Appearance, MenuButton, Theme};

actions!(test, [Ping, Pong, Unhandled]);

struct Root {
    focus: FocusHandle,
    /// Where the actions are handled: beside the menu, not above it.
    pane: FocusHandle,
    ran: Rc<RefCell<Vec<&'static str>>>,
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ping = self.ran.clone();
        let pong = self.ran.clone();
        tgg_ui::focus_navigation(div().id("root").track_focus(&self.focus))
            .size_full()
            .child(
                pressable_with("pane", self.pane.clone(), None, cx)
                    .on_action(move |_: &Ping, _, _| ping.borrow_mut().push("ping"))
                    .on_action(move |_: &Pong, _, _| pong.borrow_mut().push("pong")),
            )
            .child(MenuButton::new("app-menu", "Menu").menus(vec![
                Menu::new("First").items([
                    MenuItem::action("Ping", Ping),
                    MenuItem::action("Unhandled", Unhandled),
                ]),
                Menu::new("Second").items([MenuItem::action("Pong", Pong)]),
            ]))
    }
}

#[gpui::test]
fn actions_run_where_focus_was_through_the_keyboard(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Theme::init(Appearance::Gallery, cx);
        tgg_ui::init(cx);
    });
    let ran = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window({
        let ran = ran.clone();
        move |window, cx| {
            let root = Root {
                focus: cx.focus_handle(),
                pane: tgg_ui::pressable::tab_stop(&cx.focus_handle()),
                ran,
            };
            window.focus(&root.pane, cx);
            root
        }
    });
    let root = window.root(cx).expect("the root view");
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    cx.run_until_parked();

    // From the pane, Tab reaches the trigger and Enter opens the menu.
    cx.simulate_keystrokes("tab");
    press(cx, "enter");
    // Down highlights First, Right opens it on Ping, Enter runs it.
    cx.simulate_keystrokes("down right enter");
    cx.run_until_parked();
    assert_eq!(*ran.borrow(), ["ping"], "Ping runs in the pane");
    cx.update(|window, cx| {
        assert!(
            root.read(cx).pane.is_focused(window),
            "and focus goes back there"
        );
    });

    cx.simulate_keystrokes("tab");
    press(cx, "enter");
    // Nothing handles Unhandled, so Down skips it and wraps to Ping; Left
    // leaves First, Down reaches Second, and Enter opens it too.
    cx.simulate_keystrokes("down right down left down enter enter");
    cx.run_until_parked();
    assert_eq!(*ran.borrow(), ["ping", "pong"]);
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
    cx.run_until_parked();
}
