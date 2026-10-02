//! The player's skins in the shell: adding them to the library, installing
//! them into the game's slots, saving an edited one, and the slot renders
//! that show what each holds.

use super::{Place, Screen, Shell};
use crate::Error;
use crate::costumes::{Notice, roster, slot_label};
use crate::editor::{Editor, Pending};
use crate::game::Game;
use crate::install::{History, SlotState, install, restore_vanilla, undo};
use crate::library::{Library, SkinSource};
use crate::renders::SIZE;
use crate::review::{Review, ReviewEvent, ReviewItem};
use crate::{References, log};
use gpui::{AppContext, Context, Entity, PathPromptOptions, Window};
use melee_dat::MeleeReferenceCatalog;
use melee_dat::STAGES;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tgg_ui::{drop_images, render_image};

impl Shell {
    /// Take renders as they arrive: keep each, and show it on the costume
    /// list.
    pub(super) fn receive_renders(&self, cx: &mut Context<Self>) {
        let rendered = self.renders.rendered.clone();
        cx.spawn(async move |shell, cx| {
            while let Ok(rendered) = rendered.recv().await {
                let Some(image) = render_image(&rendered.rgba, SIZE, SIZE) else {
                    continue;
                };
                let alive = shell.update(cx, |shell, cx| {
                    let old = shell.images.insert(rendered.file.clone(), image.clone());
                    drop_images(old, cx);
                    // Recent edits show renders too.
                    if shell.recent.contains(&rendered.file) {
                        cx.notify();
                    }
                    if let Screen::Library(page) = &shell.screen {
                        page.update(cx, |page, cx| {
                            page.renders.insert(rendered.file.clone(), image.clone());
                            cx.notify();
                        });
                    }
                    let page = match &shell.screen {
                        Screen::Game(page) => Some(page.clone()),
                        _ => shell.editing_from.clone(),
                    };
                    if let Some(page) = page {
                        page.update(cx, |page, cx| {
                            page.renders.insert(rendered.file, image);
                            cx.notify();
                        });
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// Ask for renders of the game's slots that don't have one yet.
    pub(super) fn request_renders(&self, files: &[String]) {
        let Some(game) = self.game() else {
            return;
        };
        for file in files {
            if !self.images.contains_key(file) {
                self.renders.request(game.path(), file);
            }
        }
    }

    /// Ask for renders of the library's costumes that don't have one yet,
    /// drawn with the game's references.
    pub(super) fn request_skin_renders(&self, library: &Library) {
        let Some(game) = self.game() else {
            return;
        };
        for skin in library.skins() {
            let Some(slot) = skin
                .slot
                .as_deref()
                .filter(|slot| slot.starts_with("Pl") || slot.starts_with("Gr"))
            else {
                continue;
            };
            if !self.images.contains_key(&skin.id) {
                self.renders
                    .request_skin(game.path(), &skin.id, library.blob_path(&skin.id), slot);
            }
        }
    }

    /// Forget the render of slot `file` (its file changed), and ask again.
    fn rerender(&mut self, file: &str, cx: &mut Context<Self>) {
        drop_images(self.images.remove(file), cx);
        self.request_renders(&[file.to_owned()]);
    }

    /// The slots of the game with an install to undo.
    pub(super) fn undoable(&self) -> HashSet<String> {
        self.game().map_or_else(HashSet::new, |game| {
            let history = History::open();
            game.file_names()
                .into_iter()
                .filter(|file| history.can_undo(game, file))
                .collect()
        })
    }

    /// Keep the editor's document as a skin in the library, and with
    /// `install_it` put it into the slot it was opened from, then tell the
    /// editor.
    pub(super) fn save_to_library(
        &mut self,
        editor: &Entity<Editor>,
        bytes: &[u8],
        install_it: bool,
        then: Option<Pending>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(slot) = editor.read(cx).slot.clone() else {
            return;
        };
        let Some(references) = self.references.clone() else {
            return;
        };
        let game = references.game();
        let history = History::open();
        let saved = (|| {
            let before = game.read(&slot)?;
            let (name, from) = match SlotState::of(&slot, &before, &self.library.borrow()) {
                SlotState::Skin(skin) => (edited_name(&skin.name), Some(skin.id)),
                _ => (edited_name(&slot_label(Some(&slot))), None),
            };
            let mut candidate = self
                .library
                .borrow()
                .inspect(&slot, bytes.to_vec(), &references.catalog)
                .map_err(|rejected| Error::Rejected(rejected.reason))?;
            candidate.name = name.clone();
            self.library.borrow_mut().store(
                &candidate,
                Some(slot.clone()),
                SkinSource::Edited { from },
            )?;
            if install_it {
                install(game, &slot, bytes, &self.library.borrow(), &history)?;
            }
            Ok::<_, Error>(name)
        })();
        match saved {
            Ok(name) => {
                if install_it {
                    self.rerender(&slot, cx);
                }
                self.refresh_game_page(cx);
                editor.update(cx, |editor, cx| {
                    editor.saved_to_library(&name, install_it, then, window, cx)
                });
            }
            Err(error) => editor.update(cx, |editor, cx| {
                editor.set_notice(format!("Couldn't save: {error}"), true, cx)
            }),
        }
    }

    /// Bring Your game's slot states up to date, whether it's showing or
    /// waiting under the editor.
    pub(super) fn refresh_game_page(&mut self, cx: &mut Context<Self>) {
        let page = match &self.screen {
            Screen::Game(page) => Some(page.clone()),
            _ => self.editing_from.clone(),
        };
        let Some(page) = page else {
            return;
        };
        let library = self.library.clone();
        let library = library.borrow();
        let states = self
            .game()
            .map_or_else(HashMap::new, |game| slot_states(game, &library));
        let undoable = self.undoable();
        page.update(cx, |page, cx| {
            page.skins = library.skins().to_vec();
            page.states = states;
            page.undoable = undoable;
            cx.notify();
        });
    }

    /// Look at `paths` (DATs, or zips of them) and ask the player where
    /// each goes before any joins the library.
    pub(super) fn add_skins(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let checked_in;
        let catalog = match self.references.as_deref() {
            Some(references) => &references.catalog,
            None => match MeleeReferenceCatalog::checked_in() {
                Ok(catalog) => {
                    checked_in = catalog;
                    &checked_in
                }
                Err(error) => {
                    log(&format!("no reference catalog: {error}"));
                    return;
                }
            },
        };
        let (items, rejected): (Vec<_>, Vec<_>) = self
            .library
            .borrow()
            .inspect_files(paths, catalog)
            .into_iter()
            .partition(Result::is_ok);
        let items = items
            .into_iter()
            .flatten()
            .map(|candidate| ReviewItem {
                slot: candidate.slot.clone(),
                candidate,
            })
            .collect();
        let fighters = self.game().map_or_else(Vec::new, |game| {
            roster(game.file_names().iter().map(String::as_str))
        });
        let review = cx.new(|_| Review {
            items,
            rejected: rejected.into_iter().filter_map(Result::err).collect(),
            fighters,
        });
        let events = cx.subscribe_in(&review, window, |shell, review, event, window, cx| {
            match event {
                ReviewEvent::Cancel => {}
                ReviewEvent::Confirm { install } => {
                    let items = review.update(cx, |review, _| std::mem::take(&mut review.items));
                    shell.add_reviewed(items, *install, window, cx);
                }
            }
            shell.review = None;
            cx.notify();
        });
        self.review = Some((review, events));
        cx.notify();
    }

    /// Keep the reviewed files in the library for the slots the player chose
    /// and, with `install`, install each into its slot; then say what
    /// happened.
    fn add_reviewed(
        &mut self,
        items: Vec<ReviewItem>,
        install_them: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let references = self.references.clone();
        let game = references.as_deref().map(References::game);
        let history = History::open();
        let mut done = Vec::new();
        let mut failed = Vec::new();
        let mut installed = Vec::new();
        for item in items {
            let stored = self.library.borrow_mut().store(
                &item.candidate,
                item.slot.clone(),
                SkinSource::Imported,
            );
            let skin = match stored {
                Ok(skin) => skin,
                Err(error) => {
                    failed.push(format!("{}: {error}", item.candidate.name));
                    continue;
                }
            };
            let place = slot_label(skin.slot.as_deref());
            match (install_them, game, skin.slot.as_deref()) {
                (true, Some(game), Some(slot)) => {
                    let library = self.library.borrow();
                    match install(game, slot, &item.candidate.bytes, &library, &history) {
                        Ok(()) => {
                            installed.push(slot.to_owned());
                            done.push(format!("installed {} into {place}", skin.name))
                        }
                        Err(error) => failed.push(format!("{}: {error}", skin.name)),
                    }
                }
                (true, _, None) => failed.push(format!(
                    "{} was added but has no slot to install into",
                    skin.name
                )),
                _ => done.push(format!("added {} ({place})", skin.name)),
            }
        }
        for slot in &installed {
            self.rerender(slot, cx);
        }
        let installed_slots = installed;
        let text = done
            .iter()
            .chain(&failed)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let notice = (!text.is_empty()).then(|| Notice {
            text: capitalize(&text).into(),
            error: done.is_empty() && !failed.is_empty(),
        });
        self.show_with(self.place, notice, &installed_slots, window, cx);
    }

    pub(super) fn remove_skin(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let notice = match self.library.borrow_mut().remove(id) {
            Ok(skin) => Notice {
                text: format!("Removed {} from your library.", skin.name).into(),
                error: false,
            },
            Err(error) => Notice {
                text: error.to_string().into(),
                error: true,
            },
        };
        self.show_with(Place::Library, Some(notice), &[], window, cx);
    }

    /// Change what slot `slot` of the player's game holds, then show the
    /// place afresh with what happened.
    pub(super) fn change_slot(
        &mut self,
        slot: &str,
        change: Change,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(references) = self.references.clone() else {
            return;
        };
        let game = references.game();
        let library = self.library.borrow();
        let history = History::open();
        let place = slot_label(Some(slot));
        let result = match &change {
            Change::Install(id) => library
                .read(id)
                .and_then(|bytes| install(game, slot, &bytes, &library, &history))
                .map(|()| {
                    let name = library
                        .skins()
                        .iter()
                        .find(|skin| &skin.id == id)
                        .map_or("the skin", |skin| skin.name.as_str());
                    format!("Installed {name} into {place}.")
                }),
            Change::Undo => undo(game, slot, &library, &history)
                .map(|()| format!("Put back what {place} had before.")),
            Change::Restore => {
                let others: Vec<PathBuf> = self
                    .games
                    .iter()
                    .map(|choice| choice.path.clone())
                    .collect();
                restore_vanilla(game, slot, &others, &library, &history)
                    .map(|()| format!("{place} is vanilla again."))
            }
        };
        drop(library);
        let changed = if result.is_ok() {
            self.rerender(slot, cx);
            vec![slot.to_owned()]
        } else {
            Vec::new()
        };
        let notice = match result {
            Ok(text) => Notice {
                text: text.into(),
                error: false,
            },
            Err(error) => Notice {
                text: error.to_string().into(),
                error: true,
            },
        };
        self.show_with(self.place, Some(notice), &changed, window, cx);
    }

    /// Ask for skins to add, then review them.
    pub(super) fn choose_skins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add skins".into()),
        });
        cx.spawn_in(window, async move |shell, cx| {
            if let Ok(Ok(Some(paths))) = paths.await {
                shell
                    .update_in(cx, |shell, window, cx| shell.add_skins(&paths, window, cx))
                    .ok();
            }
        })
        .detach();
    }
}

/// A skin's name once edited: "Waffle Falco (edited)", not edited twice.
fn edited_name(name: &str) -> String {
    format!("{} (edited)", name.trim_end_matches(" (edited)"))
}

/// `text` with its first letter capitalized.
fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// A change to one slot of the player's game.
pub(super) enum Change {
    /// Install the library's skin with this id.
    Install(String),
    Undo,
    Restore,
}

/// What each costume slot of `game` holds.
pub(super) fn slot_states(game: &Game, library: &Library) -> HashMap<String, SlotState> {
    slot_files(game)
        .into_iter()
        .filter_map(|file| {
            let bytes = game.read(&file).ok()?;
            let state = SlotState::of(&file, &bytes, library);
            Some((file, state))
        })
        .collect()
}

/// Every slot the game page shows: each fighter's costumes, then the versus
/// stages the disc has.
pub(super) fn slot_files(game: &Game) -> Vec<String> {
    let names = game.file_names();
    let costumes = roster(names.iter().map(String::as_str))
        .into_iter()
        .flat_map(|fighter| fighter.costumes)
        .map(|costume| costume.file);
    let stages = STAGES
        .iter()
        .map(|(file, _)| *file)
        .filter(|file| names.iter().any(|name| name == file))
        .map(str::to_owned);
    costumes.chain(stages).collect()
}
