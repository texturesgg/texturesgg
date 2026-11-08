//! The colors a file draws with that aren't in its textures, and changing
//! them.
//!
//! A surface's color starts from its vertex colors or its material's diffuse
//! color, and its textures are usually multiplied onto that: a gray texture
//! over green vertices is a green platform. The colors group by the textures
//! their surfaces draw, so a group's swatches are what tints those textures.
//! A swatch changes everywhere its color is written, as one undo step; a
//! group's vertex colors can all turn around the hue circle at once.

use dat_edit::{DocumentError, DocumentSurface, MaterialColor, TextureDocument, VertexColorId};
use dat_parser::hsd::channel::HsdChannelBase;

/// Surfaces that draw the same textures, and the colors under them.
#[derive(Clone, Debug)]
pub struct ColorGroup {
    /// The textures' names joined ("Head #2 + Head #3"), or "No texture".
    pub title: String,
    /// The document textures these surfaces draw; empty for untextured ones.
    pub textures: Vec<usize>,
    /// Distinct colors, the most used first.
    pub swatches: Vec<Swatch>,
    /// The surfaces with vertex colors, for a hue shift.
    pub vertex_surfaces: Vec<usize>,
}

impl ColorGroup {
    /// Whether its colors can turn together: it has vertex colors, and more
    /// than one color.
    pub fn shifts_hue(&self) -> bool {
        !self.vertex_surfaces.is_empty() && self.swatches.len() > 1
    }
}

#[derive(Clone, Debug)]
pub struct Swatch {
    pub rgba: [u8; 4],
    /// Whether where it's stored keeps alpha.
    pub alpha: bool,
    /// How many vertices, or materials, have it.
    pub uses: usize,
    pub source: Source,
}

/// Where a swatch's color is written.
#[derive(Clone, Debug)]
pub enum Source {
    /// The document's `(surface, color)` pairs.
    Vertices(Vec<VertexColorId>),
    /// Surfaces whose material diffuse it is.
    Diffuse(Vec<usize>),
}

/// A change to a group's colors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// One swatch, to this color.
    Swatch { index: usize, rgba: [u8; 4] },
    /// Every vertex color of the group, turned by this many degrees.
    Hue(f32),
}

enum Place {
    Vertex(usize),
    Diffuse,
}

/// A document's groups, in the order its surfaces first draw them, titled
/// with its textures' `names`.
pub fn groups(document: &TextureDocument, names: &[String]) -> Vec<ColorGroup> {
    let mut groups: Vec<ColorGroup> = Vec::new();
    for (index, surface) in document.surfaces().iter().enumerate() {
        let colors = colors_of(surface);
        if colors.is_empty() {
            continue;
        }
        let textures: Vec<usize> = surface.textures.iter().map(|texture| texture.0).collect();
        let group = match groups.iter().position(|group| group.textures == textures) {
            Some(group) => group,
            None => {
                groups.push(ColorGroup { title: title(&textures, names), textures, swatches: Vec::new(), vertex_surfaces: Vec::new() });
                groups.len() - 1
            }
        };
        let group = &mut groups[group];
        if surface.base == HsdChannelBase::Vertex {
            group.vertex_surfaces.push(index);
        }
        for (rgba, alpha, uses, place) in colors {
            let same = |swatch: &&mut Swatch| {
                swatch.rgba == rgba
                    && matches!((&swatch.source, &place), (Source::Vertices(_), Place::Vertex(_)) | (Source::Diffuse(_), Place::Diffuse))
            };
            let swatch = match group.swatches.iter_mut().find(same) {
                Some(swatch) => swatch,
                None => {
                    let source = match place {
                        Place::Vertex(_) => Source::Vertices(Vec::new()),
                        Place::Diffuse => Source::Diffuse(Vec::new()),
                    };
                    group.swatches.push(Swatch { rgba, alpha: false, uses: 0, source });
                    group.swatches.last_mut().expect("pushed above")
                }
            };
            swatch.alpha |= alpha;
            swatch.uses += uses;
            match (&mut swatch.source, place) {
                (Source::Vertices(colors), Place::Vertex(color)) => colors.push(VertexColorId { surface: index, color }),
                (Source::Diffuse(surfaces), Place::Diffuse) => surfaces.push(index),
                _ => unreachable!("matched by source above"),
            }
        }
    }
    for group in &mut groups {
        group.swatches.sort_by_key(|swatch| std::cmp::Reverse(swatch.uses));
    }
    groups
}

/// Write `change` to `group`'s colors in `document`; what changed, in the
/// player's words ("#60E060 to #6060E0").
pub fn apply(document: &mut TextureDocument, group: &ColorGroup, change: Change) -> Result<String, DocumentError> {
    match change {
        Change::Swatch { index, rgba } => {
            let Some(swatch) = group.swatches.get(index) else { return Ok(String::new()) };
            match &swatch.source {
                Source::Vertices(colors) => document.recolor_vertices(colors, rgba)?,
                Source::Diffuse(surfaces) => document.set_material_color(surfaces, MaterialColor::Diffuse, rgba)?,
            }
            Ok(format!("{} to {}", hex(swatch.rgba), hex(rgba)))
        }
        Change::Hue(degrees) => {
            document.map_vertex_colors(&group.vertex_surfaces, |rgba| turn_hue(rgba, degrees))?;
            Ok(format!("{} colors by {degrees:+.0}°", group.swatches.len()))
        }
    }
}

/// The colors a surface's base comes from: `(rgba, keeps alpha, uses, place)`.
/// A surface whose base is white takes none from either.
fn colors_of(surface: &DocumentSurface) -> Vec<([u8; 4], bool, usize, Place)> {
    match surface.base {
        HsdChannelBase::Vertex => surface
            .vertex_colors
            .iter()
            .enumerate()
            .map(|(index, color)| (color.rgba, color.has_alpha(), color.vertices(), Place::Vertex(index)))
            .collect(),
        HsdChannelBase::Material => {
            surface.material.iter().map(|material| (material.get(MaterialColor::Diffuse), true, 1, Place::Diffuse)).collect()
        }
        HsdChannelBase::White => Vec::new(),
    }
}

fn title(textures: &[usize], names: &[String]) -> String {
    if textures.is_empty() {
        return "No texture".into();
    }
    textures.iter().filter_map(|&texture| names.get(texture).map(String::as_str)).collect::<Vec<_>>().join(" + ")
}

/// `rgba` with its hue turned by `degrees`, keeping saturation, value and
/// alpha.
pub fn turn_hue(rgba: [u8; 4], degrees: f32) -> [u8; 4] {
    let [r, g, b] = [rgba[0], rgba[1], rgba[2]].map(|channel| f32::from(channel) / 255.0);
    let a = rgba[3];
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    if chroma == 0.0 {
        return rgba;
    }
    let hue = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    let hue = (hue * 60.0 + degrees).rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
    let (r, g, b) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let channel = |value: f32| ((value + min) * 255.0).round() as u8;
    [channel(r), channel(g), channel(b), a]
}

/// `#RRGGBB`.
pub fn hex([r, g, b, _]: [u8; 4]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(test)]
mod tests {
    use super::turn_hue;

    #[test]
    fn turning_the_hue_keeps_how_light_and_how_gray_a_color_is() {
        // Stadium's platform green to blue: a third of the way round.
        assert_eq!(turn_hue([96, 224, 96, 255], 120.0), [96, 96, 224, 255]);
        assert_eq!(turn_hue([96, 224, 96, 255], -120.0), [224, 96, 96, 255]);
        assert_eq!(turn_hue([96, 224, 96, 77], 360.0), [96, 224, 96, 77]);
        // A gray has no hue to turn.
        assert_eq!(turn_hue([128, 128, 128, 255], 90.0), [128, 128, 128, 255]);
        assert_eq!(turn_hue([255, 0, 0, 255], 60.0), [255, 255, 0, 255]);
    }
}
