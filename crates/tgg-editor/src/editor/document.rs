//! The open document: reading a DAT into its textures and their names,
//! the selected texture, and showing an edit's pixels.

use super::Editor;
use super::view::StatusLine;
use crate::Error;
use crate::References;
use crate::open_file::OpenFile;
use crate::timeline::Timeline;
use crate::viewport::{Viewport, ViewportEvent};
use dat_edit::{DocumentError, TextureDocument, TextureEdit};
use dat_parser::hsd::scene::HsdTextureSourceId;
use gpui::{AppContext, Context, Entity, Subscription};
use melee_dat::MeleeReferenceStore;
use melee_dat::fighter::places::{CostumePlaces, TexturePlace};
use melee_dat::stage::StageTextureNames;
use std::path::PathBuf;
use std::rc::Rc;
use tgg_ui::ThumbnailItem;

impl Editor {
    /// Open `dat` as the document and list its textures.
    pub(super) fn set_document(&mut self, dat: Vec<u8>, cx: &mut Context<Self>) {
        let listing = Listing::read(dat, self.references.as_deref(), self.store.as_deref());
        self.document = listing.document;
        self.places = listing.places;
        self.names = listing.names;
        self.document_opened(cx);
    }

    /// Show the document just opened: its colors and thumbnails, with its
    /// first texture selected.
    pub(super) fn document_opened(&mut self, cx: &mut Context<Self>) {
        self.read_colors();
        let thumbnails = match &self.document {
            Ok(document) => thumbnails(document, &self.names),
            Err(error) => {
                crate::log(&format!("textures unavailable: {error}"));
                Rc::from([])
            }
        };
        let old = std::mem::replace(&mut self.thumbnails, thumbnails);
        tgg_ui::drop_images(old.iter().filter_map(|item| item.image.clone()), cx);
        // Texture indices belong to the old document.
        self.previewing = None;
        self.external.clear();
        self.select((!self.thumbnails.is_empty()).then_some(0), cx);
    }

    /// Replace the open file: its document, model, and viewport.
    pub(crate) fn replace_file(
        &mut self,
        name: &str,
        dat: Vec<u8>,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<(), Error> {
        let file = OpenFile::show(name, dat, self.references.as_deref(), cx)?;
        self.store = file.store;
        let (title, viewport) = (file.title, file.viewport);
        self.status = cx.new(|cx| StatusLine::new(viewport.clone(), cx));
        self.timeline = cx.new(|cx| Timeline::new(viewport.clone(), cx));
        self._picks = Self::subscribe_picks(&viewport, cx);
        self.viewport = viewport;
        self.read_moves(cx);
        self.title = title.into();
        self.name = name.to_owned();
        self.path = path;
        self.set_document(file.bytes, cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn highlights_selection(&self) -> bool {
        self.settings.highlight_selection.unwrap_or(true)
    }

    /// Tint the selected texture's surfaces in the viewport, or stop.
    pub(super) fn toggle_highlight(&mut self, cx: &mut Context<Self>) {
        self.settings.highlight_selection = Some(!self.highlights_selection());
        self.persist_settings(cx);
        self.sync_highlight(cx);
        cx.notify();
    }

    /// The texture being inspected, if any.
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Inspect `texture` (or nothing): the one way the selection changes. It
    /// tints the texture's surfaces in the viewport and previews an animation
    /// frame in place, so the viewport never lags the list.
    pub(crate) fn select(&mut self, texture: Option<usize>, cx: &mut Context<Self>) {
        self.selected = texture;
        self.sync_highlight(cx);
        self.sync_frame_preview(cx);
        cx.notify();
    }

    /// Keep the viewport's tint on the selected texture.
    fn sync_highlight(&mut self, cx: &mut Context<Self>) {
        // An animation frame tints the images it stands in for.
        let uses = match (&self.document, self.selected) {
            (Ok(document), Some(selected)) if self.highlights_selection() => document
                .textures()
                .get(selected)
                .map(|texture| texture.drawn_as())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        self.viewport
            .update(cx, |viewport, cx| viewport.set_highlight(&uses, cx));
    }

    /// Show a selected animation frame on the model where its animation
    /// would, and put back what was there when the selection moves on.
    fn sync_frame_preview(&mut self, cx: &mut Context<Self>) {
        let Ok(document) = &self.document else {
            return;
        };
        // Only frames the scene doesn't already draw need a stand-in.
        let frame_of = |texture: usize| {
            document
                .textures()
                .get(texture)?
                .frame
                .as_ref()
                .filter(|frame| frame.frame > 0 && !frame.replaces.is_empty())
        };
        let wanted = self
            .selected
            .filter(|&selected| frame_of(selected).is_some());
        if wanted == self.previewing {
            return;
        }
        let mut decoded = Vec::new();
        if let Some(frame) = self.previewing.and_then(frame_of) {
            // Restore each image from the texture that holds it.
            for id in &frame.replaces {
                let holder = document
                    .textures()
                    .iter()
                    .enumerate()
                    .find_map(|(index, texture)| {
                        Some((index, texture.uses.iter().position(|used| used == id)?))
                    });
                if let Some((index, pixels)) = holder
                    .and_then(|(index, usage)| Some((index, document.pixels(index, usage).ok()?)))
                {
                    decoded.push((*id, texture_size(document, index), pixels));
                }
            }
        }
        if let Some(texture) = wanted
            && let (Some(frame), Ok(pixels)) = (frame_of(texture), document.pixels(texture, 0))
        {
            let size = texture_size(document, texture);
            decoded.extend(frame.replaces.iter().map(|&id| (id, size, pixels.clone())));
        }
        self.viewport.update(cx, |viewport, cx| {
            viewport.update_textures(
                decoded
                    .iter()
                    .map(|(id, size, pixels)| (*id, *size, pixels.as_slice())),
                cx,
            )
        });
        self.previewing = wanted;
    }

    pub(super) fn subscribe_picks(
        viewport: &Entity<Viewport>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(
            viewport,
            |editor, _, event: &ViewportEvent, cx| match event {
                ViewportEvent::Picked(textures) => editor.select_picked(textures, cx),
                ViewportEvent::TextureRejected(error) => editor.set_notice(
                    format!("The viewport couldn't show an edit: {error}"),
                    true,
                    cx,
                ),
            },
        )
    }

    /// Select the texture a click landed on. A surface can blend several
    /// textures; clicking one whose texture is already selected moves to the
    /// next, and past the last unselects.
    pub(crate) fn select_picked(
        &mut self,
        textures: &[Vec<HsdTextureSourceId>],
        cx: &mut Context<Self>,
    ) {
        let Ok(document) = &self.document else {
            return;
        };
        let mut candidates: Vec<usize> = Vec::new();
        for ids in textures {
            let found = document
                .textures()
                .iter()
                .position(|texture| ids.iter().any(|id| texture.uses.contains(id)));
            if let Some(index) = found
                && !candidates.contains(&index)
            {
                candidates.push(index);
            }
        }
        if candidates.is_empty() {
            if !textures.is_empty() {
                self.set_notice("That surface's texture isn't in the list.", false, cx);
            }
            return;
        }
        // Clicking the selected texture moves to the surface's next one, and
        // past its last one unselects.
        let at = self
            .selected
            .and_then(|selected| candidates.iter().position(|&index| index == selected));
        let place = match at {
            Some(at) if at + 1 == candidates.len() => {
                self.select(None, cx);
                self.set_notice("Selection cleared", false, cx);
                return;
            }
            Some(at) => at + 1,
            None => 0,
        };
        let next = candidates[place];
        self.select(Some(next), cx);
        let text = if candidates.len() > 1 {
            format!(
                "{}: {} of {} on this surface; click again for the next",
                self.texture_name(next),
                place + 1,
                candidates.len()
            )
        } else {
            self.texture_name(next)
        };
        self.set_notice(text, false, cx);
    }

    /// A texture's name: where a stock costume draws it and its number
    /// ("Head #14"), or just the number.
    pub(crate) fn texture_name(&self, texture: usize) -> String {
        self.names
            .get(texture)
            .cloned()
            .unwrap_or_else(|| texture_name(texture, None))
    }

    /// Where a stock fighter draws `texture`, when that's known.
    pub(crate) fn place(&self, texture: usize) -> Option<TexturePlace> {
        self.places.get(texture).copied().flatten()
    }

    /// Show an applied edit: every decoded use in the viewport, and the
    /// texture's thumbnail and preview.
    pub(crate) fn show_edit(&mut self, texture: usize, edit: &TextureEdit, cx: &mut Context<Self>) {
        self.show_pixels(texture, &edit.decoded, cx);
    }

    pub(super) fn show_pixels(
        &mut self,
        texture: usize,
        decoded: &[(HsdTextureSourceId, Vec<u8>)],
        cx: &mut Context<Self>,
    ) {
        let Ok(document) = &self.document else {
            return;
        };
        let size = texture_size(document, texture);
        self.viewport.update(cx, |viewport, cx| {
            viewport.update_textures(
                decoded
                    .iter()
                    .map(|(id, pixels)| (*id, size, pixels.as_slice())),
                cx,
            )
        });
        // A previewed frame that changed shows its new pixels in place.
        if self.previewing == Some(texture) {
            self.previewing = None;
            self.sync_frame_preview(cx);
        }
        if let Some((_, pixels)) = decoded.first() {
            let mut thumbnails = self.thumbnails.to_vec();
            thumbnails[texture].replace_image(tgg_ui::render_image(pixels, size.0, size.1), cx);
            self.thumbnails = thumbnails.into();
        }
        cx.notify();
    }
}

/// A DAT read as a document: its textures, where a stock fighter draws
/// each, and what each is called.
pub(super) struct Listing {
    pub document: Result<TextureDocument, DocumentError>,
    pub places: Vec<Option<TexturePlace>>,
    pub names: Vec<String>,
}

impl Listing {
    /// Read `dat`. A stock costume's `store` names its textures by where
    /// the fighter draws them; a stock stage names its own.
    pub(super) fn read(
        dat: Vec<u8>,
        references: Option<&References>,
        store: Option<&MeleeReferenceStore>,
    ) -> Self {
        let references = references.zip(store);
        // Places come from the same parse as the document.
        let opened = TextureDocument::open_inspecting(dat, |dat, scene| {
            let places = references.map(|(references, store)| {
                CostumePlaces::read(dat, scene, references.catalog, store)
            });
            (places, StageTextureNames::read(dat))
        });
        let (document, places, stage_names) = match opened {
            Ok((document, (places, stage_names))) => (document, places, stage_names),
            Err(error) => {
                return Self {
                    document: Err(error),
                    places: Vec::new(),
                    names: Vec::new(),
                };
            }
        };
        let places = match places {
            Some(Ok(places)) => places,
            Some(Err(error)) => {
                crate::log(&format!("texture names unavailable: {error}"));
                None
            }
            None => None,
        };
        let textures = document.textures();
        let places: Vec<Option<TexturePlace>> = textures
            .iter()
            .map(|texture| places.as_ref()?.place_of(&texture.descriptors()))
            .collect();
        let stage_name =
            |texture: &dat_edit::DocumentTexture| stage_names.name_of(texture.data_offset);
        let names = textures
            .iter()
            .zip(&places)
            .enumerate()
            .map(|(index, (texture, place))| match stage_name(texture) {
                Some(name) => name.to_owned(),
                None => texture_name(index, *place),
            })
            .collect();
        let named = textures
            .iter()
            .zip(&places)
            .filter(|(texture, place)| place.is_some() || stage_name(texture).is_some())
            .count();
        let frames: Vec<String> = textures
            .iter()
            .enumerate()
            .filter_map(|(index, texture)| {
                let frame = texture.frame.as_ref()?;
                Some(format!(
                    "#{} {}/{}",
                    index + 1,
                    frame.frame + 1,
                    frame.frames
                ))
            })
            .collect();
        crate::log(&format!(
            "textures: {}, {named} named, animation frames: {}",
            textures.len(),
            frames.join(", ")
        ));
        Self {
            document: Ok(document),
            places,
            names,
        }
    }
}

/// A document texture's (width, height), which its decoded pixels have.
fn texture_size(document: &TextureDocument, texture: usize) -> (u32, u32) {
    let entry = &document.textures()[texture];
    (entry.width.into(), entry.height.into())
}

fn texture_name(texture: usize, place: Option<TexturePlace>) -> String {
    match place {
        Some(place) => format!("{} #{}", place.label(), texture + 1),
        None => format!("Texture {}", texture + 1),
    }
}

/// A browser row per document texture, decoded through its first use.
pub(crate) fn thumbnails(document: &TextureDocument, names: &[String]) -> Rc<[ThumbnailItem]> {
    document
        .textures()
        .iter()
        .enumerate()
        .map(|(index, texture)| {
            let image = document.pixels(index, 0).ok().and_then(|rgba| {
                tgg_ui::render_image(&rgba, texture.width.into(), texture.height.into())
            });
            let format = texture.format.name();
            let uses = match texture.uses.len() {
                1 => String::new(),
                uses => format!(" · {uses} uses"),
            };
            ThumbnailItem {
                image,
                title: names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| texture_name(index, None))
                    .into(),
                detail: format!(
                    "{}{}×{} · {format}{uses}",
                    texture
                        .frame
                        .as_ref()
                        .map_or_else(String::new, |frame| format!(
                            "frame {}/{} · ",
                            frame.frame + 1,
                            frame.frames
                        )),
                    texture.width,
                    texture.height
                )
                .into(),
            }
        })
        .collect()
}
