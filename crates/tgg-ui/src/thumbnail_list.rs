//! A virtualized list of image rows: a thumbnail, a title, and a detail line.
//! Only visible rows render, so it stays fast for hundreds of textures.
//!
//! Selection belongs to the caller. Clicking a row or pressing Up or Down
//! while the list has focus asks the caller to select it, and keyboard moves
//! scroll the selection into view, as does a selection the caller makes
//! (from a click in the viewport, say).

use crate::tokens::{density, font, radius, space};
use crate::{Palette, Theme, rem};
use gpui::prelude::FluentBuilder;
use gpui::{
    AbsoluteLength, App, ElementId, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, ObjectFit, ParentElement, RenderImage, RenderOnce, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, UniformListScrollHandle, Window, div, img, px,
    uniform_list,
};
use std::rc::Rc;
use std::sync::Arc;

/// Row height and thumbnail size, in web pixels.
const ROW: f32 = 56.0;
const THUMBNAIL: f32 = 40.0;

/// An image gpui can draw, from row-major RGBA8 pixels.
pub fn render_image(rgba: &[u8], width: u32, height: u32) -> Option<Arc<RenderImage>> {
    // gpui images are BGRA.
    let mut bgra = rgba.to_vec();
    for texel in bgra.as_chunks_mut::<4>().0 {
        texel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

/// Free `images` from every window's image atlas. gpui keeps a drawn image
/// uploaded until it is dropped by hand, so replacing one without this leaks
/// its texture. The drop waits until the current update is done, when every
/// window, including one being updated now, can be reached.
pub fn drop_images(images: impl IntoIterator<Item = Arc<RenderImage>>, cx: &mut App) {
    let images: Vec<_> = images.into_iter().collect();
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

/// Size of one checkerboard square, and of the pre-rendered checker image.
const CHECKER_SQUARE: u32 = 8;
const CHECKER_SIZE: u32 = 512;

thread_local! {
    static CHECKERBOARDS: std::cell::RefCell<Vec<(Palette, Arc<RenderImage>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A checkerboard in the palette's surface and raise colors, shown behind
/// textures so transparent texels read as transparent. Drawn at its native
/// size and clipped, so squares stay 8 px in any box; built once per palette.
pub fn checkerboard(palette: Palette) -> Arc<RenderImage> {
    CHECKERBOARDS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, image)) = cache.iter().find(|(cached, _)| *cached == palette) {
            return image.clone();
        }
        let channels = |color: crate::Color| {
            let [r, g, b, _] = color.0.to_be_bytes();
            [r, g, b, 255]
        };
        let (light, dark) = (channels(palette.raise), channels(palette.surface));
        let rgba: Vec<u8> = (0..CHECKER_SIZE * CHECKER_SIZE)
            .flat_map(|texel| {
                let (x, y) = (texel % CHECKER_SIZE, texel / CHECKER_SIZE);
                if (x / CHECKER_SQUARE + y / CHECKER_SQUARE).is_multiple_of(2) {
                    light
                } else {
                    dark
                }
            })
            .collect();
        let image =
            render_image(&rgba, CHECKER_SIZE, CHECKER_SIZE).expect("a square checker image");
        cache.push((palette, image.clone()));
        image
    })
}

/// `image` over a checkerboard, filling its parent (which must be relative),
/// with the parent's corner `radius`. gpui clips children to a rectangle, so
/// both layers round their own corners.
pub fn over_checkerboard(
    image: Arc<RenderImage>,
    palette: Palette,
    radius: impl Into<AbsoluteLength> + Copy,
) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .child(
            img(checkerboard(palette))
                .absolute()
                .inset_0()
                .size_full()
                .rounded(radius)
                .object_fit(ObjectFit::None),
        )
        .child(
            img(image)
                .absolute()
                .inset_0()
                .size_full()
                .rounded(radius)
                .object_fit(ObjectFit::Contain),
        )
}

/// One row of a [`ThumbnailList`].
#[derive(Clone)]
pub struct ThumbnailItem {
    pub image: Option<Arc<RenderImage>>,
    pub title: SharedString,
    pub detail: SharedString,
}

impl ThumbnailItem {
    /// Show `image` in place of the current one, freeing the old one.
    pub fn replace_image(&mut self, image: Option<Arc<RenderImage>>, cx: &mut App) {
        let old = std::mem::replace(&mut self.image, image);
        drop_images(old, cx);
    }
}

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

struct ListState {
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    /// The selection last rendered, to notice when the caller changes it.
    shown: Option<usize>,
}

#[derive(IntoElement)]
pub struct ThumbnailList {
    id: ElementId,
    items: Rc<[ThumbnailItem]>,
    selected: Option<usize>,
    on_select: SelectHandler,
}

impl ThumbnailList {
    pub fn new(
        id: impl Into<ElementId>,
        items: impl Into<Rc<[ThumbnailItem]>>,
        selected: Option<usize>,
        on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            items: items.into(),
            selected,
            on_select: Rc::new(on_select),
        }
    }
}

impl RenderOnce for ThumbnailList {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let palette = Theme::global(cx).palette;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ListState {
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            shown: None,
        });
        if state.read(cx).shown != self.selected {
            state.update(cx, |state, _| state.shown = self.selected);
            if let Some(selected) = self.selected {
                state
                    .read(cx)
                    .scroll
                    .scroll_to_item(selected, ScrollStrategy::Nearest);
            }
        }
        let (focus, scroll) = {
            let state = state.read(cx);
            (state.focus.clone(), state.scroll.clone())
        };

        let count = self.items.len();
        let selected = self.selected;
        let key_select = self.on_select.clone();
        let key_scroll = scroll.clone();
        let items = self.items.clone();
        let on_select = self.on_select.clone();
        let rows = uniform_list((self.id.clone(), "rows"), count, move |range, _, _| {
            range
                .map(|index| {
                    let item = &items[index];
                    let select = on_select.clone();
                    let is_selected = selected == Some(index);
                    div()
                        .id(("thumbnail", index))
                        .w_full()
                        .h(rem(ROW))
                        .px(rem(space::XS))
                        .flex()
                        .items_center()
                        .gap(rem(space::SM))
                        .rounded(rem(radius::SM))
                        .cursor_pointer()
                        .when(is_selected, |row| {
                            row.bg(palette.raise.to_gpui())
                                .border_1()
                                .border_color(palette.line_strong.to_gpui())
                        })
                        .when(!is_selected, |row| {
                            row.hover(|style| style.bg(palette.raise.to_gpui()))
                        })
                        .on_click(move |_, window, cx| select(index, window, cx))
                        .child(
                            div()
                                .size(rem(THUMBNAIL))
                                .flex_none()
                                .rounded(rem(radius::SM))
                                .overflow_hidden()
                                .bg(palette.bg.to_gpui())
                                .border_1()
                                .border_color(palette.line.to_gpui())
                                .relative()
                                .when_some(item.image.clone(), |thumbnail, image| {
                                    thumbnail.child(over_checkerboard(
                                        image,
                                        palette,
                                        rem(radius::SM),
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(rem(density::CONTROL_TEXT))
                                        .font_weight(FontWeight::NORMAL)
                                        .truncate()
                                        .child(item.title.clone()),
                                )
                                .child(
                                    div()
                                        .font_family(font::MONO)
                                        .text_size(rem(density::DETAIL_TEXT))
                                        .text_color(palette.muted.to_gpui())
                                        .truncate()
                                        .child(item.detail.clone()),
                                ),
                        )
                })
                .collect()
        })
        .track_scroll(&scroll)
        .size_full();

        div()
            .id(self.id)
            .track_focus(&crate::pressable::tab_stop(&focus))
            .tab_index(0)
            .size_full()
            .p(rem(space::XXS))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if count == 0 {
                    return;
                }
                let next = match (event.keystroke.key.as_str(), selected) {
                    ("down", Some(index)) => (index + 1).min(count - 1),
                    ("up", Some(index)) => index.saturating_sub(1),
                    ("down" | "up", None) => 0,
                    _ => return,
                };
                cx.stop_propagation();
                key_scroll.scroll_to_item(next, ScrollStrategy::Center);
                key_select(next, window, cx);
            })
            .child(rows)
    }
}
