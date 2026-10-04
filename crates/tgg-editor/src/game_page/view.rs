//! The game page's window: the roster, a fighter's or stage's slots, and
//! what each slot can do.

use super::{GamePage, Tab};
use crate::costumes::{CostumesEvent, FIGHTER_FILE, slot_label};
use crate::editor::TogglePlayback;
use crate::install::SlotState;
use crate::library::Skin;
use crate::renders::RenderKey;
use crate::stage;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, ExternalPaths, InteractiveElement, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div,
};
use melee_dat::MeleeSlot;
use std::rc::Rc;
use tgg_ui::page_header::INSET;
use tgg_ui::pane::PANE_MARGIN;
use tgg_ui::tokens::{density, space, text};
use tgg_ui::{
    Button, ButtonSize, ButtonVariant, Card, CardLayout, CardStatus, Chip, MenuButton, MenuItem,
    PageHeader, Pane, PaneColumn, PaneFit, Tabs, Theme, rem,
};

impl GamePage {
    /// The fighters or stages as a grid of cards, under the page's header.
    fn roster(&self, cx: &mut Context<Self>) -> Div {
        let palette = Theme::global(cx).palette;
        let this = cx.entity();
        let tabs = Tabs::new(
            "game-tabs",
            match self.tab {
                Tab::Fighters => 0,
                Tab::Stages => 1,
            },
            move |tab, _, cx| {
                this.update(cx, |page, cx| {
                    page.tab = [Tab::Fighters, Tab::Stages][tab];
                    cx.notify();
                })
            },
        )
        .tab("Fighters", Some(self.fighters.len()))
        .tab("Stages", Some(self.stages.len()));
        let status = |custom: usize| CardStatus {
            label: match custom {
                0 => "Vanilla".into(),
                custom => format!("{custom} custom").into(),
            },
            dot: (custom > 0).then_some(palette.accent),
            accent: false,
        };
        let cards: Vec<AnyElement> = match self.tab {
            Tab::Fighters => self
                .fighters
                .iter()
                .enumerate()
                .map(|(index, fighter)| {
                    let custom = fighter.slots().filter(|&slot| self.changed(slot)).count();
                    let image = self
                        .shown_slot(fighter)
                        .and_then(|slot| self.model_for(slot))
                        .and_then(|slot| self.renders.get(&RenderKey::Slot(slot)).cloned());
                    let this = cx.entity();
                    Card::new(
                        ("fighter", index),
                        CardLayout::Tile,
                        fighter.character.name(),
                    )
                    .image(image)
                    .status(status(custom))
                    .on_press(move |_, cx| {
                        this.update(cx, |page, cx| {
                            page.fighter = index;
                            page.open = true;
                            page.show_selected(cx);
                        })
                    })
                    .into_any_element()
                })
                .collect(),
            Tab::Stages => self
                .stages
                .iter()
                .enumerate()
                .map(|(index, &stage)| {
                    let this = cx.entity();
                    let slot = MeleeSlot::Stage(stage);
                    Card::new(("stage", index), CardLayout::Tile, stage.name())
                        .image(self.renders.get(&RenderKey::Slot(slot)).cloned())
                        .status(status(usize::from(self.changed(slot))))
                        .on_press(move |_, cx| {
                            this.update(cx, |page, cx| {
                                page.stage = index;
                                page.open = true;
                                page.show_selected(cx);
                            })
                        })
                        .into_any_element()
                })
                .collect(),
        };
        let changed = self
            .states
            .keys()
            .filter(|&&slot| self.changed(slot))
            .count();
        let choose = cx.entity();
        let header = PageHeader::new("Melee")
            .when_some(self.chip.clone(), |header, chip| {
                let settings = cx.entity();
                header.beside(
                    Chip::new(
                        "game-chip",
                        if chip.slippi {
                            "1.02 · Slippi's ISO"
                        } else {
                            "1.02"
                        },
                    )
                    .when(chip.slippi, |chip| chip.dot(palette.success))
                    .tooltip(format!(
                        "Skins install into {}. Change it in Settings.",
                        chip.file
                    ))
                    .on_press(move |_, cx| {
                        settings.update(cx, |_, cx| cx.emit(CostumesEvent::Settings))
                    }),
                )
            })
            .line(format!(
                "{} fighters and {} stages, {}.",
                self.fighters.len(),
                self.stages.len(),
                match changed {
                    0 => "all vanilla".to_owned(),
                    1 => "1 slot changed from vanilla".to_owned(),
                    changed => format!("{changed} slots changed from vanilla"),
                }
            ))
            .action(
                Button::new("add-skins", "Add skins…")
                    .size(ButtonSize::Sm)
                    .on_press(move |_, cx| {
                        choose.update(cx, |_, cx| cx.emit(CostumesEvent::Choose))
                    }),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .children(
                self.notice_line(cx)
                    .map(|line| line.px(rem(INSET)).pb(rem(space::SM))),
            )
            .child(div().flex_none().px(rem(INSET)).child(tabs))
            .child(
                div()
                    .id("roster")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .content_start()
                            .gap(rem(space::SM))
                            .px(rem(INSET))
                            .pt(rem(space::MD + space::XXS))
                            .pb(rem(space::XL))
                            .children(cards),
                    ),
            )
    }

    fn notice_line(&self, cx: &mut Context<Self>) -> Option<Div> {
        let palette = Theme::global(cx).palette;
        let notice = self.notice.clone()?;
        Some(
            div()
                .text_size(rem(text::XS))
                .text_color(if notice.error {
                    palette.danger.to_gpui()
                } else {
                    palette.accent_text.to_gpui()
                })
                .child(notice.text),
        )
    }

    /// What `slot` holds, and whether that's changed from vanilla.
    fn holds(&self, slot: MeleeSlot) -> (String, bool) {
        match self.states.get(&slot) {
            None | Some(SlotState::Vanilla) => ("Vanilla".to_owned(), false),
            Some(SlotState::Skin(skin)) => (skin.name.clone(), true),
            Some(SlotState::Custom) => ("Custom".to_owned(), true),
        }
    }

    /// The Costumes pane: the fighter's slots (or the stage) as rows, and
    /// what the selected one can do under them.
    fn costumes_pane(&self, cx: &mut Context<Self>) -> Pane {
        let palette = Theme::global(cx).palette;
        let selected = self.selected_slot();
        let rows: Vec<AnyElement> = match self.tab {
            Tab::Fighters => self
                .fighters
                .get(self.fighter)
                .map(|fighter| {
                    fighter
                        .slots()
                        .map(|slot| {
                            let (holds, custom) = self.holds(slot);
                            let this = cx.entity();
                            let character = fighter.character;
                            Card::new(
                                SharedString::from(format!("slot-{slot}")),
                                CardLayout::Row,
                                slot.color().map_or(FIGHTER_FILE, |color| color.name()),
                            )
                            .image(self.renders.get(&RenderKey::Slot(slot)).cloned())
                            .detail(holds)
                            .when(custom, |card| {
                                card.status(CardStatus {
                                    label: "Custom".into(),
                                    dot: Some(palette.accent),
                                    accent: true,
                                })
                            })
                            .selected(selected == Some(slot))
                            .on_press(move |_, cx| {
                                this.update(cx, |page, cx| {
                                    page.slots.insert(character, slot);
                                    page.show_selected(cx);
                                })
                            })
                            .into_any_element()
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Tab::Stages => selected
                .iter()
                .map(|&slot| {
                    let (holds, custom) = self.holds(slot);
                    Card::new("stage-slot", CardLayout::Row, slot_label(Some(slot)))
                        .image(self.renders.get(&RenderKey::Slot(slot)).cloned())
                        .detail(holds)
                        .when(custom, |card| {
                            card.status(CardStatus {
                                label: "Custom".into(),
                                dot: Some(palette.accent),
                                accent: true,
                            })
                        })
                        .selected(true)
                        .into_any_element()
                })
                .collect(),
        };
        let title = match self.tab {
            Tab::Fighters => "Costumes",
            Tab::Stages => "Stage",
        };
        Pane::new("costumes", title, PaneFit::Fill)
            .count(rows.len().to_string())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("slots")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(rem(space::XS - 2.0))
                                    .px(rem(space::XS + 2.0))
                                    .pb(rem(space::XS))
                                    .children(rows),
                            ),
                    )
                    .child(self.slot_actions(cx)),
            )
    }

    /// What the selected slot can do: change skin, undo, restore vanilla.
    fn slot_actions(&self, cx: &mut Context<Self>) -> Div {
        let palette = Theme::global(cx).palette;
        let Some(slot) = self.selected_slot() else {
            return div();
        };
        let emit = |event: CostumesEvent, cx: &mut Context<Self>| {
            let this = cx.entity();
            let event = Rc::new(event);
            move |_: &mut Window, cx: &mut gpui::App| {
                let event = event.clone();
                this.update(cx, |_, cx| cx.emit((*event).clone()))
            }
        };
        let (_, changed) = self.holds(slot);
        // Skins made for any of this fighter's slots, or this stage.
        let installable: Vec<&Skin> = self
            .skins
            .iter()
            .filter(|skin| skin.slot.is_some_and(|made_for| made_for.fits(slot)))
            .collect();
        let change = (!installable.is_empty()).then(|| {
            installable.iter().fold(
                MenuButton::new("install-skin", "Change skin")
                    .select()
                    .size(ButtonSize::Sm),
                |menu, skin| {
                    let installed = matches!(
                        self.states.get(&slot),
                        Some(SlotState::Skin(there)) if there.id == skin.id
                    );
                    let from = skin
                        .slot
                        .filter(|made_for| *made_for != slot)
                        .and_then(MeleeSlot::color)
                        .map(|color| format!(" (made for {})", color.name()))
                        .unwrap_or_default();
                    menu.item(
                        MenuItem::new(
                            format!("{}{from}", skin.name),
                            emit(
                                CostumesEvent::Install {
                                    skin: skin.id,
                                    slot,
                                },
                                cx,
                            ),
                        )
                        .checked(installed),
                    )
                },
            )
        });
        let undo = self.undoable.contains(&slot);
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(rem(space::XS))
            .px(rem(space::MD - 2.0))
            .py(rem(space::SM))
            .border_t_1()
            .border_color(palette.line.to_gpui())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(rem(space::XS))
                    .children(change)
                    .when(undo, |actions| {
                        actions.child(
                            Button::new("undo-slot", "Undo")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_press(emit(CostumesEvent::Undo(slot), cx)),
                        )
                    })
                    .when(changed, |actions| {
                        actions.child(
                            Button::new("restore-slot", "Restore vanilla")
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_press(emit(CostumesEvent::Restore(slot), cx)),
                        )
                    })
                    .when(installable.is_empty() && !undo && !changed, |actions| {
                        actions.child(
                            div()
                                .text_size(rem(density::DETAIL_TEXT))
                                .text_color(palette.muted.to_gpui())
                                .child("Skins you add for it show up here."),
                        )
                    }),
            )
            .children(self.notice_line(cx))
    }

    /// One fighter or stage, moving on the stage, with its Costumes pane
    /// floating at the right.
    fn subject_view(&self, cx: &mut Context<Self>) -> Div {
        let palette = Theme::global(cx).palette;
        let width = self.split.read(cx).right.width;
        let clear = rem(width + PANE_MARGIN * 2.0);
        let stage_area = match (&self.preview, &self.problem) {
            (_, Some(problem)) => div()
                .size_full()
                .p(rem(space::LG))
                .bg(stage::color())
                .text_color(palette.danger.to_gpui())
                .child(problem.clone()),
            (Some(preview), None) => stage::stage(&preview.file.viewport, &preview.timeline, clear),
            (None, None) => div().size_full().bg(stage::color()),
        };
        let pane = self.costumes_pane(cx);
        div().relative().size_full().child(stage_area).child(
            PaneColumn::new(self.split.clone(), PANE_MARGIN).children([pane.into_any_element()]),
        )
    }
}

impl Render for GamePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.open {
            self.subject_view(cx)
        } else {
            self.roster(cx)
        };
        div()
            .id("game")
            .size_full()
            // The timeline's button and Space in the preview ask for this.
            .on_action(cx.listener(|page, _: &TogglePlayback, _, cx| {
                if let Some(preview) = &page.preview {
                    preview
                        .file
                        .viewport
                        .update(cx, |viewport, cx| viewport.toggle_playback(cx));
                }
            }))
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                cx.emit(CostumesEvent::Add(paths.paths().to_vec()));
            }))
            .child(body)
    }
}
