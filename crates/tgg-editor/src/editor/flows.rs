//! The editor's flows on a model written out by hand ([`crate::test_dat`]),
//! driven through gpui's test app: these run wherever the tests do, with no
//! game files. The viewport has no GPU here, so they cover everything but
//! drawing. `crate::iso_tests` runs what only a real fighter shows.

use super::{Editor, EditorEvent, Pending, Unsaved};
use crate::open_file::OpenFile;
use crate::settings::Settings;
use crate::test_dat::{self, GREEN, PIXELS, SIZE, YELLOW};
use gpui::{Context, TestAppContext, WindowHandle};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use tgg_ui::{Appearance, Theme};

/// Open the test model in an editor window, as a file on disk at `path`.
fn open(path: Option<PathBuf>, cx: &mut TestAppContext) -> WindowHandle<Editor> {
    cx.update(|cx| {
        Theme::init(Appearance::Gallery, cx);
        tgg_ui::init(cx);
    });
    cx.add_window(move |_, cx| {
        let mut file =
            OpenFile::show("test.dat", test_dat::model(), None, cx).expect("the model loads");
        file.path = path;
        Editor::new(file, None, Settings::default(), None, cx)
    })
}

fn bytes(editor: &Editor) -> Vec<u8> {
    editor
        .document
        .as_ref()
        .expect("a document")
        .bytes()
        .to_vec()
}

fn notice(editor: &Editor) -> String {
    editor
        .notice
        .as_ref()
        .map_or_else(String::new, |notice| notice.text.to_string())
}

/// A PNG of one color.
fn png(size: u32, rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(size, size, image::Rgba(rgba));
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode a PNG");
    png
}

/// The swatch colors of the editor's first color group, in order.
fn swatches(editor: &Editor) -> Vec<[u8; 4]> {
    editor.color_groups[0]
        .swatches
        .iter()
        .map(|swatch| swatch.rgba)
        .collect()
}

/// Every event the editor emits from here on.
fn events(editor: &mut Editor, cx: &mut Context<Editor>) -> Rc<RefCell<Vec<&'static str>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    cx.subscribe(&cx.entity(), move |_, _, event: &EditorEvent, _| {
        record.borrow_mut().push(match event {
            EditorEvent::Leave => "leave",
            EditorEvent::Stay => "stay",
            EditorEvent::SaveToLibrary { .. } => "save to library",
        });
    })
    .detach();
    let _ = editor;
    seen
}

#[gpui::test]
fn opening_lists_the_texture_and_the_colors_under_it(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, _| {
            assert_eq!(editor.selected(), Some(0));
            assert_eq!(editor.texture_name(0), "Texture 1");
            assert!(!editor.is_modified());
            assert!(!editor.home(), "no game, so nowhere to go back to");
            assert_eq!(swatches(editor), [GREEN, YELLOW]);
            assert_eq!(editor.color_groups[0].swatches[0].uses, 2);
            assert_eq!(editor.color_groups[0].textures, [0]);
            assert_eq!(editor.export_name(0), "test-texture-1.png");
        })
        .unwrap();
}

#[gpui::test]
fn an_import_undoes_and_redoes_to_the_exact_file(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            let original = bytes(editor);
            editor.import_bytes(0, "red.png", &png(SIZE, [255, 0, 0, 255]), cx);
            assert!(
                notice(editor).starts_with("Imported red.png")
                    && notice(editor).ends_with("every texel exact"),
                "{}",
                notice(editor)
            );
            assert!(editor.is_modified());
            let imported = bytes(editor);
            // RGBA8 holds alpha and red, then green and blue.
            assert_eq!(imported[0x20 + PIXELS..][..2], [0xFF, 0xFF]);
            assert_eq!(imported[0x20 + PIXELS + 32..][..2], [0x00, 0x00]);

            editor.step_history(false, cx);
            assert_eq!(bytes(editor), original);
            assert!(!editor.is_modified());
            assert_eq!(notice(editor), "Undid the edit to Texture 1");

            editor.step_history(true, cx);
            assert_eq!(bytes(editor), imported);
            editor.step_history(true, cx);
            assert_eq!(notice(editor), "Nothing to redo.");
        })
        .unwrap();
}

#[gpui::test]
fn an_import_of_the_wrong_size_or_kind_changes_nothing(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            editor.import_bytes(0, "big.png", &png(16, [255, 0, 0, 255]), cx);
            assert_eq!(notice(editor), "big.png is 16×16; this texture is 8×8.");
            editor.import_bytes(0, "notes.png", b"not a PNG", cx);
            assert!(
                notice(editor).starts_with("Couldn't read notes.png"),
                "{}",
                notice(editor)
            );
            assert!(!editor.is_modified());
        })
        .unwrap();
}

#[gpui::test]
fn an_exported_texture_imports_back_without_a_change(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            let exported = editor.texture_png(0).expect("export");
            editor.import_bytes(0, "same.png", &exported, cx);
            assert!(
                notice(editor).contains("0 of 4 blocks changed"),
                "{}",
                notice(editor)
            );
            assert!(!editor.is_modified());
        })
        .unwrap();
}

#[gpui::test]
fn recoloring_a_swatch_rewrites_its_vertices_and_undoes(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            let original = bytes(editor);
            // The green swatch becomes blue; RGB565 keeps 5 bits of it.
            editor.pick_swatch(0, 0, cx);
            for (channel, value) in [0.0, 0.0, 255.0].into_iter().enumerate() {
                editor.set_channel(channel, value, cx);
            }
            editor.apply_pick(cx);
            assert_eq!(notice(editor), "Recolored Texture 1: #60E060 to #0000FF");
            assert!(editor.is_modified());
            assert_eq!(bytes(editor).len(), original.len());
            // Yellow now has one use to blue's two; the list stays by use.
            assert_eq!(swatches(editor), [[0, 0, 248, 255], YELLOW]);

            editor.step_history(false, cx);
            assert_eq!(bytes(editor), original);
            assert_eq!(swatches(editor), [GREEN, YELLOW]);
            assert_eq!(notice(editor), "Undid a color change");
        })
        .unwrap();
}

#[gpui::test]
fn a_hue_shift_turns_every_vertex_color_of_the_group(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            editor.pick_hue(0, cx);
            editor.set_hue(120.0, cx);
            editor.apply_pick(cx);
            assert_eq!(notice(editor), "Recolored Texture 1: 2 colors by +120°");
            let turned = swatches(editor);
            assert_eq!(turned.len(), 2);
            // Green turns to blue, yellow to cyan.
            assert!(
                turned[0][2] > turned[0][0] && turned[0][2] > turned[0][1],
                "{turned:?}"
            );
            assert!(
                turned[1][0] < turned[1][1] && turned[1][0] < turned[1][2],
                "{turned:?}"
            );
        })
        .unwrap();
}

#[gpui::test]
fn clicking_the_surface_selects_its_texture_and_again_clears_it(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            let uses = editor.document.as_ref().unwrap().textures()[0].uses.clone();
            editor.select(None, cx);
            editor.select_picked(std::slice::from_ref(&uses), cx);
            assert_eq!(editor.selected(), Some(0));
            assert_eq!(notice(editor), "Texture 1");
            editor.select_picked(&[uses], cx);
            assert_eq!(editor.selected(), None);
            assert_eq!(notice(editor), "Selection cleared");
        })
        .unwrap();
}

#[gpui::test]
fn leaving_over_unsaved_edits_waits_for_an_answer(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, window, cx| {
            let events = events(editor, cx);
            assert!(
                editor.proceed_or_ask(Pending::Leave, cx),
                "nothing to lose yet"
            );

            editor.import_bytes(0, "red.png", &png(SIZE, [255, 0, 0, 255]), cx);
            assert!(!editor.proceed_or_ask(Pending::Close, cx));
            assert_eq!(editor.unsaved, Some(Pending::Close));
            // A later request replaces the one waiting.
            assert!(!editor.proceed_or_ask(Pending::Leave, cx));
            assert_eq!(editor.unsaved, Some(Pending::Leave));

            // Cancel keeps the edits and says the editor is staying.
            editor.resolve_unsaved(Unsaved::Cancel, window, cx);
            assert!(editor.unsaved.is_none() && editor.is_modified());

            // Don't save lets go.
            assert!(!editor.proceed_or_ask(Pending::Leave, cx));
            editor.resolve_unsaved(Unsaved::Discard, window, cx);
            events
        })
        .map(|events| {
            cx.run_until_parked();
            assert_eq!(*events.borrow(), ["stay", "leave"]);
        })
        .unwrap();
}

#[gpui::test]
fn saving_writes_the_file_and_clears_unsaved(cx: &mut TestAppContext) {
    let folder = tempfile::tempdir().expect("temp folder");
    let path = folder.path().join("test.dat");
    std::fs::write(&path, test_dat::model()).expect("write the model");
    let window = open(Some(path.clone()), cx);
    window
        .update(cx, |editor, window, cx| {
            editor.import_bytes(0, "red.png", &png(SIZE, [255, 0, 0, 255]), cx);
            editor.save(window, cx);
            assert_eq!(notice(editor), "Saved test.dat");
            assert!(!editor.is_modified());
            assert_eq!(std::fs::read(&path).expect("read back"), bytes(editor));

            // The saved state is the one undo now leaves.
            editor.step_history(false, cx);
            assert!(editor.is_modified());
        })
        .unwrap();
}

#[gpui::test]
fn opening_another_file_replaces_the_document(cx: &mut TestAppContext) {
    let window = open(None, cx);
    window
        .update(cx, |editor, _, cx| {
            editor.open_bytes("broken.dat", vec![0; 8], None, cx);
            assert!(
                notice(editor).starts_with("Couldn't open broken.dat"),
                "{}",
                notice(editor)
            );
            assert_eq!(editor.name, "test.dat", "the open file stays");

            editor.import_bytes(0, "red.png", &png(SIZE, [255, 0, 0, 255]), cx);
            editor.open_bytes("other.dat", test_dat::model(), None, cx);
            assert_eq!(editor.name, "other.dat");
            assert_eq!(notice(editor), "Opened other.dat");
            assert!(!editor.is_modified());
            assert_eq!(editor.selected(), Some(0));
        })
        .unwrap();
}
