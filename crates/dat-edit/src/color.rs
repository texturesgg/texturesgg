//! Where a display object's color comes from besides its textures, and the
//! places in the file that hold it.
//!
//! A stage's color is mostly not in its textures. TEV starts from the
//! material's diffuse color or from the vertex colors, and a texture is
//! usually multiplied onto that: Pokemon Stadium's platforms are a gray
//! texture over green vertices. Vertex colors are written DIRECT in the
//! display list beside each vertex, in a 2-, 3- or 4-byte format.
//!
//! A surface lists each distinct vertex color once, with every place it is
//! written, so recoloring is replacing a swatch. The new color is written in
//! the same format at the same places: nothing moves, as with a texture.

use crate::document::{DocumentTexture, TextureIndex};
use dat_parser::DatFile;
use dat_parser::gx::display_list::{DirectColor, decode_direct_color};
use dat_parser::gx::{GxAttrName, GxAttrType, GxCompTypeClr};
use dat_parser::hsd::channel::{HsdChannelBase, HsdColorChannelState};
use dat_parser::hsd::scene::{DObjId, HsdDisplayObject, HsdScene};
use std::collections::HashMap;

/// One display object and what colors it besides its textures.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentSurface {
    pub dobj: DObjId,
    /// The document textures it draws, in stage order.
    pub textures: Vec<TextureIndex>,
    /// What its color starts from, before any texture.
    pub base: HsdChannelBase,
    /// Its distinct vertex colors, the most used first. Empty when it has no
    /// direct vertex colors.
    pub vertex_colors: Vec<VertexColor>,
    /// Its material's colors, when it has a material.
    pub material: Option<MaterialColors>,
}

/// One color among a surface's vertices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexColor {
    /// As the game reads it.
    pub rgba: [u8; 4],
    pub format: GxCompTypeClr,
    /// Data-section offsets of every vertex that has it.
    pub(crate) sites: Vec<u32>,
}

impl VertexColor {
    /// How many vertices have this color.
    pub fn vertices(&self) -> usize {
        self.sites.len()
    }

    /// Whether the format keeps alpha; RGB565 and RGB8 read as opaque.
    pub fn has_alpha(&self) -> bool {
        !matches!(self.format, GxCompTypeClr::Rgb565 | GxCompTypeClr::Rgb8)
    }
}

/// A material's three colors, RGBA8 in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialColors {
    /// Data-section offset of the material's color block.
    pub(crate) offset: u32,
    pub ambient: [u8; 4],
    pub diffuse: [u8; 4],
    pub specular: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialColor {
    Ambient,
    Diffuse,
    Specular,
}

impl MaterialColor {
    /// Where the color is in the material's block.
    pub(crate) const fn offset(self) -> u32 {
        match self {
            Self::Ambient => 0x00,
            Self::Diffuse => 0x04,
            Self::Specular => 0x08,
        }
    }
}

impl MaterialColors {
    pub fn get(&self, color: MaterialColor) -> [u8; 4] {
        match color {
            MaterialColor::Ambient => self.ambient,
            MaterialColor::Diffuse => self.diffuse,
            MaterialColor::Specular => self.specular,
        }
    }

    /// Read the block at `offset`.
    fn read(dat: &DatFile, offset: u32) -> Option<Self> {
        let block = dat.data_slice(offset, 12)?;
        let color = |at: usize| [block[at], block[at + 1], block[at + 2], block[at + 3]];
        Some(Self {
            offset,
            ambient: color(0),
            diffuse: color(4),
            specular: color(8),
        })
    }
}

/// Every display object of `scene`, in scene order.
pub(crate) fn surfaces(
    dat: &DatFile,
    scene: &HsdScene,
    textures: &[DocumentTexture],
) -> Vec<DocumentSurface> {
    scene
        .roots
        .iter()
        .flat_map(|root| &root.joints)
        .flat_map(|joint| &joint.display_objects)
        .map(|display_object| surface(dat, scene, textures, display_object))
        .collect()
}

fn surface(
    dat: &DatFile,
    scene: &HsdScene,
    textures: &[DocumentTexture],
    display_object: &HsdDisplayObject,
) -> DocumentSurface {
    let material = display_object.material.as_ref();
    let mut drawn = Vec::new();
    for stage in material.map_or(&[][..], |material| &material.textures) {
        let Some(id) = stage
            .texture
            .and_then(|index| scene.textures.get(index.0))
            .map(|texture| texture.id)
        else {
            continue;
        };
        if let Some(texture) = textures
            .iter()
            .position(|texture| texture.uses.contains(&id))
            .map(TextureIndex)
            && !drawn.contains(&texture)
        {
            drawn.push(texture);
        }
    }

    // Distinct colors in first-seen order, then the most used first.
    let mut colors: Vec<VertexColor> = Vec::new();
    let mut seen: HashMap<(u32, [u8; 4]), usize> = HashMap::new();
    for polygon in &display_object.polygons {
        let Some(format) = polygon
            .attributes
            .iter()
            .find(|attribute| {
                attribute.attr_name == GxAttrName::Color0
                    && attribute.attr_type == GxAttrType::Direct
            })
            .and_then(|attribute| attribute.comp_type.color())
        else {
            continue;
        };
        for vertex in polygon
            .primitive_groups
            .iter()
            .flat_map(|group| &group.vertices)
        {
            let Some(DirectColor { rgba, offset: site }) = vertex.color0 else {
                continue;
            };
            let index = *seen.entry((format as u32, rgba)).or_insert_with(|| {
                colors.push(VertexColor {
                    rgba,
                    format,
                    sites: Vec::new(),
                });
                colors.len() - 1
            });
            // A display list shared by two polygons lists its vertices twice.
            if !colors[index].sites.contains(&site) {
                colors[index].sites.push(site);
            }
        }
    }
    colors.sort_by_key(|color| std::cmp::Reverse(color.sites.len()));

    DocumentSurface {
        dobj: display_object.source_id,
        textures: drawn,
        base: HsdColorChannelState::from_render_flags(
            material.map_or(0, |material| material.render_flags),
        )
        .base,
        vertex_colors: colors,
        material: material
            // The MObj's material pointer, at 0x0C.
            .and_then(|material| dat.resolve_pointer(material.source_id.0 + 0x0C).ok()?)
            .and_then(|offset| MaterialColors::read(dat, offset)),
    }
}

/// Read every surface's colors again from `dat`, after its bytes changed.
/// Sites do not move; only what they hold does.
pub(crate) fn refresh(dat: &DatFile, surfaces: &mut [DocumentSurface]) {
    for surface in surfaces {
        for color in &mut surface.vertex_colors {
            if let Some(rgba) = color
                .sites
                .first()
                .and_then(|&site| dat.data_slice(site, color.format.byte_len()))
                .and_then(|bytes| decode_direct_color(color.format, bytes))
            {
                color.rgba = rgba;
            }
        }
        if let Some(material) = &mut surface.material
            && let Some(read) = MaterialColors::read(dat, material.offset)
        {
            *material = read;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{DocumentError, TextureDocument, Undone, VertexColorId};

    const GREEN: [u8; 2] = [0x67, 0x0C];
    const YELLOW: [u8; 2] = [0xFF, 0xE6];
    /// Two green vertices and a yellow one, RGB565, then a material block.
    const VERTICES: [u32; 3] = [0x10, 0x14, 0x18];
    const MATERIAL: u32 = 0x20;

    /// Pokemon Stadium's platform in miniature.
    fn document() -> TextureDocument {
        let mut data = vec![0u8; 0x30];
        for (site, color) in VERTICES.into_iter().zip([GREEN, GREEN, YELLOW]) {
            data[site as usize..][..2].copy_from_slice(&color);
        }
        data[MATERIAL as usize..][..12]
            .copy_from_slice(&[10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255]);
        let mut file = vec![0u8; 0x20];
        file[..4].copy_from_slice(&(0x20 + data.len() as u32).to_be_bytes());
        file[4..8].copy_from_slice(&(data.len() as u32).to_be_bytes());
        file.extend(data);
        let color = |rgba, sites: &[u32]| VertexColor {
            rgba,
            format: GxCompTypeClr::Rgb565,
            sites: sites.to_vec(),
        };
        TextureDocument::with_surfaces(
            file,
            vec![DocumentSurface {
                dobj: DObjId(0x40),
                textures: Vec::new(),
                base: HsdChannelBase::Vertex,
                vertex_colors: vec![
                    color([96, 224, 96, 255], &VERTICES[..2]),
                    color([248, 252, 48, 255], &VERTICES[2..]),
                ],
                material: Some(MaterialColors {
                    offset: MATERIAL,
                    ambient: [10, 20, 30, 255],
                    diffuse: [40, 50, 60, 255],
                    specular: [70, 80, 90, 255],
                }),
            }],
        )
    }

    fn color_id(surface: usize, color: usize) -> VertexColorId {
        VertexColorId { surface, color }
    }

    fn data(document: &TextureDocument, offset: u32, len: usize) -> &[u8] {
        &document.bytes()[0x20 + offset as usize..][..len]
    }

    #[test]
    fn recoloring_a_swatch_rewrites_every_vertex_that_has_it() {
        let mut document = document();
        let original = document.bytes().to_vec();
        // Blue, which RGB565 stores as its nearest: 5 bits of blue.
        document
            .recolor_vertices(&[color_id(0, 0)], [0, 0, 255, 255])
            .unwrap();
        assert_eq!(data(&document, VERTICES[0], 2), [0x00, 0x1F]);
        assert_eq!(data(&document, VERTICES[1], 2), [0x00, 0x1F]);
        assert_eq!(
            data(&document, VERTICES[2], 2),
            YELLOW,
            "the arrows keep theirs"
        );
        // The surface lists what the game reads back, not what was asked.
        assert_eq!(
            document.surfaces()[0].vertex_colors[0].rgba,
            [0, 0, 248, 255]
        );
        assert_eq!(document.surfaces()[0].vertex_colors[0].vertices(), 2);
        assert_eq!(document.bytes().len(), original.len());
        assert!(document.is_modified());

        assert_eq!(document.undo().unwrap().unwrap(), Undone::Colors);
        assert_eq!(document.bytes(), original);
        assert_eq!(
            document.surfaces()[0].vertex_colors[0].rgba,
            [96, 224, 96, 255]
        );
        assert!(!document.is_modified());
        assert_eq!(document.redo().unwrap().unwrap(), Undone::Colors);
        assert_eq!(data(&document, VERTICES[0], 2), [0x00, 0x1F]);
    }

    #[test]
    fn mapping_a_surface_changes_all_its_colors_in_one_step() {
        let mut document = document();
        // Swap red and blue.
        document
            .map_vertex_colors(&[0], |[r, g, b, a]| [b, g, r, a])
            .unwrap();
        let colors = &document.surfaces()[0].vertex_colors;
        assert_eq!(
            colors[0].rgba,
            [96, 224, 96, 255],
            "green has equal red and blue"
        );
        assert_eq!(colors[1].rgba, [48, 252, 248, 255]);
        document.undo();
        assert!(!document.can_undo(), "one step");
        assert_eq!(data(&document, VERTICES[2], 2), YELLOW);
    }

    #[test]
    fn a_material_color_is_four_bytes_in_its_block() {
        let mut document = document();
        document
            .set_material_color(&[0], MaterialColor::Diffuse, [1, 2, 3, 4])
            .unwrap();
        assert_eq!(
            data(&document, MATERIAL, 12),
            [10, 20, 30, 255, 1, 2, 3, 4, 70, 80, 90, 255]
        );
        let material = document.surfaces()[0].material.unwrap();
        assert_eq!(material.get(MaterialColor::Diffuse), [1, 2, 3, 4]);
        assert_eq!(material.get(MaterialColor::Ambient), [10, 20, 30, 255]);
    }

    #[test]
    fn an_unchanged_color_and_unknown_targets_leave_no_step() {
        let mut document = document();
        document
            .recolor_vertices(&[color_id(0, 0)], [96, 224, 96, 255])
            .unwrap();
        assert!(!document.can_undo() && !document.is_modified());
        assert!(matches!(
            document.recolor_vertices(&[color_id(1, 0)], [0; 4]),
            Err(DocumentError::UnknownSurface(1))
        ));
        assert!(matches!(
            document.recolor_vertices(&[color_id(0, 2)], [0; 4]),
            Err(DocumentError::UnknownColor(VertexColorId {
                surface: 0,
                color: 2
            }))
        ));
        assert!(!document.can_undo());
    }
}
