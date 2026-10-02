//! File actions: open and save DATs, import and export texture PNGs.
//!
//! The operations work on bytes; file dialogs and the filesystem wrap them.
//! Every action reports its result in the editor's notice line. An import
//! reports its fidelity: how many blocks it re-encoded and how many texels
//! the GX format couldn't reproduce exactly.

use crate::Error;
use crate::disk::{read_capped, write_atomically};
use crate::editor::{Editor, Pending, Unsaved};
use dat_edit::PaletteOutcome;
use dat_parser::hsd::scene::HSD_SCENE_MAX_DAT_BYTES;
use gpui::{Context, PathPromptOptions, Window};
use std::path::{Path, PathBuf};

/// The largest PNG an import reads: far past any GX texture's.
const MAX_PNG_BYTES: u64 = 64 * 1024 * 1024;

impl Editor {
    /// Open DAT bytes, replacing the current file.
    pub(crate) fn open_bytes(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        match self.replace_file(name, bytes, path, cx) {
            Ok(()) => self.set_notice(format!("Opened {name}"), false, cx),
            Err(error) => self.set_notice(format!("Couldn't open {name}: {error}"), true, cx),
        }
    }

    /// The selected texture, or a notice asking for one.
    fn selected_for(&mut self, verb: &str, cx: &mut Context<Self>) -> Option<usize> {
        if self.selected().is_none() {
            self.set_notice(format!("Select a texture to {verb}."), true, cx);
        }
        self.selected()
    }

    /// Replace `texture` with a PNG, reporting the fidelity of the result.
    pub(crate) fn import_bytes(
        &mut self,
        texture: usize,
        name: &str,
        png: &[u8],
        cx: &mut Context<Self>,
    ) {
        let Ok(document) = &mut self.document else {
            return;
        };
        let entry = document.textures()[texture].clone();
        // The size comes from the header, so a wrong-sized image is turned
        // away before its pixels are decoded.
        let reader = || image::ImageReader::new(std::io::Cursor::new(png)).with_guessed_format();
        let dimensions = reader()
            .map_err(image::ImageError::IoError)
            .and_then(image::ImageReader::into_dimensions);
        let (width, height) = match dimensions {
            Ok(dimensions) => dimensions,
            Err(error) => {
                self.set_notice(format!("Couldn't read {name}: {error}"), true, cx);
                return;
            }
        };
        if (width, height) != (entry.width.into(), entry.height.into()) {
            self.set_notice(
                format!(
                    "{name} is {width}×{height}; this texture is {}×{}.",
                    entry.width, entry.height
                ),
                true,
                cx,
            );
            return;
        }
        let decoded = reader()
            .map_err(image::ImageError::IoError)
            .and_then(image::ImageReader::decode);
        let image = match decoded {
            Ok(image) => image.to_rgba8(),
            Err(error) => {
                self.set_notice(format!("Couldn't read {name}: {error}"), true, cx);
                return;
            }
        };
        // A CI texture that owns its palette gets one rebuilt for the new
        // pixels; otherwise they map to the colors it has.
        let result = document.import(texture, image.as_raw());
        match result {
            Ok(edit) => {
                let texels = u32::from(entry.width) * u32::from(entry.height);
                let report = if edit.lossy_texels == 0 {
                    "every texel exact".to_owned()
                } else {
                    format!(
                        "{} of {texels} texels approximated (at most {} off in a channel)",
                        edit.lossy_texels, edit.max_channel_error
                    )
                };
                let palette = match &edit.palette {
                    Some(PaletteOutcome::Rebuilt { colors }) => {
                        format!(", palette rebuilt with {colors} colors")
                    }
                    Some(PaletteOutcome::Kept(lock)) => format!(", palette kept because {lock}"),
                    None => String::new(),
                };
                self.show_edit(texture, &edit, cx);
                self.set_notice(
                    format!(
                        "Imported {name}: {} of {} blocks changed{palette}, {report}",
                        edit.patch.changed_blocks, edit.patch.blocks
                    ),
                    false,
                    cx,
                );
            }
            Err(error) => self.set_notice(format!("Couldn't import {name}: {error}"), true, cx),
        }
    }

    /// `texture` encoded as a PNG.
    pub(crate) fn texture_png(&self, texture: usize) -> Result<Vec<u8>, Error> {
        let document = self.document.as_ref().map_err(|_| Error::NoDocument)?;
        let entry = &document.textures()[texture];
        let pixels = document.pixels(texture, 0)?;
        let image = image::RgbaImage::from_raw(entry.width.into(), entry.height.into(), pixels)
            .ok_or(Error::PixelSize)?;
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)?;
        Ok(png)
    }

    /// The suggested name for an exported texture: `PlFcRe-14-head.png`, or
    /// `PlFcRe-texture-14.png` when it has no place.
    pub(crate) fn export_name(&self, texture: usize) -> String {
        let stem = self
            .name
            .rsplit_once('.')
            .map_or(&*self.name, |(stem, _)| stem);
        let place = self.place(texture);
        match place {
            Some(place) => {
                let slug: String = place
                    .label()
                    .to_lowercase()
                    .split(|character: char| !character.is_alphanumeric())
                    .filter(|word| !word.is_empty())
                    .collect::<Vec<_>>()
                    .join("-");
                format!("{stem}-{}-{slug}.png", texture + 1)
            }
            None => format!("{stem}-texture-{}.png", texture + 1),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Ask for one file to read.
fn prompt_for_file(
    prompt: &'static str,
    cx: &mut Context<Editor>,
) -> impl Future<Output = Option<PathBuf>> + use<> {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(prompt.into()),
    });
    async move {
        let Ok(Ok(Some(paths))) = paths.await else {
            return None;
        };
        paths.into_iter().next()
    }
}

impl Editor {
    /// Where file dialogs start: beside the open file, or the working
    /// directory.
    fn dialog_directory(&self) -> PathBuf {
        self.path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    pub(crate) fn open_dat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = prompt_for_file("Open DAT", cx);
        cx.spawn_in(window, async move |editor, cx| {
            if let Some(path) = path.await {
                editor
                    .update_in(cx, |editor, _, cx| editor.open_asking(&path, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Open a DAT from disk once unsaved edits are saved or let go.
    fn open_asking(&mut self, path: &Path, cx: &mut Context<Self>) {
        if self.proceed_or_ask(Pending::Open(path.to_path_buf()), cx) {
            self.open_file(path, cx);
        }
    }

    /// Whether `action` can go ahead now. With unsaved edits it waits on the
    /// "Save changes?" dialog instead.
    pub(crate) fn proceed_or_ask(&mut self, action: Pending, cx: &mut Context<Self>) -> bool {
        let proceed = hold_unless_saved(self.is_modified(), &mut self.unsaved, action);
        if !proceed {
            cx.notify();
        }
        proceed
    }

    /// Answer the "Save changes?" dialog: save then continue, continue
    /// without saving, or cancel and keep everything.
    pub(crate) fn resolve_unsaved(
        &mut self,
        answer: Unsaved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(action) = self.unsaved.take() else {
            return;
        };
        cx.notify();
        match answer {
            Unsaved::Cancel => cx.emit(crate::editor::EditorEvent::Stay),
            Unsaved::Discard => self.continue_with(action, window, cx),
            Unsaved::Save if self.slot.is_some() => self.save_to_library(false, Some(action), cx),
            Unsaved::Save => match self.path.clone() {
                Some(path) => {
                    if self.write(&path, cx) {
                        self.continue_with(action, window, cx);
                    }
                }
                // A file without a path yet is saved as, then continues.
                None => self.save_as_then(Some(action), window, cx),
            },
        }
    }

    /// Ask the app to keep the document as a skin in the library, and with
    /// `install` to put it into the slot being edited too, then carry out
    /// `then`.
    pub(crate) fn save_to_library(
        &mut self,
        install: bool,
        then: Option<Pending>,
        cx: &mut Context<Self>,
    ) {
        let Ok(document) = &self.document else {
            return;
        };
        let bytes = document.bytes().to_vec();
        cx.emit(crate::editor::EditorEvent::SaveToLibrary {
            bytes,
            install,
            then,
        });
    }

    /// The app kept the document as the skin `name` (and `installed` it):
    /// the edits are saved; carry out `then`.
    pub(crate) fn saved_to_library(
        &mut self,
        name: &str,
        installed: bool,
        then: Option<Pending>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Ok(document) = &mut self.document {
            document.mark_saved();
        }
        let text = if installed {
            format!("Saved {name} and installed it")
        } else {
            format!("Saved {name} to your library")
        };
        self.set_notice(text, false, cx);
        if let Some(then) = then {
            self.continue_with(then, window, cx);
        }
    }

    /// Carry out an action the dialog held.
    fn continue_with(&mut self, action: Pending, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Pending::Close => window.remove_window(),
            Pending::Leave => cx.emit(crate::editor::EditorEvent::Leave),
            Pending::Open(path) => self.open_file(&path, cx),
        }
    }

    /// Open a DAT from disk, replacing the current file.
    pub(crate) fn open_file(&mut self, path: &Path, cx: &mut Context<Self>) {
        let name = file_name(path);
        match read_capped(path, HSD_SCENE_MAX_DAT_BYTES as u64) {
            Ok(bytes) => self.open_bytes(&name, bytes, Some(path.to_path_buf()), cx),
            Err(error) => self.set_notice(format!("Couldn't open {name}: {error}"), true, cx),
        }
    }

    /// Files dropped on the window: a DAT opens, a PNG imports into the
    /// selected texture.
    pub(crate) fn drop_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let Some(path) = paths.first() else {
            return;
        };
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
        match extension.as_deref() {
            Some("dat" | "usd") => self.open_asking(path, cx),
            Some("png") => {
                if let Some(texture) = self.selected_for("drop a PNG onto", cx) {
                    self.import_file(texture, path, cx);
                }
            }
            _ => self.set_notice(
                format!(
                    "Drop a DAT to open it, or a PNG to replace the selected texture; {} is neither.",
                    file_name(path)
                ),
                true,
                cx,
            ),
        }
    }

    /// Write the document to `path`, which becomes its own. Returns whether
    /// it was written.
    fn write(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        let name = file_name(path);
        let Ok(document) = &mut self.document else {
            self.set_notice("There's no document to save.", true, cx);
            return false;
        };
        match write_atomically(path, document.bytes()) {
            Ok(()) => {
                document.mark_saved();
                self.path = Some(path.to_path_buf());
                self.title = retitled(&self.title, &self.name, &name).into();
                self.name = name.clone();
                self.set_notice(format!("Saved {name}"), false, cx);
                true
            }
            Err(error) => {
                self.set_notice(format!("Couldn't save {name}: {error}"), true, cx);
                false
            }
        }
    }

    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.slot.is_some() {
            self.save_to_library(false, None, cx);
        } else if let Some(path) = self.path.clone() {
            self.write(&path, cx);
        } else {
            self.save_as(window, cx);
        }
    }

    pub(crate) fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save_as_then(None, window, cx);
    }

    /// Save As, then carry out `then` once the file is written.
    fn save_as_then(&mut self, then: Option<Pending>, window: &mut Window, cx: &mut Context<Self>) {
        let path = cx.prompt_for_new_path(&self.dialog_directory(), Some(&self.name));
        cx.spawn_in(window, async move |editor, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            editor
                .update_in(cx, |editor, window, cx| {
                    if editor.write(&path, cx)
                        && let Some(then) = then
                    {
                        editor.continue_with(then, window, cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    pub(crate) fn import_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(texture) = self.selected_for("import into", cx) else {
            return;
        };
        let path = prompt_for_file("Import PNG", cx);
        cx.spawn_in(window, async move |editor, cx| {
            if let Some(path) = path.await {
                editor
                    .update_in(cx, |editor, _, cx| editor.import_file(texture, &path, cx))
                    .ok();
            }
        })
        .detach();
    }

    /// Replace `texture` with a PNG from disk.
    pub(crate) fn import_file(&mut self, texture: usize, path: &Path, cx: &mut Context<Self>) {
        let name = file_name(path);
        match read_capped(path, MAX_PNG_BYTES) {
            Ok(png) => self.import_bytes(texture, &name, &png, cx),
            Err(error) => self.set_notice(format!("Couldn't read {name}: {error}"), true, cx),
        }
    }

    pub(crate) fn export_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(texture) = self.selected_for("export", cx) else {
            return;
        };
        let path =
            cx.prompt_for_new_path(&self.dialog_directory(), Some(&self.export_name(texture)));
        cx.spawn_in(window, async move |editor, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            editor
                .update_in(cx, |editor, _, cx| editor.export_file(texture, &path, cx))
                .ok();
        })
        .detach();
    }

    fn export_file(&mut self, texture: usize, path: &Path, cx: &mut Context<Self>) {
        let name = file_name(path);
        let result = self
            .texture_png(texture)
            .and_then(|png| Ok(write_atomically(path, &png)?));
        match result {
            Ok(()) => self.set_notice(format!("Exported {name}"), false, cx),
            Err(error) => self.set_notice(format!("Couldn't export {name}: {error}"), true, cx),
        }
    }
}

/// Hold `action` in `slot` when there are unsaved edits; otherwise it can go
/// ahead. A newer request replaces one already waiting.
fn hold_unless_saved(modified: bool, slot: &mut Option<Pending>, action: Pending) -> bool {
    if modified {
        *slot = Some(action);
    }
    !modified
}

/// The title with its file-name part renamed: `PlFcRe.dat · Falco`
/// saved as `mine.dat` becomes `mine.dat · Falco`.
fn retitled(title: &str, old_name: &str, new_name: &str) -> String {
    let rest = match title.strip_prefix(old_name) {
        Some(rest) if rest.is_empty() || rest.starts_with(" · ") => rest,
        _ => title.find(" · ").map_or("", |at| &title[at..]),
    };
    format!("{new_name}{rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retitling_renames_the_file_part() {
        assert_eq!(
            retitled("PlFcRe.dat · Falco", "PlFcRe.dat", "mine.dat"),
            "mine.dat · Falco"
        );
        assert_eq!(
            retitled("costume.dat · bind pose", "other.dat", "mine.dat"),
            "mine.dat · bind pose"
        );
        assert_eq!(retitled("PlFcRe.dat", "PlFcRe.dat", "mine.dat"), "mine.dat");
    }
}
