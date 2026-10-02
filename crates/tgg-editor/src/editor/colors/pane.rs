//! The Colors pane: each group's swatches, and the sliders for the color
//! being edited.

use super::{ColorGroup, ColorPick, Source, Target, hex};
use crate::editor::{Editor, PaneKind};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use std::rc::Rc;
use tgg_ui::tokens::{density, font, radius, space};
use tgg_ui::{Button, ButtonSize, ButtonVariant, Pane, PaneFit, Slider, Theme, pressable, rem};

/// The most swatches a group shows; the rest are reached by the hue shift.
const SHOWN: usize = 24;

/// A swatch's side, and the pane's tallest list, in web pixels.
const SWATCH: f32 = 22.0;

const LIST_HEIGHT: f32 = 300.0;

impl Editor {
    /// The groups to list: the selected texture's, or all of them when it
    /// has none (or nothing is selected).
    fn shown_groups(&self) -> Vec<usize> {
        let of_selected: Vec<usize> = self
            .selected()
            .map(|selected| {
                self.color_groups
                    .iter()
                    .enumerate()
                    .filter(|(_, group)| group.textures.contains(&selected))
                    .map(|(index, _)| index)
                    .collect()
            })
            .unwrap_or_default();
        if of_selected.is_empty() {
            (0..self.color_groups.len()).collect()
        } else {
            of_selected
        }
    }

    pub(crate) fn colors_pane(&self, window: &mut Window, cx: &mut Context<Self>) -> Pane {
        let palette = Theme::global(cx).palette;
        let shown = self.shown_groups();
        let swatches: usize = shown
            .iter()
            .map(|&group| self.color_groups[group].swatches.len())
            .sum();
        let mut body = div()
            .id("colors")
            .max_h(rem(LIST_HEIGHT))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(rem(space::SM))
            .px(rem(space::SM))
            .pb(rem(space::SM));
        if shown.is_empty() {
            body = body.child(
                div()
                    .text_size(rem(density::CONTROL_TEXT))
                    .text_color(palette.muted.to_gpui())
                    .child("Every color here is in a texture."),
            );
        }
        for group in shown {
            body = body.child(self.group_element(group, window, cx));
        }
        self.pane_card(PaneKind::Colors, "Colors", PaneFit::Content, cx)
            .count(swatches.to_string())
            .child(body)
    }

    fn group_element(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = Theme::global(cx).palette;
        let group = &self.color_groups[index];
        let picked = self.color_pick.as_ref().filter(|pick| pick.group == index);
        let picked_swatch = picked.and_then(|pick| match pick.target {
            Target::Swatch { index, .. } => Some(index),
            Target::Hue(_) => None,
        });
        let squares = group
            .swatches
            .iter()
            .enumerate()
            .take(SHOWN)
            .map(|(swatch_index, swatch)| {
                let this = cx.entity();
                let [r, g, b, _] = swatch.rgba;
                pressable(
                    ("swatch", index * 4096 + swatch_index),
                    Some(Rc::new(move |_: &mut Window, cx: &mut gpui::App| {
                        this.update(cx, |editor, cx| editor.pick_swatch(index, swatch_index, cx))
                    })),
                    window,
                    cx,
                )
                .flex_none()
                .size(rem(SWATCH))
                .rounded(rem(radius::SM / 2.0))
                .border_1()
                .border_color(if picked_swatch == Some(swatch_index) {
                    palette.accent_text.to_gpui()
                } else {
                    palette.line_strong.to_gpui()
                })
                .bg(gpui::rgb(u32::from_be_bytes([0, r, g, b])))
            })
            .collect::<Vec<_>>();
        let hidden = group.swatches.len().saturating_sub(SHOWN);
        let shift = (!group.vertex_surfaces.is_empty() && group.swatches.len() > 1).then(|| {
            let this = cx.entity();
            Button::new(("shift-hue", index), "Shift hue")
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .on_press(move |_, cx| this.update(cx, |editor, cx| editor.pick_hue(index, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap(rem(space::XXS))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rem(space::XS))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(rem(density::CONTROL_TEXT))
                            .child(group.title.clone()),
                    )
                    .children(shift),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(rem(space::XXS))
                    .children(squares)
                    .children((hidden > 0).then(|| {
                        div()
                            .text_size(rem(density::DETAIL_TEXT))
                            .text_color(palette.muted.to_gpui())
                            .child(format!("+{hidden}"))
                    })),
            )
            .children(picked.map(|pick| self.pick_element(group, pick, cx)))
            .into_any_element()
    }

    /// The controls for the color being edited, under its group.
    fn pick_element(
        &self,
        group: &ColorGroup,
        pick: &ColorPick,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let palette = Theme::global(cx).palette;
        let label = |text: String| {
            div()
                .flex_none()
                .w(rem(space::XL + space::XS))
                .font_family(font::MONO)
                .text_size(rem(density::DETAIL_TEXT))
                .text_color(palette.muted.to_gpui())
                .child(text)
        };
        let row = |name: &'static str, value: String, slider: Slider| {
            div()
                .flex()
                .items_center()
                .gap(rem(space::XS))
                .child(label(name.into()))
                .child(div().flex_1().min_w_0().child(slider))
                .child(label(value))
        };
        let mut controls = div()
            .flex()
            .flex_col()
            .gap(rem(space::XXS))
            .p(rem(space::XS))
            .rounded(rem(radius::SM))
            .bg(palette.raise.to_gpui());
        let changed = match pick.target {
            Target::Swatch { index, rgba } => {
                let Some(swatch) = group.swatches.get(index) else {
                    return div().into_any_element();
                };
                let chip = |[r, g, b, _]: [u8; 4]| {
                    div()
                        .flex_none()
                        .w(rem(space::XL))
                        .h(rem(SWATCH))
                        .rounded(rem(radius::SM / 2.0))
                        .border_1()
                        .border_color(palette.line_strong.to_gpui())
                        .bg(gpui::rgb(u32::from_be_bytes([0, r, g, b])))
                };
                controls = controls.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rem(space::XS))
                        .child(chip(swatch.rgba))
                        .child(chip(rgba))
                        .child(
                            div()
                                .font_family(font::MONO)
                                .text_size(rem(density::CONTROL_TEXT))
                                .child(hex(rgba)),
                        )
                        .child(
                            div()
                                .text_size(rem(density::DETAIL_TEXT))
                                .text_color(palette.muted.to_gpui())
                                .child(match &swatch.source {
                                    Source::Vertices(_) => format!("{} vertices", swatch.uses),
                                    Source::Diffuse(_) => "material".to_owned(),
                                }),
                        ),
                );
                let channels: &[&'static str] = if swatch.alpha {
                    &["R", "G", "B", "A"]
                } else {
                    &["R", "G", "B"]
                };
                for (channel, &name) in channels.iter().enumerate() {
                    let this = cx.entity();
                    controls = controls.child(row(
                        name,
                        rgba[channel].to_string(),
                        Slider::new(
                            ("color-channel", channel),
                            f32::from(rgba[channel]),
                            255.0,
                            1.0,
                            move |value, _, cx| {
                                this.update(cx, |editor, cx| editor.set_channel(channel, value, cx))
                            },
                        ),
                    ));
                }
                rgba != swatch.rgba
            }
            Target::Hue(degrees) => {
                let this = cx.entity();
                controls = controls.child(row(
                    "Hue",
                    format!("{degrees:+.0}°"),
                    // The slider runs from 0; the turn from -180 to 180.
                    Slider::new(
                        "color-hue",
                        degrees + 180.0,
                        360.0,
                        1.0,
                        move |value, _, cx| {
                            this.update(cx, |editor, cx| editor.set_hue(value - 180.0, cx))
                        },
                    ),
                ));
                degrees != 0.0
            }
        };
        let apply = cx.entity();
        let cancel = cx.entity();
        controls
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(rem(space::XS))
                    .pt(px(2.0))
                    .child(
                        Button::new("color-cancel", "Cancel")
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Sm)
                            .on_press(move |_, cx| {
                                cancel.update(cx, |editor, cx| editor.cancel_pick(cx))
                            }),
                    )
                    .child(
                        Button::new("color-apply", "Apply")
                            .variant(ButtonVariant::Primary)
                            .size(ButtonSize::Sm)
                            .disabled(!changed)
                            .on_press(move |_, cx| {
                                apply.update(cx, |editor, cx| editor.apply_pick(cx))
                            }),
                    ),
            )
            .into_any_element()
    }
}
