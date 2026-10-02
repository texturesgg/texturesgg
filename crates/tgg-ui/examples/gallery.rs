//! Every tgg-ui component in one window, in either palette, for checking
//! them one at a time as they're built.
//!
//! ```text
//! cargo run -p tgg-ui --example gallery [-- --paper] [--dialog]
//! ```

use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Render, Styled, TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, size,
};
use tgg_ui::tokens::{font, space, text};
use tgg_ui::{
    Appearance, Button, ButtonSize, ButtonVariant, Dialog, MenuButton, MenuItem, PaneSize, Split,
    SplitState, Theme, ThumbnailItem, ThumbnailList, Tooltip, rem,
};

struct Gallery {
    split: Entity<SplitState>,
    dialog_open: bool,
    thumbnails: std::rc::Rc<[ThumbnailItem]>,
    selected: Option<usize>,
}

/// Procedural stand-ins for decoded textures: hue gradients.
fn sample_thumbnails() -> std::rc::Rc<[ThumbnailItem]> {
    let names = [
        ("PlFcRe body", "256×256 · CMPR · 3 uses"),
        ("PlFcRe eyes", "32×32 · RGB5A3"),
        ("PlFcRe feathers", "128×128 · CMPR · 2 uses"),
        ("PlFcRe boots", "64×64 · CI8"),
        ("PlFcRe jacket", "128×128 · CMPR"),
        ("PlFcRe emblem", "64×32 · IA8"),
    ];
    names
        .iter()
        .enumerate()
        .map(|(index, (title, detail))| {
            let rgba: Vec<u8> = (0..64 * 64)
                .flat_map(|texel| {
                    let (x, y) = (texel % 64, texel / 64);
                    let shift = index as u32 * 40;
                    [
                        ((x * 4 + shift) % 256) as u8,
                        ((y * 4 + shift / 2) % 256) as u8,
                        (255 - (x + y) * 2) as u8,
                        255,
                    ]
                })
                .collect();
            ThumbnailItem {
                image: tgg_ui::render_image(&rgba, 64, 64),
                title: (*title).into(),
                detail: (*detail).into(),
            }
        })
        .collect()
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *Theme::global(cx);
        let palette = theme.palette;
        let this = cx.entity();
        let set_dialog = move |open: bool| {
            let this = this.clone();
            move |_: &mut Window, cx: &mut App| {
                this.update(cx, |gallery, cx| {
                    gallery.dialog_open = open;
                    cx.notify();
                })
            }
        };
        let heading = |label: &'static str| {
            div()
                .text_size(rem(text::XS))
                .text_color(palette.muted.to_gpui())
                .child(label)
        };
        let row = || div().flex().flex_wrap().items_center().gap(rem(space::XS));
        let variants = [
            ("Primary", ButtonVariant::Primary),
            ("Secondary", ButtonVariant::Secondary),
            ("Ghost", ButtonVariant::Ghost),
            ("Danger", ButtonVariant::Danger),
        ];
        let buttons = |size: ButtonSize, suffix: &'static str| {
            row().children(variants.iter().map(move |(label, variant)| {
                Button::new(format!("{label}-{suffix}"), *label)
                    .variant(*variant)
                    .size(size)
                    .on_press(|_, _| {})
            }))
        };
        let panel = |title: &'static str, body: &'static str| {
            div()
                .size_full()
                .bg(palette.surface.to_gpui())
                .p(rem(space::SM))
                .flex()
                .flex_col()
                .gap(rem(space::XS))
                .child(
                    div()
                        .text_size(rem(text::SM))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(rem(text::XS))
                        .text_color(palette.muted.to_gpui())
                        .child(body),
                )
        };

        let toggle = Button::new(
            "palette",
            match theme.appearance {
                Appearance::Gallery => "Switch to Paper",
                Appearance::Paper => "Switch to Gallery",
            },
        )
        .on_press(|window, cx| {
            let next = match Theme::global(cx).appearance {
                Appearance::Gallery => Appearance::Paper,
                Appearance::Paper => Appearance::Gallery,
            };
            Theme::init(next, cx);
            window.refresh();
        });

        let menu = MenuButton::new("file-menu", "File")
            .item(MenuItem::new("Open DAT…", |_, _| {}).shortcut(tgg_ui::shortcut("secondary-o")))
            .item(MenuItem::new("Save", |_, _| {}).shortcut(tgg_ui::shortcut("secondary-s")))
            .separator()
            .item(MenuItem::new("Export PNG…", |_, _| {}));

        let center =
            div()
                .size_full()
                .p(rem(space::LG))
                .flex()
                .flex_col()
                .gap(rem(space::MD))
                .child(
                    div()
                        .text_size(rem(text::H3))
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .child("tgg-ui gallery"),
                )
                .child(row().child(toggle))
                .child(heading("Menu and tooltips"))
                .child(
                    row()
                        .child(menu)
                        .child(
                            Button::new("import", "Import PNG")
                                .variant(ButtonVariant::Ghost)
                                .tooltip(
                                    Tooltip::new("Replace this texture")
                                        .shortcut(tgg_ui::shortcut("secondary-i")),
                                )
                                .on_press(|_, _| {}),
                        )
                        .child(
                            Button::new("save", "Save DAT")
                                .variant(ButtonVariant::Primary)
                                .tooltip(Tooltip::new("Write the patched DAT"))
                                .on_press(|_, _| {}),
                        ),
                )
                .child(heading("Buttons, medium"))
                .child(buttons(ButtonSize::Md, "md"))
                .child(heading("Buttons, small"))
                .child(buttons(ButtonSize::Sm, "sm"))
                .child(heading("Dialog"))
                .child(row().child(
                    Button::new("open-dialog", "Close PlFcRe.dat…").on_press(set_dialog(true)),
                ))
                .child(heading("Disabled"))
                .child(row().child(Button::new("disabled", "Save DAT").disabled(true)))
                .child(
                    div()
                        .font_family(font::MONO)
                        .text_size(rem(text::XS))
                        .text_color(palette.muted.to_gpui())
                        .child("PlFcRe.dat · 256×256 CMPR · tab to see focus rings"),
                );

        tgg_ui::focus_navigation(div().id("gallery"))
            .size_full()
            .bg(palette.bg.to_gpui())
            .text_color(palette.text.to_gpui())
            .font_family(font::SANS)
            .child(
                Split::new(self.split.clone())
                    .left(div().size_full().bg(palette.surface.to_gpui()).child(
                        ThumbnailList::new("textures", self.thumbnails.clone(), self.selected, {
                            let this = cx.entity();
                            move |index, _, cx| {
                                this.update(cx, |gallery, cx| {
                                    gallery.selected = Some(index);
                                    cx.notify();
                                })
                            }
                        }),
                    ))
                    .center(center)
                    .right(panel("Inspector", "Same here, on the right.")),
            )
            .when(self.dialog_open, |root| {
                root.child(
                    Dialog::new("unsaved", "Save changes to PlFcRe.dat?", set_dialog(false))
                        .description("3 textures changed. Unsaved edits are lost if you close.")
                        .action("Cancel", ButtonVariant::Ghost, set_dialog(false))
                        .action("Don't save", ButtonVariant::Danger, set_dialog(false))
                        .action("Save", ButtonVariant::Primary, set_dialog(false)),
                )
            })
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().any(|argument| argument == name);
    let appearance = if flag("--paper") {
        Appearance::Paper
    } else {
        Appearance::Gallery
    };
    let dialog_open = flag("--dialog");
    gpui_platform::application().run(move |cx: &mut App| {
        tgg_ui::fonts::load(cx).expect("load the bundled fonts");
        tgg_ui::init(cx);
        Theme::init(appearance, cx);
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("tgg-ui gallery".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.0), px(640.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                Theme::global(cx).apply(window);
                let split = cx.new(|_| SplitState {
                    left: PaneSize::new(240.0, 180.0, 400.0),
                    right: PaneSize::new(280.0, 220.0, 440.0),
                });
                cx.new(|_| Gallery {
                    split,
                    dialog_open,
                    thumbnails: sample_thumbnails(),
                    selected: Some(0),
                })
            },
        )
        .expect("open the gallery window");
        cx.activate(true);
    });
}
