//! Every skin in the player's library, as cards grouped by the fighter each
//! was made for: its render, where it goes, whether it's in their game now,
//! and a way to install or remove it. Tabs narrow it to what's in the game
//! or what isn't.

use crate::costumes::{CostumesEvent, Notice, slot_label};
use crate::install::SlotState;
use crate::library::Skin;
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, EventEmitter, ExternalPaths, InteractiveElement, IntoElement,
    ParentElement, Render, RenderImage, SharedString, StatefulInteractiveElement, Styled, Window,
    div,
};
use std::collections::HashMap;
use std::sync::Arc;
use tgg_ui::page_header::INSET;
use tgg_ui::tokens::{radius, space, text};
use tgg_ui::{
    Button, ButtonSize, ButtonVariant, Card, CardLayout, CardStatus, IconButton, IconName,
    PageHeader, Tabs, Theme, rem,
};

pub(crate) enum LibraryEvent {
    /// The same requests the costume list makes: add, install.
    Costumes(CostumesEvent),
    /// Take the skin with this id out of the library.
    Remove(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    InGame,
    NotInstalled,
}

pub(crate) struct LibraryPage {
    pub skins: Vec<Skin>,
    /// What each slot of the player's game holds, by file.
    pub states: HashMap<String, SlotState>,
    pub notice: Option<Notice>,
    /// Each skin's render, by id, as they arrive.
    pub renders: HashMap<String, Arc<RenderImage>>,
    filter: Filter,
}

impl EventEmitter<LibraryEvent> for LibraryPage {}

impl LibraryPage {
    pub fn new(
        skins: Vec<Skin>,
        states: HashMap<String, SlotState>,
        renders: HashMap<String, Arc<RenderImage>>,
    ) -> Self {
        Self {
            skins,
            states,
            notice: None,
            renders,
            filter: Filter::All,
        }
    }

    fn installed(&self, skin: &Skin) -> bool {
        skin.slot.as_ref().is_some_and(|slot| {
            matches!(self.states.get(slot), Some(SlotState::Skin(there)) if there.id == skin.id)
        })
    }

    /// The skins the tab shows, grouped by the fighter they were made for
    /// (in the order they were added, newest first), stages and skins
    /// without a slot last.
    fn groups(&self) -> Vec<(String, Vec<&Skin>)> {
        let mut groups: Vec<(String, Vec<&Skin>)> = Vec::new();
        for skin in self.skins.iter().rev() {
            let shown = match self.filter {
                Filter::All => true,
                Filter::InGame => self.installed(skin),
                Filter::NotInstalled => !self.installed(skin),
            };
            if !shown {
                continue;
            }
            let group = match skin.slot.as_deref() {
                Some(slot) if slot.starts_with("Pl") => slot_label(Some(slot))
                    .split(" · ")
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                Some(_) => "Stages".to_owned(),
                None => "Without a slot".to_owned(),
            };
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, skins)) => skins.push(skin),
                None => groups.push((group, vec![skin])),
            }
        }
        let last = |name: &str| matches!(name, "Stages" | "Without a slot");
        groups.sort_by_key(|(name, _)| last(name));
        groups
    }

    fn card(&self, skin: &Skin, cx: &mut Context<Self>) -> AnyElement {
        let palette = Theme::global(cx).palette;
        let installed = self.installed(skin);
        // "Red", or the stage's name: the fighter is the group's heading.
        let place = slot_label(skin.slot.as_deref());
        let place = place.rsplit(" · ").next().unwrap_or_default().to_owned();
        let install = skin.slot.clone().filter(|_| !installed).map(|slot| {
            let this = cx.entity();
            let id = skin.id.clone();
            Button::new(
                SharedString::from(format!("install-{}", skin.id)),
                "Install",
            )
            .size(ButtonSize::Sm)
            .on_press(move |_, cx| {
                let event = LibraryEvent::Costumes(CostumesEvent::Install {
                    skin: id.clone(),
                    slot: slot.clone(),
                });
                this.update(cx, |_, cx| cx.emit(event))
            })
        });
        let remove = {
            let this = cx.entity();
            let id = skin.id.clone();
            IconButton::new(
                SharedString::from(format!("remove-{}", skin.id)),
                IconName::Close,
                "Remove from your library",
            )
            .small()
            .on_press(move |_, cx| {
                let id = id.clone();
                this.update(cx, |_, cx| cx.emit(LibraryEvent::Remove(id)))
            })
        };
        Card::new(
            SharedString::from(format!("skin-{}", skin.id)),
            CardLayout::Tile,
            skin.name.clone(),
        )
        .image(self.renders.get(&skin.id).cloned())
        .detail(place)
        .corner(remove)
        .map(|card| match install {
            Some(install) => card.control(install),
            None => card.status(CardStatus {
                label: "In game".into(),
                dot: Some(palette.success),
                accent: false,
            }),
        })
        .into_any_element()
    }

    /// No skins yet: where they come from, and a way to add some.
    fn empty(&self, cx: &mut Context<Self>) -> Div {
        let palette = Theme::global(cx).palette;
        let choose = cx.entity();
        div().px(rem(INSET)).pt(rem(space::MD)).child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(rem(space::SM))
                .py(rem(space::XXL))
                .rounded(rem(radius::LG))
                .border_1()
                .border_color(palette.line_strong.to_gpui())
                .child(
                    div()
                        .text_size(rem(text::MD))
                        .child("Drop skins anywhere on this window"),
                )
                .child(
                    div()
                        .text_size(rem(text::XS))
                        .text_color(palette.muted.to_gpui())
                        .child("Costume and stage .dat files, or .zip files of them."),
                )
                .child(
                    Button::new("choose-skins", "Choose files…")
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Sm)
                        .on_press(move |_, cx| {
                            choose.update(cx, |_, cx| {
                                cx.emit(LibraryEvent::Costumes(CostumesEvent::Choose))
                            })
                        }),
                ),
        )
    }
}

impl Render for LibraryPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let in_game = self
            .skins
            .iter()
            .filter(|skin| self.installed(skin))
            .count();
        let total = self.skins.len();
        let choose = cx.entity();
        let header = PageHeader::new("Library")
            .line(match (total, in_game) {
                (0, _) => "Skins you add are kept here, ready to install.".to_owned(),
                (1, 1) => "1 skin, in your game.".to_owned(),
                (1, _) => "1 skin.".to_owned(),
                (total, 0) => format!("{total} skins, none in your game."),
                (total, in_game) => format!("{total} skins, {in_game} in your game."),
            })
            .action(
                Button::new("add-skins", "Add skins…")
                    .size(ButtonSize::Sm)
                    .on_press(move |_, cx| {
                        choose.update(cx, |_, cx| {
                            cx.emit(LibraryEvent::Costumes(CostumesEvent::Choose))
                        })
                    }),
            );
        let this = cx.entity();
        let tabs = Tabs::new(
            "library-tabs",
            match self.filter {
                Filter::All => 0,
                Filter::InGame => 1,
                Filter::NotInstalled => 2,
            },
            move |tab, _, cx| {
                this.update(cx, |page, cx| {
                    page.filter = [Filter::All, Filter::InGame, Filter::NotInstalled][tab];
                    cx.notify();
                })
            },
        )
        .tab("All", Some(total))
        .tab("In your game", Some(in_game))
        .tab("Not installed", Some(total - in_game));
        let groups: Vec<AnyElement> = self
            .groups()
            .into_iter()
            .map(|(name, skins)| {
                let cards: Vec<AnyElement> =
                    skins.into_iter().map(|skin| self.card(skin, cx)).collect();
                div()
                    .flex()
                    .flex_col()
                    .gap(rem(space::SM))
                    .child(
                        div()
                            .text_size(rem(text::XS))
                            .text_color(palette.muted.to_gpui())
                            .child(name),
                    )
                    .child(div().flex().flex_wrap().gap(rem(space::SM)).children(cards))
                    .into_any_element()
            })
            .collect();
        let empty_tab = groups.is_empty() && total > 0;
        div()
            .id("library")
            .size_full()
            .flex()
            .flex_col()
            .on_drop(cx.listener(|_, paths: &ExternalPaths, _, cx| {
                cx.emit(LibraryEvent::Costumes(CostumesEvent::Add(
                    paths.paths().to_vec(),
                )));
            }))
            .child(header)
            .when_some(self.notice.clone(), |page, notice| {
                page.child(
                    div()
                        .flex_none()
                        .px(rem(INSET))
                        .pb(rem(space::SM))
                        .text_size(rem(text::XS))
                        .text_color(if notice.error {
                            palette.danger.to_gpui()
                        } else {
                            palette.accent_text.to_gpui()
                        })
                        .child(notice.text),
                )
            })
            .map(|page| {
                if total == 0 {
                    return page.child(self.empty(cx));
                }
                page.child(div().flex_none().px(rem(INSET)).child(tabs))
                    .child(
                        div()
                            .id("skins")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(rem(space::LG))
                                    .px(rem(INSET))
                                    .pt(rem(space::MD + space::XXS))
                                    .pb(rem(space::XL))
                                    .children(groups)
                                    .when(empty_tab, |list| {
                                        list.child(
                                            div()
                                                .text_size(rem(text::XS))
                                                .text_color(palette.muted.to_gpui())
                                                .child(match self.filter {
                                                    Filter::InGame => {
                                                        "None of your skins are in your game."
                                                    }
                                                    _ => "Every skin is in your game.",
                                                }),
                                        )
                                    }),
                            ),
                    )
            })
    }
}
