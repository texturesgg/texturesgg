//! An open DAT as the editor edits it: its textures, grouped by the pixel data
//! they decode, with every edit written into the file bytes in place.
//!
//! HAL archives often point several image descriptors at one block of pixel
//! data, and a CI image may be drawn through more than one palette. The
//! document lists each block of data once, with every descriptor pair (use)
//! the scene draws it with, so an edit re-decodes every use and the editor can
//! update each one.
//!
//! Texture animations add textures the scene never draws statically: the
//! later frames of a fighter's blinking eyes. The document lists those too,
//! each with the frame it is and the images it stands in for, so they can be
//! edited and previewed on the model.
//!
//! Every edit is kept as an undoable step: the texture's encoded bytes before
//! and after. Undo and redo copy bytes back rather than re-encoding, so they
//! are exact and cheap, and "modified" means the bytes differ from the last
//! save, so undoing back to it clears the flag.

use crate::color::{self, DocumentSurface, MaterialColor};
use crate::texture::{TexturePatch, TexturePatchError, patch_palette, patch_texture, writable};
use dat_parser::descriptor::tobj::{ImageDesc, TlutDesc};
use dat_parser::gx::display_list::encode_direct_color;
use dat_parser::gx::texture::TextureReadError;
use dat_parser::gx::texture::decode_texture;
use dat_parser::hsd::scene::{HsdScene, HsdSceneError, HsdTextureSourceId, TlutDescId};
use dat_parser::hsd::texture_animation::texture_animations;
use dat_parser::raw::header::DATA_SECTION_OFFSET;
use dat_parser::{DatFile, DatParseError};
use gx_texture::{PaletteFormat, TexelRect, TextureFormat, image_data_size};
use std::collections::{BTreeMap, HashSet, VecDeque};
use thiserror::Error;

/// A texture's position in [`TextureDocument::textures`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureIndex(pub usize);

/// A position in a texture's [`descriptors`](DocumentTexture::descriptors):
/// which image and palette pair reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UseIndex(pub usize);

/// One of a surface's vertex colors: the surface's position in
/// [`TextureDocument::surfaces`] and the color's in its `vertex_colors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VertexColorId {
    pub surface: usize,
    pub color: usize,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DocumentError {
    #[error("the file is not a readable DAT: {0}")]
    Parse(#[from] DatParseError),
    #[error("the DAT's scene could not be built: {0}")]
    Scene(#[from] HsdSceneError),
    #[error(transparent)]
    Patch(#[from] TexturePatchError),
    #[error("texture {} is not in the document", .0.0)]
    UnknownTexture(TextureIndex),
    #[error("texture {} has no use {}", .texture.0, .usage.0)]
    UnknownUse {
        texture: TextureIndex,
        usage: UseIndex,
    },
    #[error("texture {} does not decode: {source}", .texture.0)]
    Undecodable {
        texture: TextureIndex,
        source: TextureReadError,
    },
    #[error("the palette of texture {} does not read: {source}", .texture.0)]
    UnreadablePalette {
        texture: TextureIndex,
        source: dat_parser::descriptor::DescriptorParseError,
    },
    #[error(
        "texture {} shares its pixel data with texture {}, which reads it differently",
        .texture.0, .other.0
    )]
    SharedPixels {
        texture: TextureIndex,
        other: TextureIndex,
    },
    #[error(transparent)]
    Palette(#[from] gx_texture::PaletteError),
    #[error("surface {0} is not in the document")]
    UnknownSurface(usize),
    #[error("surface {} has no vertex color {}", .0.surface, .0.color)]
    UnknownColor(VertexColorId),
    #[error("surface {0} has no material")]
    NoMaterial(usize),
}

/// What an import did with a CI texture's palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// Rebuilt for the new pixels, with this many colors.
    Rebuilt { colors: usize },
    /// Kept, because rebuilding it could change something else; the pixels
    /// map to its existing colors.
    Kept(PaletteLock),
}

/// Why a texture's palette can't be rebuilt to fit new pixels; they then
/// map to its existing colors.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PaletteLock {
    #[error("it isn't a CI4 or CI8 texture")]
    NotPaletted,
    #[error("it's drawn through {0} different palettes")]
    SeveralPalettes(usize),
    #[error("another texture uses the same palette")]
    Shared(TextureIndex),
    #[error("its palette descriptor doesn't read")]
    Unreadable,
}

/// One block of image data and every descriptor pair that draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentTexture {
    /// Data-section offset of the pixel data.
    pub data_offset: u32,
    pub width: u16,
    pub height: u16,
    pub format: TextureFormat,
    /// Image and palette descriptors the scene draws this data with, in
    /// descriptor order. CI data drawn through several palettes has several.
    /// A frame only a texture animation shows has none; see
    /// [`descriptors`](Self::descriptors).
    pub uses: Vec<HsdTextureSourceId>,
    /// When a texture animation shows it, which frame it is.
    pub frame: Option<AnimationFrame>,
}

/// A texture's place in a texture animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationFrame {
    /// Its earliest frame index in any animation that shows it.
    pub frame: usize,
    /// That animation's frame count.
    pub frames: usize,
    /// The animations' own descriptor pairs for this frame, beyond the
    /// texture's `uses`: how the animations read it.
    pub sources: Vec<HsdTextureSourceId>,
    /// The images the scene draws where the animation shows this frame: the
    /// animated TObjs' own image and palette pairs, to preview it in place.
    /// Only images with this texture's size and format can be stood in for.
    pub replaces: Vec<HsdTextureSourceId>,
}

impl DocumentTexture {
    /// Every descriptor pair that reads this data: the scene's `uses`, then
    /// animation frames' own. Usage indices elsewhere index this list.
    pub fn descriptors(&self) -> Vec<HsdTextureSourceId> {
        let mut descriptors = self.uses.clone();
        if let Some(frame) = &self.frame {
            descriptors.extend(&frame.sources);
        }
        descriptors
    }

    /// What the scene draws where this texture shows: its uses, and for an
    /// animation frame the images it stands in for.
    pub fn drawn_as(&self) -> Vec<HsdTextureSourceId> {
        let mut drawn = self.uses.clone();
        if let Some(frame) = &self.frame {
            drawn.extend(&frame.replaces);
        }
        drawn
    }
}

/// What an edit changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureEdit {
    pub patch: TexturePatch,
    /// What happened to a CI texture's palette on an import; `None` for
    /// other edits and formats.
    pub palette: Option<PaletteOutcome>,
    /// Texels that decode differently from the pixels asked for: CMPR's
    /// lossy blocks, or colors a CI palette lacks. Counted over the edited
    /// region, widened for CMPR to the 8x8 tiles it re-encodes whole.
    pub lossy_texels: usize,
    /// The largest difference in any channel (0–255) between an edited texel
    /// and what was asked for: how far from exact the lossy texels are.
    pub max_channel_error: u8,
    /// Every descriptor's pixels after the edit (see
    /// [`DocumentTexture::descriptors`]), as the game decodes them.
    pub decoded: Vec<(HsdTextureSourceId, Vec<u8>)>,
}

/// What an undo or redo put back, for the caller to redraw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Undone {
    Texture(Restored),
    /// Vertex or material colors; [`TextureDocument::surfaces`] has them.
    Colors,
}

/// A texture's pixels after an undo or redo, for the caller to redraw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restored {
    pub texture: TextureIndex,
    /// Every descriptor's pixels, as the game decodes them.
    pub decoded: Vec<(HsdTextureSourceId, Vec<u8>)>,
}

/// How much encoded data the undo history keeps before dropping its oldest
/// steps. A 256x256 CMPR step keeps 64 KiB (before and after).
const HISTORY_BYTES: usize = 64 << 20;

/// One undoable edit: the data bytes it changed, before and after. A
/// texture's pixels, and its palette when the edit rebuilt it; or colors.
struct Step {
    /// Identifies the document state this step produces.
    id: u64,
    /// The texture edited; `None` for an edit to colors.
    texture: Option<usize>,
    changes: Vec<Change>,
}

struct Change {
    /// Data-section offset of `before` and `after`.
    data_offset: usize,
    before: Vec<u8>,
    after: Vec<u8>,
}

impl Step {
    fn size(&self) -> usize {
        self.changes
            .iter()
            .map(|change| change.before.len() + change.after.len())
            .sum()
    }
}

struct History {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
    /// The state below the oldest undo step: the opened file, or the state
    /// the oldest dropped step produced.
    base: u64,
    /// The state last saved (or opened), if the history can still reach it.
    saved: u64,
    next_id: u64,
    bytes: usize,
    /// `HISTORY_BYTES`, smaller in tests.
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            base: 0,
            saved: 0,
            next_id: 0,
            bytes: 0,
            limit: HISTORY_BYTES,
        }
    }
}

impl History {
    /// The state the document is in now.
    fn current(&self) -> u64 {
        self.undo.back().map_or(self.base, |step| step.id)
    }

    fn push(&mut self, mut step: Step) {
        self.next_id += 1;
        step.id = self.next_id;
        self.bytes -= self.redo.drain(..).map(|step| step.size()).sum::<usize>();
        self.bytes += step.size();
        self.undo.push_back(step);
        while self.bytes > self.limit && self.undo.len() > 1 {
            let dropped = self.undo.pop_front().expect("more than one step");
            self.bytes -= dropped.size();
            self.base = dropped.id;
        }
    }
}

pub struct TextureDocument {
    file: Vec<u8>,
    /// `file` parsed; its data section is kept in step with every edit.
    dat: DatFile,
    textures: Vec<DocumentTexture>,
    surfaces: Vec<DocumentSurface>,
    history: History,
}

impl TextureDocument {
    /// Parse a DAT and list the textures its scene draws. A texture in a
    /// format the codec does not handle is not listed.
    pub fn open(file: Vec<u8>) -> Result<Self, DocumentError> {
        Self::open_inspecting(file, |_, _| ()).map(|(document, ())| document)
    }

    /// Open, also handing the parsed file and scene to `inspect`, to derive
    /// something else from the same parse (the editor places a fighter's
    /// textures on its body this way).
    pub fn open_inspecting<T>(
        file: Vec<u8>,
        inspect: impl FnOnce(&DatFile, &HsdScene) -> T,
    ) -> Result<(Self, T), DocumentError> {
        let dat = DatFile::parse(&file)?;
        let scene = HsdScene::from_dat(&dat)?;
        let drawn = scene.textures.iter().filter_map(|texture| {
            let image = &texture.image;
            let data_offset = image.data_offset?;
            let format = TextureFormat::try_from(image.format).ok()?;
            Some((texture.id, data_offset, image.width, image.height, format))
        });
        let animations = texture_animations(&dat, &scene);
        let drawn_ids: HashSet<HsdTextureSourceId> =
            scene.textures.iter().map(|texture| texture.id).collect();
        // Frames the scene never draws, read from their own image descriptors.
        let frames: Vec<_> = animations
            .iter()
            .flat_map(|animation| &animation.frames)
            .filter(|id| !drawn_ids.contains(id))
            .filter_map(|&id| {
                let image = ImageDesc::parse(&dat, id.image.0).ok()?;
                let format = TextureFormat::try_from(image.format).ok()?;
                Some((id, image.data_ptr?, image.width, image.height, format))
            })
            .collect();
        let (mut textures, undrawn) = group(drawn, frames);
        // Where each drawn pair's texture is, to size-check stand-ins.
        let holder = |id: &HsdTextureSourceId, textures: &[DocumentTexture]| {
            textures
                .iter()
                .find(|texture| texture.uses.contains(id))
                .map(|texture| (texture.width, texture.height, texture.format))
        };
        let shapes: Vec<_> = animations
            .iter()
            .map(|animation| animation.base.and_then(|base| holder(&base, &textures)))
            .collect();
        for (texture, undrawn) in textures.iter_mut().zip(&undrawn) {
            let shape = (texture.width, texture.height, texture.format);
            for (animation, base_shape) in animations.iter().zip(&shapes) {
                let reads =
                    |id: &HsdTextureSourceId| texture.uses.contains(id) || undrawn.contains(id);
                let Some(frame) = animation.frames.iter().position(reads) else {
                    continue;
                };
                let entry = texture.frame.get_or_insert_with(|| AnimationFrame {
                    frame,
                    frames: animation.frames.len(),
                    sources: Vec::new(),
                    replaces: Vec::new(),
                });
                if frame < entry.frame {
                    entry.frame = frame;
                    entry.frames = animation.frames.len();
                }
                for id in animation.frames.iter().filter(|id| undrawn.contains(id)) {
                    if !entry.sources.contains(id) {
                        entry.sources.push(*id);
                    }
                }
                if let Some(base) = animation.base
                    && *base_shape == Some(shape)
                    && !entry.replaces.contains(&base)
                {
                    entry.replaces.push(base);
                }
            }
        }
        let inspected = inspect(&dat, &scene);
        let surfaces = color::surfaces(&dat, &scene, &textures);
        Ok((Self::new(file, dat, textures, surfaces), inspected))
    }

    fn new(
        file: Vec<u8>,
        dat: DatFile,
        textures: Vec<DocumentTexture>,
        surfaces: Vec<DocumentSurface>,
    ) -> Self {
        Self {
            file,
            dat,
            textures,
            surfaces,
            history: History::default(),
        }
    }

    /// A document of surfaces only, for tests.
    #[cfg(test)]
    pub(crate) fn with_surfaces(file: Vec<u8>, surfaces: Vec<DocumentSurface>) -> Self {
        let dat = DatFile::parse(&file).expect("test DAT parses");
        Self::new(file, dat, Vec::new(), surfaces)
    }

    /// A document of drawn texture uses only, for tests.
    #[cfg(test)]
    fn from_parts(file: Vec<u8>, dat: DatFile, uses: impl IntoIterator<Item = Use>) -> Self {
        let (textures, _) = group(uses, []);
        Self::new(file, dat, textures, Vec::new())
    }

    pub fn textures(&self) -> &[DocumentTexture] {
        &self.textures
    }

    /// Every display object and what colors it besides its textures, in
    /// scene order.
    pub fn surfaces(&self) -> &[DocumentSurface] {
        &self.surfaces
    }

    /// Replace vertex colors everywhere they are written, as one edit. The color is stored in the vertices' own
    /// format, which may keep fewer bits than `rgba`; the surfaces then list
    /// what the game reads back.
    pub fn recolor_vertices(
        &mut self,
        colors: &[VertexColorId],
        rgba: [u8; 4],
    ) -> Result<(), DocumentError> {
        let mut writes = Vec::new();
        for &id in colors {
            let entry = self
                .surface(id.surface)?
                .vertex_colors
                .get(id.color)
                .ok_or(DocumentError::UnknownColor(id))?;
            let bytes = encode_direct_color(entry.format, rgba);
            writes.extend(entry.sites.iter().map(|&site| (site, bytes.clone())));
        }
        self.write_colors(writes)
    }

    /// Pass every vertex color of `surfaces` through `map`, as one edit: a
    /// tint or hue shift for surfaces whose shading is baked into many
    /// colors.
    pub fn map_vertex_colors(
        &mut self,
        surfaces: &[usize],
        map: impl Fn([u8; 4]) -> [u8; 4],
    ) -> Result<(), DocumentError> {
        let mut writes = Vec::new();
        for &surface in surfaces {
            for color in &self.surface(surface)?.vertex_colors {
                let bytes = encode_direct_color(color.format, map(color.rgba));
                writes.extend(color.sites.iter().map(|&site| (site, bytes.clone())));
            }
        }
        self.write_colors(writes)
    }

    /// Set one of the material colors of `surfaces`, as one edit. Display
    /// objects that share a material change with it.
    pub fn set_material_color(
        &mut self,
        surfaces: &[usize],
        color: MaterialColor,
        rgba: [u8; 4],
    ) -> Result<(), DocumentError> {
        let mut writes = Vec::new();
        for &surface in surfaces {
            let material = self
                .surface(surface)?
                .material
                .ok_or(DocumentError::NoMaterial(surface))?;
            writes.push((material.offset + color.offset(), rgba.to_vec()));
        }
        self.write_colors(writes)
    }

    fn surface(&self, surface: usize) -> Result<&DocumentSurface, DocumentError> {
        self.surfaces
            .get(surface)
            .ok_or(DocumentError::UnknownSurface(surface))
    }

    /// Write colors as one undo step, or nothing if any write is refused.
    fn write_colors(&mut self, writes: Vec<(u32, Vec<u8>)>) -> Result<(), DocumentError> {
        let mut changes = Vec::new();
        for (site, bytes) in &writes {
            if let Err(error) = self.write(&mut changes, *site as usize, bytes) {
                self.roll_back(&changes);
                return Err(error);
            }
        }
        self.commit(None, changes);
        color::refresh(&self.dat, &mut self.surfaces);
        Ok(())
    }

    /// The file as it stands, with every edit applied.
    pub fn bytes(&self) -> &[u8] {
        &self.file
    }

    /// Whether edits stand between the file and the state it was opened or
    /// last marked saved in. Undoing back to that state clears it; painting
    /// the old pixels back as a new edit does not.
    pub fn is_modified(&self) -> bool {
        self.history.current() != self.history.saved
    }

    /// Record that `bytes()` has been written out.
    pub fn mark_saved(&mut self) {
        self.history.saved = self.history.current();
    }

    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.history.redo.is_empty()
    }

    /// Undo the latest edit, returning what it put back.
    pub fn undo(&mut self) -> Option<Result<Undone, DocumentError>> {
        let step = self.history.undo.pop_back()?;
        self.roll_back(&step.changes);
        let texture = step.texture;
        self.history.redo.push(step);
        Some(self.restored(texture))
    }

    /// Redo the latest undone edit, returning what it put back.
    pub fn redo(&mut self) -> Option<Result<Undone, DocumentError>> {
        let step = self.history.redo.pop()?;
        for change in &step.changes {
            self.copy_in(change.data_offset, &change.after);
        }
        let texture = step.texture;
        self.history.undo.push_back(step);
        Some(self.restored(texture))
    }

    fn restored(&mut self, texture: Option<usize>) -> Result<Undone, DocumentError> {
        match texture {
            Some(texture) => Ok(Undone::Texture(Restored {
                texture: TextureIndex(texture),
                decoded: self.decode_uses(texture)?,
            })),
            None => {
                color::refresh(&self.dat, &mut self.surfaces);
                Ok(Undone::Colors)
            }
        }
    }

    /// Write `bytes` at `data_offset` in both the file and its parse, and
    /// record what was there in `changes`, so an edit is always undoable. An
    /// unchanged write records nothing; a range outside the data section, or
    /// over a relocated pointer, is refused before anything is written.
    fn write(
        &mut self,
        changes: &mut Vec<Change>,
        data_offset: usize,
        bytes: &[u8],
    ) -> Result<(), DocumentError> {
        let site = u32::try_from(data_offset).map_err(|_| TexturePatchError::OutOfBounds {
            data_offset: u32::MAX,
        })?;
        let before = writable(&self.dat, site, bytes.len())?.to_vec();
        if before == bytes {
            return Ok(());
        }
        self.copy_in(data_offset, bytes);
        changes.push(Change {
            data_offset,
            before,
            after: bytes.to_vec(),
        });
        Ok(())
    }

    /// Put back what `changes` overwrote, newest first: an edit that failed
    /// partway.
    fn roll_back(&mut self, changes: &[Change]) {
        for change in changes.iter().rev() {
            self.copy_in(change.data_offset, &change.before);
        }
    }

    /// Record an edit's changes as one undo step.
    fn commit(&mut self, texture: Option<usize>, changes: Vec<Change>) {
        if !changes.is_empty() {
            self.history.push(Step {
                id: 0,
                texture,
                changes,
            });
        }
    }

    /// Copy bytes into the file and its parse, which always hold the same
    /// data section.
    fn copy_in(&mut self, data_offset: usize, bytes: &[u8]) {
        let range = data_offset..data_offset + bytes.len();
        self.dat.data[range.clone()].copy_from_slice(bytes);
        self.file[DATA_SECTION_OFFSET + range.start..DATA_SECTION_OFFSET + range.end]
            .copy_from_slice(bytes);
    }

    /// A texture's pixels through one of its
    /// [`descriptors`](DocumentTexture::descriptors), as the game decodes
    /// them.
    pub fn pixels(&self, texture: TextureIndex, usage: UseIndex) -> Result<Vec<u8>, DocumentError> {
        let id = self.use_of(texture, usage)?;
        self.decode(texture.0, id)
    }

    /// The descriptor pair `usage` names among `texture`'s.
    fn use_of(
        &self,
        texture: TextureIndex,
        usage: UseIndex,
    ) -> Result<HsdTextureSourceId, DocumentError> {
        self.texture(texture.0)?
            .descriptors()
            .get(usage.0)
            .copied()
            .ok_or(DocumentError::UnknownUse { texture, usage })
    }

    /// Replace a texture's pixels with row-major RGBA8 `rgba`, mapping colors
    /// through use `usage`'s palette when the format is CI4 or CI8. With
    /// `dirty`, only the blocks it overlaps are re-encoded.
    pub fn apply(
        &mut self,
        texture: TextureIndex,
        usage: UseIndex,
        rgba: &[u8],
        dirty: Option<TexelRect>,
    ) -> Result<TextureEdit, DocumentError> {
        let id = self.use_of(texture, usage)?;
        let (texture, usage) = (texture.0, usage.0);
        self.owns_its_pixels(texture)?;
        let patch = patch_texture(&self.dat, id.image, id.palette, rgba, dirty)?;
        let mut changes = Vec::new();
        self.write(&mut changes, patch.data_offset as usize, &patch.bytes)?;
        self.commit(Some(texture), changes);
        self.edit_report(texture, usage, rgba, dirty, patch)
    }

    /// Refuse to edit a texture whose pixel data another texture reads with a
    /// different size or format, or from another offset: writing one would
    /// change the other with nothing to say so.
    fn owns_its_pixels(&self, texture: usize) -> Result<(), DocumentError> {
        let extent = |entry: &DocumentTexture| {
            let start = u64::from(entry.data_offset);
            image_data_size(entry.width, entry.height, entry.format)
                .map(|len| start..start + len as u64)
        };
        let Ok(own) = extent(self.texture(texture)?) else {
            return Ok(());
        };
        let shared = self.textures.iter().enumerate().find(|&(other, entry)| {
            other != texture
                && extent(entry).is_ok_and(|range| own.start < range.end && range.start < own.end)
        });
        match shared {
            Some((other, _)) => Err(DocumentError::SharedPixels {
                texture: TextureIndex(texture),
                other: TextureIndex(other),
            }),
            None => Ok(()),
        }
    }

    /// Whether [`import`](Self::import) can rebuild `texture`'s palette: it must be CI4 or CI8, drawn through
    /// one palette, and no other texture may use that palette's colors.
    /// Returns the palette descriptor.
    pub fn rebuildable_palette(&self, texture: TextureIndex) -> Result<TlutDescId, PaletteLock> {
        let TextureIndex(texture) = texture;
        let entry = self.textures.get(texture).ok_or(PaletteLock::Unreadable)?;
        if entry.format.palette_entries().is_none() {
            return Err(PaletteLock::NotPaletted);
        }
        // Palettes are identified by the colors they point at: descriptors
        // can share them.
        let colors_of = |descriptor: TlutDescId| {
            TlutDesc::parse(&self.dat, descriptor.0)
                .ok()
                .and_then(|tlut| tlut.data_ptr)
        };
        let mut palettes: Vec<(TlutDescId, u32)> = Vec::new();
        for id in &entry.descriptors() {
            let descriptor = id.palette.ok_or(PaletteLock::NotPaletted)?;
            let colors = colors_of(descriptor).ok_or(PaletteLock::Unreadable)?;
            if !palettes.iter().any(|&(_, seen)| seen == colors) {
                palettes.push((descriptor, colors));
            }
        }
        let [(descriptor, colors)] = palettes[..] else {
            return Err(PaletteLock::SeveralPalettes(palettes.len()));
        };
        for (other, texture_entry) in self.textures.iter().enumerate() {
            if other == texture {
                continue;
            }
            let shares = texture_entry.descriptors().iter().any(|id| {
                id.palette
                    .and_then(colors_of)
                    .is_some_and(|other_colors| other_colors == colors)
            });
            if shares {
                return Err(PaletteLock::Shared(TextureIndex(other)));
            }
        }
        Ok(descriptor)
    }

    /// Replace a texture's pixels with a whole new image, as an import
    /// does. A CI texture that owns its palette (see
    /// [`rebuildable_palette`](Self::rebuildable_palette)) gets a palette
    /// rebuilt for the image (see [`gx_texture::build_palette`]), so an image
    /// with no more colors than the palette holds encodes exactly; the
    /// palette and pixels are one undo step. Any other CI texture keeps its
    /// palette, and the result says why.
    pub fn import(
        &mut self,
        texture: TextureIndex,
        rgba: &[u8],
    ) -> Result<TextureEdit, DocumentError> {
        if self.texture(texture.0)?.format.palette_entries().is_none() {
            return self.apply(texture, UseIndex(0), rgba, None);
        }
        match self.rebuildable_palette(texture) {
            Ok(descriptor) => self.rebuild_palette(texture.0, descriptor, rgba),
            Err(lock) => {
                let mut edit = self.apply(texture, UseIndex(0), rgba, None)?;
                edit.palette = Some(PaletteOutcome::Kept(lock));
                Ok(edit)
            }
        }
    }

    fn rebuild_palette(
        &mut self,
        texture: usize,
        descriptor: TlutDescId,
        rgba: &[u8],
    ) -> Result<TextureEdit, DocumentError> {
        let entry = self.texture(texture)?.clone();
        self.owns_its_pixels(texture)?;
        let tlut = TlutDesc::parse(&self.dat, descriptor.0).map_err(TexturePatchError::from)?;
        let reachable = entry.format.palette_entries().unwrap_or(0);
        let count = usize::from(tlut.color_count).min(reachable);
        let format = PaletteFormat::try_from(tlut.format).map_err(TexturePatchError::from)?;
        let (colors, entries) = gx_texture::build_palette(rgba, format, count)?;
        // Write the palette, then encode the pixels through it.
        let palette_offset = patch_palette(&self.dat, descriptor, &entries)?;
        let mut changes = Vec::new();
        self.write(&mut changes, palette_offset as usize, &entries)?;
        let written = patch_texture(
            &self.dat,
            entry.descriptors()[0].image,
            Some(descriptor),
            rgba,
            None,
        )
        .map_err(DocumentError::from)
        .and_then(|patch| {
            self.write(&mut changes, patch.data_offset as usize, &patch.bytes)?;
            Ok(patch)
        });
        let patch = match written {
            Ok(patch) => patch,
            Err(error) => {
                // The edit didn't happen: put the palette back.
                self.roll_back(&changes);
                return Err(error);
            }
        };
        self.commit(Some(texture), changes);
        let mut edit = self.edit_report(texture, 0, rgba, None, patch)?;
        edit.palette = Some(PaletteOutcome::Rebuilt {
            colors: colors.len(),
        });
        Ok(edit)
    }

    /// Decode an edited texture and measure how far it lands from `rgba`.
    fn edit_report(
        &self,
        texture: usize,
        usage: usize,
        rgba: &[u8],
        dirty: Option<TexelRect>,
        patch: TexturePatch,
    ) -> Result<TextureEdit, DocumentError> {
        let entry = self.texture(texture)?;
        let decoded = self.decode_uses(texture)?;
        let edited = &decoded[usage].1;
        let (mut lossy_texels, mut max_channel_error) = (0, 0u8);
        // CMPR re-encodes every tile the region touches, so texels beside
        // the painted ones can move too.
        let measured = match dirty {
            Some(rect) if entry.format == TextureFormat::Cmpr => {
                let (x, y) = (rect.x & !7, rect.y & !7);
                let end = |start: u16, length: u16| {
                    (u32::from(start) + u32::from(length)).next_multiple_of(8)
                };
                let span =
                    |from: u16, to: u32| u16::try_from(to - u32::from(from)).unwrap_or(u16::MAX);
                Some(TexelRect {
                    x,
                    y,
                    width: span(x, end(rect.x, rect.width)),
                    height: span(y, end(rect.y, rect.height)),
                })
            }
            other => other,
        };
        for index in texels_in(measured, entry.width, entry.height) {
            let texel = index * 4..index * 4 + 4;
            let error = edited[texel.clone()]
                .iter()
                .zip(&rgba[texel])
                .map(|(&got, &asked)| got.abs_diff(asked))
                .max()
                .unwrap_or(0);
            if error > 0 {
                lossy_texels += 1;
                max_channel_error = max_channel_error.max(error);
            }
        }
        Ok(TextureEdit {
            patch,
            palette: None,
            lossy_texels,
            max_channel_error,
            decoded,
        })
    }

    /// Every descriptor's pixels. They differ only by palette, so this decodes once
    /// per distinct palette.
    fn decode_uses(
        &self,
        texture: usize,
    ) -> Result<Vec<(HsdTextureSourceId, Vec<u8>)>, DocumentError> {
        let descriptors = self.texture(texture)?.descriptors();
        let mut by_palette: BTreeMap<Option<u32>, Vec<u8>> = BTreeMap::new();
        let mut decoded = Vec::with_capacity(descriptors.len());
        for &use_id in &descriptors {
            let palette = use_id.palette.map(|palette| palette.0);
            let pixels = match by_palette.get(&palette) {
                Some(pixels) => pixels.clone(),
                None => {
                    let pixels = self.decode(texture, use_id)?;
                    by_palette.insert(palette, pixels.clone());
                    pixels
                }
            };
            decoded.push((use_id, pixels));
        }
        Ok(decoded)
    }

    fn texture(&self, texture: usize) -> Result<&DocumentTexture, DocumentError> {
        self.textures
            .get(texture)
            .ok_or(DocumentError::UnknownTexture(TextureIndex(texture)))
    }

    fn decode(&self, texture: usize, id: HsdTextureSourceId) -> Result<Vec<u8>, DocumentError> {
        let entry = &self.textures[texture];
        let tlut = match id.palette {
            Some(palette) => Some(TlutDesc::parse(&self.dat, palette.0).map_err(|source| {
                DocumentError::UnreadablePalette {
                    texture: TextureIndex(texture),
                    source,
                }
            })?),
            None => None,
        };
        decode_texture(
            &self.dat,
            entry.data_offset,
            entry.width,
            entry.height,
            entry.format,
            tlut.as_ref(),
        )
        .map_err(|source| DocumentError::Undecodable {
            texture: TextureIndex(texture),
            source,
        })
    }
}

/// A descriptor pair and the image data it reads: data offset, width,
/// height, format.
type Use = (HsdTextureSourceId, u32, u16, u16, TextureFormat);

/// Group descriptor pairs by the image data they read: one texture per block
/// of data, with the pairs the scene draws as its `uses`. Returns, per
/// texture, the undrawn pairs (animation frames) that read it too.
fn group(
    drawn: impl IntoIterator<Item = Use>,
    undrawn: impl IntoIterator<Item = Use>,
) -> (Vec<DocumentTexture>, Vec<Vec<HsdTextureSourceId>>) {
    type Pairs = (Vec<HsdTextureSourceId>, Vec<HsdTextureSourceId>);
    // Ordered by data offset, then the format's GX value.
    let mut grouped: BTreeMap<(u32, u32, u16, u16), (TextureFormat, Pairs)> = BTreeMap::new();
    let tagged = drawn
        .into_iter()
        .map(|entry| (entry, true))
        .chain(undrawn.into_iter().map(|entry| (entry, false)));
    for ((id, data_offset, width, height, format), is_drawn) in tagged {
        let (_, pairs) = grouped
            .entry((data_offset, u32::from(format), width, height))
            .or_insert_with(|| (format, Pairs::default()));
        if is_drawn {
            pairs.0.push(id);
        } else {
            pairs.1.push(id);
        }
    }
    let order = |id: &HsdTextureSourceId| (id.image.0, id.palette.map(|palette| palette.0));
    grouped
        .into_iter()
        .map(
            |((data_offset, _, width, height), (format, (mut uses, mut undrawn)))| {
                uses.sort_by_key(order);
                uses.dedup();
                undrawn.sort_by_key(order);
                undrawn.dedup();
                undrawn.retain(|id| !uses.contains(id));
                let texture = DocumentTexture {
                    data_offset,
                    width,
                    height,
                    format,
                    uses,
                    frame: None,
                };
                (texture, undrawn)
            },
        )
        .unzip()
}

/// Row-major texel indices inside `region`, or the whole image.
fn texels_in(region: Option<TexelRect>, width: u16, height: u16) -> impl Iterator<Item = usize> {
    let (width, height) = (usize::from(width), usize::from(height));
    let (x0, y0, x1, y1) = match region {
        Some(rect) => (
            usize::from(rect.x).min(width),
            usize::from(rect.y).min(height),
            (usize::from(rect.x) + usize::from(rect.width)).min(width),
            (usize::from(rect.y) + usize::from(rect.height)).min(height),
        ),
        None => (0, 0, width, height),
    };
    (y0..y1).flat_map(move |y| (x0..x1).map(move |x| y * width + x))
}

#[cfg(test)]
mod tests {
    use super::{
        DocumentError, PaletteLock, PaletteOutcome, TextureDocument, TextureIndex, UseIndex,
    };
    use crate::Undone;
    use crate::texture::TexturePatchError;
    use dat_parser::DatFile;
    use dat_parser::hsd::scene::{HsdTextureSourceId, ImageDescId, TlutDescId};
    use dat_parser::raw::header::DATA_SECTION_OFFSET;
    use gx_texture::TexelRect;
    use gx_texture::TextureFormat::{Ci8, Cmpr};

    const TLUT: u32 = 0x18;
    const COLORS: u32 = 0x28;
    const PIXELS: u32 = 0x40;
    const SECOND_IMAGE: u32 = 0x80;
    const SECOND_TLUT: u32 = 0x98;
    const SECOND_COLORS: u32 = 0xA8;

    /// An 8x8 CI8 image drawn through two image descriptors, each with its own
    /// four-color RGB565 palette: red/green/blue/white, and the reverse.
    fn archive() -> Vec<u8> {
        archive_with(&[])
    }

    /// [`archive`] with more relocated pointer sites (words that hold 0).
    fn archive_with(extra_sites: &[u32]) -> Vec<u8> {
        let mut data = vec![0u8; 0xB0];
        let image = |data: &mut [u8], at: usize| {
            data[at..at + 4].copy_from_slice(&PIXELS.to_be_bytes());
            data[at + 4..at + 6].copy_from_slice(&8u16.to_be_bytes());
            data[at + 6..at + 8].copy_from_slice(&8u16.to_be_bytes());
            data[at + 8..at + 12].copy_from_slice(&9u32.to_be_bytes());
        };
        let tlut = |data: &mut [u8], at: usize, colors: u32| {
            data[at..at + 4].copy_from_slice(&colors.to_be_bytes());
            data[at + 4..at + 8].copy_from_slice(&1u32.to_be_bytes());
            data[at + 0x0c..at + 0x0e].copy_from_slice(&4u16.to_be_bytes());
        };
        image(&mut data, 0);
        tlut(&mut data, TLUT as usize, COLORS);
        image(&mut data, SECOND_IMAGE as usize);
        tlut(&mut data, SECOND_TLUT as usize, SECOND_COLORS);
        let palette = [0xF800u16, 0x07E0, 0x001F, 0xFFFF];
        for (index, color) in palette.iter().enumerate() {
            let at = COLORS as usize + index * 2;
            data[at..at + 2].copy_from_slice(&color.to_be_bytes());
            let at = SECOND_COLORS as usize + index * 2;
            data[at..at + 2].copy_from_slice(&palette[3 - index].to_be_bytes());
        }
        for texel in 0..64 {
            data[PIXELS as usize + texel] = (texel % 4) as u8;
        }
        for &site in extra_sites {
            data[site as usize..site as usize + 4].fill(0);
        }
        let mut sites = vec![0, TLUT, SECOND_IMAGE, SECOND_TLUT];
        sites.extend(extra_sites);
        sites.sort_unstable();

        let mut file = vec![0; DATA_SECTION_OFFSET];
        let file_size = DATA_SECTION_OFFSET + data.len() + sites.len() * 4;
        file[0..4].copy_from_slice(&(file_size as u32).to_be_bytes());
        file[4..8].copy_from_slice(&(data.len() as u32).to_be_bytes());
        file[8..12].copy_from_slice(&(sites.len() as u32).to_be_bytes());
        file.extend_from_slice(&data);
        file.extend(sites.iter().flat_map(|site| site.to_be_bytes()));
        file
    }

    fn document() -> TextureDocument {
        let file = archive();
        let dat = DatFile::parse(&file).unwrap();
        let first = HsdTextureSourceId {
            image: ImageDescId(0),
            palette: Some(TlutDescId(TLUT)),
        };
        let second = HsdTextureSourceId {
            image: ImageDescId(SECOND_IMAGE),
            palette: Some(TlutDescId(SECOND_TLUT)),
        };
        TextureDocument::from_parts(
            file,
            dat,
            [(second, PIXELS, 8, 8, Ci8), (first, PIXELS, 8, 8, Ci8)],
        )
    }

    #[test]
    fn shared_image_data_is_one_texture_with_every_use() {
        let document = document();
        let [texture] = document.textures() else {
            panic!("one texture expected");
        };
        assert_eq!(texture.data_offset, PIXELS);
        assert_eq!(texture.uses.len(), 2);
        assert_eq!(texture.uses[0].image, ImageDescId(0));
        assert!(!document.is_modified());
        assert_eq!(
            &document.pixels(TextureIndex(0), UseIndex(0)).unwrap()[..4],
            &[255, 0, 0, 255]
        );
        assert_eq!(
            &document.pixels(TextureIndex(0), UseIndex(1)).unwrap()[..4],
            &[255, 255, 255, 255]
        );
    }

    #[test]
    fn an_edit_patches_the_bytes_and_redecodes_every_use() {
        let mut document = document();
        let original = document.bytes().to_vec();
        let mut rgba = document.pixels(TextureIndex(0), UseIndex(0)).unwrap();
        // Texel 0 becomes near-blue: index 2 through the first palette.
        rgba[..4].copy_from_slice(&[10, 5, 240, 255]);
        let dirty = TexelRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let edit = document
            .apply(TextureIndex(0), UseIndex(0), &rgba, Some(dirty))
            .unwrap();

        assert_eq!(edit.patch.changed_blocks, 1);
        // Near-blue isn't in the palette, so the one edited texel is lossy:
        // it decodes as pure blue, 15 away in blue.
        assert_eq!(edit.lossy_texels, 1);
        assert_eq!(edit.max_channel_error, 15);
        assert!(document.is_modified());
        let changed: Vec<usize> = (0..original.len())
            .filter(|&index| document.bytes()[index] != original[index])
            .collect();
        assert_eq!(changed, [DATA_SECTION_OFFSET + PIXELS as usize]);

        // Index 2 is blue through the first palette and green through the
        // reversed second one, and the document reads back the same.
        assert_eq!(&edit.decoded[0].1[..4], &[0, 0, 255, 255]);
        assert_eq!(&edit.decoded[1].1[..4], &[0, 255, 0, 255]);
        assert_eq!(
            document.pixels(TextureIndex(0), UseIndex(1)).unwrap(),
            edit.decoded[1].1
        );
        let reparsed = TextureDocument::from_parts(
            document.bytes().to_vec(),
            DatFile::parse(document.bytes()).unwrap(),
            document.textures()[0]
                .uses
                .iter()
                .map(|&id| (id, PIXELS, 8, 8, Ci8)),
        );
        assert_eq!(
            reparsed.pixels(TextureIndex(0), UseIndex(0)).unwrap(),
            edit.decoded[0].1
        );
    }

    /// Set texel 0 to palette color `index` through the first use.
    fn paint(document: &mut TextureDocument, index: usize) {
        let colors = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
        let mut rgba = document.pixels(TextureIndex(0), UseIndex(0)).unwrap();
        rgba[..4].copy_from_slice(&colors[index]);
        document
            .apply(TextureIndex(0), UseIndex(0), &rgba, None)
            .unwrap();
    }

    #[test]
    fn undo_and_redo_restore_the_exact_bytes() {
        let mut document = document();
        let original = document.bytes().to_vec();
        assert!(!document.can_undo() && document.undo().is_none());

        paint(&mut document, 2);
        let edited = document.bytes().to_vec();
        let Undone::Texture(restored) = document.undo().unwrap().unwrap() else {
            panic!("a texture edit was undone");
        };
        assert_eq!(document.bytes(), original);
        assert_eq!(restored.texture, TextureIndex(0));
        assert_eq!(&restored.decoded[0].1[..4], &[255, 0, 0, 255]);
        assert_eq!(
            restored.decoded[1].1,
            document.pixels(TextureIndex(0), UseIndex(1)).unwrap()
        );
        assert!(!document.is_modified());

        document.redo().unwrap().unwrap();
        assert_eq!(document.bytes(), edited);
        assert_eq!(
            &document.pixels(TextureIndex(0), UseIndex(0)).unwrap()[..4],
            &[0, 0, 255, 255]
        );
        assert!(document.is_modified() && !document.can_redo());
    }

    #[test]
    fn modified_tracks_the_saved_state() {
        let mut document = document();
        paint(&mut document, 1);
        document.mark_saved();
        assert!(!document.is_modified());
        document.undo();
        assert!(document.is_modified());
        document.redo();
        assert!(!document.is_modified());

        // A new edit after an undo discards the redo branch, and with it the
        // saved state.
        document.undo();
        paint(&mut document, 2);
        assert!(!document.can_redo() && document.is_modified());
        document.undo();
        assert!(document.is_modified());
    }

    #[test]
    fn the_oldest_steps_drop_past_the_limit() {
        let mut document = document();
        // Each step keeps the 64-byte image twice.
        document.history.limit = 128 * 2;
        for index in [1, 2, 1] {
            paint(&mut document, index);
        }
        assert_eq!(document.history.undo.len(), 2);
        document.undo();
        document.undo();
        assert!(document.undo().is_none());
        // The opened state is out of reach, so the file stays modified.
        assert_eq!(
            &document.pixels(TextureIndex(0), UseIndex(0)).unwrap()[..4],
            &[0, 255, 0, 255]
        );
        assert!(document.is_modified());
    }

    /// The archive's image drawn through its first palette only.
    fn single_palette_document() -> TextureDocument {
        single_palette_document_of(archive())
    }

    fn single_palette_document_of(file: Vec<u8>) -> TextureDocument {
        let dat = DatFile::parse(&file).unwrap();
        let first = HsdTextureSourceId {
            image: ImageDescId(0),
            palette: Some(TlutDescId(TLUT)),
        };
        TextureDocument::from_parts(file, dat, [(first, PIXELS, 8, 8, Ci8)])
    }

    #[test]
    fn a_rebuilt_palette_fits_new_colors_exactly_and_undoes_together() {
        let mut document = single_palette_document();
        let original = document.bytes().to_vec();
        assert_eq!(
            document.rebuildable_palette(TextureIndex(0)),
            Ok(TlutDescId(TLUT))
        );
        // Two RGB565-exact colors the palette doesn't have.
        let (violet, yellow) = ([8, 4, 255, 255], [255, 255, 0, 255]);
        let rgba: Vec<u8> = (0..64)
            .flat_map(|texel| if texel % 3 == 0 { violet } else { yellow })
            .collect();
        let kept = {
            let mut copy = single_palette_document();
            copy.apply(TextureIndex(0), UseIndex(0), &rgba, None)
                .unwrap()
        };
        assert!(kept.lossy_texels > 0, "the old palette lacks both colors");

        let edit = document.import(TextureIndex(0), &rgba).unwrap();
        assert_eq!(edit.palette, Some(PaletteOutcome::Rebuilt { colors: 2 }));
        assert_eq!(edit.lossy_texels, 0);
        assert_eq!(document.pixels(TextureIndex(0), UseIndex(0)).unwrap(), rgba);
        // The palette and the pixels are one step.
        document.undo().unwrap().unwrap();
        assert_eq!(document.bytes(), original);
        assert!(!document.can_undo());
    }

    #[test]
    fn a_rebuild_that_fails_partway_puts_the_palette_back() {
        // A relocated pointer inside the texels: the palette is written,
        // then the pixels are refused.
        let mut document = single_palette_document_of(archive_with(&[PIXELS + 8]));
        let original = document.bytes().to_vec();
        let rgba: Vec<u8> = (0..64).flat_map(|_| [8, 4, 255, 255]).collect();
        assert!(matches!(
            document.import(TextureIndex(0), &rgba),
            Err(DocumentError::Patch(
                TexturePatchError::OverlapsPointer { .. }
            ))
        ));
        assert_eq!(document.bytes(), original);
        assert!(!document.can_undo() && !document.is_modified());
    }

    #[test]
    fn a_texture_drawn_through_two_palettes_keeps_them_on_import() {
        // The shared archive draws its image through two palettes.
        let mut document = document();
        assert_eq!(
            document.rebuildable_palette(TextureIndex(0)),
            Err(PaletteLock::SeveralPalettes(2))
        );
        let rgba = document.pixels(TextureIndex(0), UseIndex(0)).unwrap();
        let edit = document.import(TextureIndex(0), &rgba).unwrap();
        assert_eq!(
            edit.palette,
            Some(PaletteOutcome::Kept(PaletteLock::SeveralPalettes(2)))
        );
        assert!(!document.is_modified());
    }

    #[test]
    fn frames_keep_their_own_descriptors_apart_from_drawn_uses() {
        use super::{AnimationFrame, group};
        let pair = |image: u32| HsdTextureSourceId {
            image: ImageDescId(image),
            palette: None,
        };
        // Base data drawn by 0x10 and read by frame 0's 0x20; a frame the
        // scene never draws, read by 0x30.
        let (textures, undrawn) = group(
            [(pair(0x10), 0x100, 8, 8, Cmpr)],
            [
                (pair(0x20), 0x100, 8, 8, Cmpr),
                (pair(0x30), 0x200, 8, 8, Cmpr),
            ],
        );
        assert_eq!(textures[0].uses, [pair(0x10)]);
        assert_eq!(undrawn[0], [pair(0x20)]);
        assert!(textures[1].uses.is_empty());
        assert_eq!(undrawn[1], [pair(0x30)]);

        let mut frame = textures[1].clone();
        frame.frame = Some(AnimationFrame {
            frame: 1,
            frames: 2,
            sources: vec![pair(0x30)],
            replaces: vec![pair(0x10)],
        });
        assert_eq!(frame.descriptors(), [pair(0x30)]);
        assert_eq!(frame.drawn_as(), [pair(0x10)]);
    }

    #[test]
    fn pixel_data_two_textures_read_differently_is_not_edited() {
        let file = archive();
        let dat = DatFile::parse(&file).unwrap();
        let pair = |image, palette| HsdTextureSourceId {
            image: ImageDescId(image),
            palette: Some(TlutDescId(palette)),
        };
        let mut document = TextureDocument::from_parts(
            file,
            dat,
            [
                (pair(0, TLUT), PIXELS, 8, 8, Ci8),
                (pair(SECOND_IMAGE, SECOND_TLUT), PIXELS, 4, 4, Ci8),
            ],
        );
        assert_eq!(document.textures().len(), 2);

        for texture in 0..2 {
            let size = if document.textures()[texture].width == 8 {
                256
            } else {
                64
            };
            assert!(matches!(
                document.apply(TextureIndex(texture), UseIndex(0), &vec![0; size], None),
                Err(DocumentError::SharedPixels { .. })
            ));
            assert!(matches!(
                document.import(TextureIndex(texture), &vec![0; size]),
                Err(DocumentError::SharedPixels { .. })
            ));
        }
        assert!(!document.is_modified());
    }

    #[test]
    fn a_write_over_a_relocated_pointer_is_refused() {
        // The first image descriptor's data pointer is relocated; a color
        // write landing on it would leave a file that no longer parses.
        let mut document = document();
        let mut changes = Vec::new();
        assert!(matches!(
            document.write(&mut changes, 2, &[0xFF; 4]),
            Err(DocumentError::Patch(TexturePatchError::OverlapsPointer {
                data_offset: 2,
                site: 0
            }))
        ));
        assert!(changes.is_empty());
        assert_eq!(document.bytes(), archive());
    }

    #[test]
    fn unknown_textures_and_uses_are_errors() {
        let mut document = document();
        assert!(matches!(
            document.pixels(TextureIndex(1), UseIndex(0)),
            Err(DocumentError::UnknownTexture(TextureIndex(1)))
        ));
        assert!(matches!(
            document.apply(TextureIndex(0), UseIndex(2), &[0; 256], None),
            Err(DocumentError::UnknownUse {
                texture: TextureIndex(0),
                usage: UseIndex(2)
            })
        ));
        assert!(!document.is_modified());
    }
}
