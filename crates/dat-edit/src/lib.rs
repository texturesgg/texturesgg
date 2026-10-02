//! In-place edits to HSD DAT archives.
//!
//! `dat-parser` reads archives; this crate writes them. An edit overwrites only
//! the bytes it changes and never moves data, so the header, relocation and
//! root tables, and every other byte stay identical and the file reparses with
//! the same layout.

pub mod color;
pub mod document;
pub mod texture;

pub use color::{DocumentSurface, MaterialColor, MaterialColors, VertexColor};
pub use document::{
    AnimationFrame, DocumentError, DocumentTexture, PaletteLock, PaletteOutcome, Restored,
    TextureDocument, TextureEdit, TextureIndex, Undone, UseIndex, VertexColorId,
};
pub use gx_texture::TexelRect;
pub use texture::{TexturePatch, TexturePatchError, patch_texture};
