//! The selected texture in and out as a PNG. An import must be the
//! texture's size; it reports its fidelity: how many blocks it re-encoded,
//! how many texels the texture's format couldn't hold exactly, and what
//! happened to a palette.

use super::{Document, actions};
use crate::state::App;
use dat_edit::{PaletteLock, PaletteOutcome, TextureEdit, TextureIndex, UseIndex};
use rune::{FileDialogOptions, Tx};
use std::path::Path;
use tgg_app::textures::texture_size;

/// Ask for a PNG to replace the selected texture with.
pub fn choose_import(app: &mut App, tx: &mut Tx) {
    let Some(texture) = app.editor.as_ref().and_then(|document| document.selected) else {
        return;
    };
    let options = FileDialogOptions {
        title: "Replace the texture with a PNG".into(),
        ..Default::default()
    }
    .filter("PNG image", &["png"]);
    tx.open_file(options, move |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => import_file(app, texture, &path),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// Replace `texture` with the PNG at `path`.
pub fn import_file(app: &mut App, texture: usize, path: &Path) {
    let name = path.file_name().map_or_else(
        || "the image".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    match std::fs::read(path) {
        Ok(png) => import_png(app, texture, &name, &png),
        Err(error) => app.tell(format!("Couldn't read {name}: {error}"), true),
    }
}

/// Replace `texture` with `png`, named `name`, saying how faithfully.
pub fn import_png(app: &mut App, texture: usize, name: &str, png: &[u8]) {
    let Some(size) = size_of(app, texture) else {
        return;
    };
    // The size comes from the header, so a wrong-sized image is turned away
    // before its pixels are decoded.
    let reader = || image::ImageReader::new(std::io::Cursor::new(png)).with_guessed_format();
    let dimensions = reader()
        .map_err(image::ImageError::IoError)
        .and_then(image::ImageReader::into_dimensions);
    let (width, height) = match dimensions {
        Ok(dimensions) => dimensions,
        Err(error) => return app.tell(format!("Couldn't read {name}: {error}"), true),
    };
    if (width, height) != size {
        return app.tell(
            format!(
                "{name} is {width}×{height}; this texture is {}×{}.",
                size.0, size.1
            ),
            true,
        );
    }
    let image = match reader()
        .map_err(image::ImageError::IoError)
        .and_then(image::ImageReader::decode)
    {
        Ok(image) => image.to_rgba8(),
        Err(error) => return app.tell(format!("Couldn't read {name}: {error}"), true),
    };
    match actions::import(app, texture, image.as_raw()) {
        Ok(edit) => app.tell(format!("Imported {name}: {}", fidelity(&edit, size)), false),
        Err(error) => app.tell(format!("Couldn't import {name}: {error}"), true),
    }
}

/// How faithfully an import landed: "3 of 64 blocks changed, every texel
/// exact".
fn fidelity(edit: &TextureEdit, (width, height): (u32, u32)) -> String {
    let texels = width * height;
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
        // Textures are numbered from one in the list.
        Some(PaletteOutcome::Kept(PaletteLock::Shared(other))) => format!(
            ", palette kept because texture {} uses the same palette",
            other.0 + 1
        ),
        Some(PaletteOutcome::Kept(lock)) => format!(", palette kept because {lock}"),
        None => String::new(),
    };
    format!(
        "{} of {} blocks changed{palette}, {report}",
        edit.patch.changed_blocks, edit.patch.blocks
    )
}

/// Ask where to save the selected texture as a PNG.
pub fn choose_export(app: &mut App, tx: &mut Tx) {
    let Some(document) = &app.editor else { return };
    let Some(texture) = document.selected else {
        return;
    };
    let file_name = format!("{}.png", document.listing.name(texture));
    let options = FileDialogOptions {
        title: "Export the texture".into(),
        file_name: Some(file_name),
        ..Default::default()
    }
    .filter("PNG image", &["png"]);
    tx.save_file(options, move |app: &mut App, answer, _| match answer {
        Ok(Some(path)) => export_to(app, texture, &path),
        Ok(None) => {}
        Err(error) => app.tell(format!("The file dialog couldn't open: {error}"), true),
    });
}

/// `texture`, as the game decodes it, as a PNG.
pub fn texture_png(document: &Document, texture: usize) -> Result<Vec<u8>, String> {
    let open = document
        .listing
        .document
        .as_ref()
        .map_err(|error| error.to_string())?;
    let entry = open
        .textures()
        .get(texture)
        .ok_or("there's no such texture")?;
    let (width, height) = texture_size(entry);
    let rgba = open
        .pixels(TextureIndex(texture), UseIndex(0))
        .map_err(|error| error.to_string())?;
    let image =
        image::RgbaImage::from_raw(width, height, rgba).ok_or("its pixels don't match its size")?;
    let mut png = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(png)
}

/// Write `texture`, as the game decodes it, to `path` as a PNG.
pub fn export_to(app: &mut App, texture: usize, path: &Path) {
    let Some(document) = &app.editor else { return };
    let written = texture_png(document, texture)
        .and_then(|png| std::fs::write(path, png).map_err(|error| error.to_string()));
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    match written {
        Ok(()) => app.tell(format!("Exported {name}."), false),
        Err(error) => app.tell(format!("Couldn't export {name}: {error}"), true),
    }
}

/// `texture`'s (width, height), when a document is open.
fn size_of(app: &App, texture: usize) -> Option<(u32, u32)> {
    let open = app.editor.as_ref()?.listing.document.as_ref().ok()?;
    open.textures().get(texture).map(texture_size)
}
