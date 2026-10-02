//! Checking files before they join the library: each dropped or chosen file
//! with the fighter and color it seems made for, which the player can
//! change, and files the app can't use with the reason. The player then adds
//! them to the library, or adds and installs them.

use crate::costumes::{Fighter, slot_label};
use crate::library::{Candidate, Rejected};
use gpui::prelude::FluentBuilder;
use gpui::{
    Context, EventEmitter, FontWeight, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div,
};
use melee_dat::MeleeSlot;
use tgg_ui::tokens::{space, text};
use tgg_ui::{ButtonVariant, Dialog, MenuButton, MenuItem, Theme, rem};

/// A file to add, and the slot the player has settled on for it.
pub(crate) struct ReviewItem {
    pub candidate: Candidate,
    pub slot: Option<MeleeSlot>,
}

pub(crate) enum ReviewEvent {
    Cancel,
    /// Add the files to the library; with `install`, into their slots too.
    Confirm {
        install: bool,
    },
}

pub(crate) struct Review {
    pub items: Vec<ReviewItem>,
    pub rejected: Vec<Rejected>,
    /// The fighters in the player's game, for choosing a slot.
    pub fighters: Vec<Fighter>,
}

impl EventEmitter<ReviewEvent> for Review {}

impl Review {
    /// The fighter a costume slot belongs to, among the player's fighters.
    fn fighter_of(&self, slot: Option<MeleeSlot>) -> Option<&Fighter> {
        let character = slot?.character()?;
        self.fighters
            .iter()
            .find(|fighter| fighter.character == character)
    }

    /// Menus to choose item `index`'s fighter and color.
    fn slot_menus(&self, index: usize, cx: &mut Context<Self>) -> gpui::Div {
        let slot = self.items[index].slot;
        let fighter = self.fighter_of(slot);
        let color = slot.and_then(MeleeSlot::color);
        let fighters = self.fighters.iter().fold(
            MenuButton::new(
                SharedString::from(format!("fighter-{index}")),
                fighter.map_or("Choose fighter", |fighter| fighter.character.name()),
            )
            .select(),
            |menu, choice| {
                let this = cx.entity();
                // Keep the color when the new fighter has it.
                let target = choice
                    .costumes
                    .iter()
                    .find(|costume| Some(costume.color) == color)
                    .or_else(|| choice.costumes.first())
                    .map(|costume| costume.slot(choice));
                menu.item(
                    MenuItem::new(choice.character.name(), move |_, cx| {
                        this.update(cx, |review, cx| {
                            review.items[index].slot = target;
                            cx.notify();
                        })
                    })
                    .checked(fighter.is_some_and(|fighter| fighter.character == choice.character)),
                )
            },
        );
        let colors = fighter.map(|fighter| {
            fighter.costumes.iter().fold(
                MenuButton::new(
                    SharedString::from(format!("color-{index}")),
                    color.map_or("Color", |color| color.name()),
                )
                .select(),
                |menu, costume| {
                    let this = cx.entity();
                    let target = costume.slot(fighter);
                    menu.item(
                        MenuItem::new(costume.color.name(), move |_, cx| {
                            this.update(cx, |review, cx| {
                                review.items[index].slot = Some(target);
                                cx.notify();
                            })
                        })
                        .checked(slot == Some(target)),
                    )
                },
            )
        });
        let stage = slot
            .filter(|slot| matches!(slot, MeleeSlot::Stage(_)))
            .map(|slot| slot_label(Some(slot)));
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(rem(space::XS))
            .map(|menus| match stage {
                Some(stage) => menus.child(stage),
                None => menus.child(fighters).children(colors),
            })
    }
}

impl Render for Review {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let emit = |event: fn() -> ReviewEvent, cx: &mut Context<Self>| {
            let this = cx.entity();
            move |_: &mut Window, cx: &mut gpui::App| this.update(cx, |_, cx| cx.emit(event()))
        };
        let rows = (0..self.items.len())
            .map(|index| {
                let menus = self.slot_menus(index, cx);
                let item = &self.items[index];
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(rem(space::MD))
                    .py(rem(space::XS))
                    .border_b_1()
                    .border_color(palette.line.to_gpui())
                    .child(
                        div().flex().flex_col().min_w_0().child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(item.candidate.name.clone()),
                        ),
                    )
                    .child(menus)
            })
            .collect::<Vec<_>>();
        let rejected = self.rejected.iter().map(|rejected| {
            div()
                .py(rem(space::XS))
                .text_size(rem(text::SM))
                .text_color(palette.danger.to_gpui())
                .child(format!(
                    "Can't add {}: {}",
                    rejected.file_name, rejected.reason
                ))
        });
        let title = match self.items.as_slice() {
            [] => "Nothing to add".to_owned(),
            [item] => format!("Add {}", item.candidate.name),
            items => format!("Add {} skins", items.len()),
        };
        let mut dialog = Dialog::new("review", title, emit(|| ReviewEvent::Cancel, cx))
            .body(div().flex().flex_col().children(rows).children(rejected))
            .action(
                if self.items.is_empty() {
                    "Close"
                } else {
                    "Cancel"
                },
                ButtonVariant::Ghost,
                emit(|| ReviewEvent::Cancel, cx),
            );
        if !self.items.is_empty() {
            dialog = dialog
                .action(
                    "Add",
                    ButtonVariant::Secondary,
                    emit(|| ReviewEvent::Confirm { install: false }, cx),
                )
                .action(
                    "Add and install",
                    ButtonVariant::Primary,
                    emit(|| ReviewEvent::Confirm { install: true }, cx),
                );
        }
        dialog
    }
}
