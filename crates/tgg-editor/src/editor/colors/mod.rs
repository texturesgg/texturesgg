//! The Colors pane: the colors a file draws with that are not in its
//! textures, and changing them.
//!
//! A surface's color starts from its vertex colors or its material's diffuse
//! color, and its textures are usually multiplied onto that: a gray texture
//! over green vertices is a green platform. The pane lists those colors as
//! swatches, grouped by the textures the surfaces draw, so the swatches
//! beside a texture are what tints it. With a texture selected, only its
//! groups show.
//!
//! Pressing a swatch opens it for editing; applying writes the new color
//! everywhere the old one was, as one undo step. A group whose shading is
//! baked into many colors can have all of them turned around the hue circle
//! at once.
//!
//! The renderer bakes vertex colors into its geometry, so an edit reloads
//! the model from the document's bytes, at the same camera and frame.

mod pane;

use crate::editor::Editor;
use crate::load_model;
use dat_edit::{DocumentSurface, MaterialColor, TextureDocument};
use dat_parser::hsd::channel::HsdChannelBase;
use gpui::{Context, SharedString};

/// Surfaces that draw the same textures, and the colors under them.
pub(crate) struct ColorGroup {
    pub(crate) title: SharedString,
    /// The document textures these surfaces draw; empty for untextured ones.
    pub(crate) textures: Vec<usize>,
    /// Distinct colors, the most used first.
    pub(crate) swatches: Vec<Swatch>,
    /// The surfaces with vertex colors, for a hue shift.
    vertex_surfaces: Vec<usize>,
}

pub(crate) struct Swatch {
    pub(crate) rgba: [u8; 4],
    /// Whether where it is stored keeps alpha.
    alpha: bool,
    /// How many vertices, or materials, have it.
    pub(crate) uses: usize,
    source: Source,
}

/// Where a swatch's color is written.
enum Source {
    /// `(surface, color)` pairs of the document.
    Vertices(Vec<(usize, usize)>),
    /// Surfaces whose material diffuse it is.
    Diffuse(Vec<usize>),
}

/// The color being edited.
pub(crate) struct ColorPick {
    group: usize,
    target: Target,
}

enum Target {
    /// One swatch, and the color it will become.
    Swatch { index: usize, rgba: [u8; 4] },
    /// Every vertex color of the group, turned by this many degrees.
    Hue(f32),
}

enum Place {
    Vertex(usize),
    Diffuse,
}

impl Editor {
    /// List the document's colors again, after it opened or changed.
    pub(crate) fn read_colors(&mut self) {
        self.color_groups = match &self.document {
            Ok(document) => groups(document, &self.names),
            Err(_) => Vec::new(),
        };
        self.color_pick = None;
    }

    /// Draw the document's bytes: an edit the renderer cannot patch in place.
    pub(crate) fn reload_model(&mut self, cx: &mut Context<Self>) {
        let Ok(document) = &self.document else {
            return;
        };
        match load_model(&self.name, document.bytes(), self.references.as_deref()) {
            Ok(loaded) => self.viewport.update(cx, |viewport, cx| {
                viewport.replace_model(loaded.model, cx);
            }),
            Err(error) => self.set_notice(format!("Couldn't redraw: {error}"), true, cx),
        }
    }

    pub(super) fn pick_swatch(&mut self, group: usize, index: usize, cx: &mut Context<Self>) {
        let Some(swatch) = self
            .color_groups
            .get(group)
            .and_then(|group| group.swatches.get(index))
        else {
            return;
        };
        self.color_pick = Some(ColorPick {
            group,
            target: Target::Swatch {
                index,
                rgba: swatch.rgba,
            },
        });
        cx.notify();
    }

    pub(super) fn pick_hue(&mut self, group: usize, cx: &mut Context<Self>) {
        self.color_pick = Some(ColorPick {
            group,
            target: Target::Hue(0.0),
        });
        cx.notify();
    }

    pub(super) fn set_channel(&mut self, channel: usize, value: f32, cx: &mut Context<Self>) {
        if let Some(ColorPick {
            target: Target::Swatch { rgba, .. },
            ..
        }) = &mut self.color_pick
        {
            rgba[channel] = value.round().clamp(0.0, 255.0) as u8;
            cx.notify();
        }
    }

    pub(super) fn set_hue(&mut self, degrees: f32, cx: &mut Context<Self>) {
        if let Some(ColorPick {
            target: Target::Hue(hue),
            ..
        }) = &mut self.color_pick
        {
            *hue = degrees;
            cx.notify();
        }
    }

    fn cancel_pick(&mut self, cx: &mut Context<Self>) {
        self.color_pick = None;
        cx.notify();
    }

    /// Write the picked color, redraw, and say what changed.
    pub(super) fn apply_pick(&mut self, cx: &mut Context<Self>) {
        let (Some(pick), Ok(document)) = (self.color_pick.take(), &mut self.document) else {
            return;
        };
        let Some(group) = self.color_groups.get(pick.group) else {
            return;
        };
        let (written, what) = match pick.target {
            Target::Swatch { index, rgba } => {
                let Some(swatch) = group.swatches.get(index) else {
                    return;
                };
                let written = match &swatch.source {
                    Source::Vertices(colors) => document.recolor_vertices(colors, rgba),
                    Source::Diffuse(surfaces) => {
                        document.set_material_color(surfaces, MaterialColor::Diffuse, rgba)
                    }
                };
                (written, format!("{} to {}", hex(swatch.rgba), hex(rgba)))
            }
            Target::Hue(degrees) => (
                document.map_vertex_colors(&group.vertex_surfaces, |rgba| turn_hue(rgba, degrees)),
                format!("{} colors by {degrees:+.0}°", group.swatches.len()),
            ),
        };
        match written {
            Ok(()) => {
                let title = group.title.clone();
                self.read_colors();
                self.reload_model(cx);
                self.set_notice(format!("Recolored {title}: {what}"), false, cx);
            }
            Err(error) => self.set_notice(format!("Couldn't recolor: {error}"), true, cx),
        }
    }
}

/// The groups of a document, in the order its surfaces first draw them.
pub(crate) fn groups(document: &TextureDocument, names: &[String]) -> Vec<ColorGroup> {
    let mut groups: Vec<ColorGroup> = Vec::new();
    for (index, surface) in document.surfaces().iter().enumerate() {
        let colors = colors_of(surface);
        if colors.is_empty() {
            continue;
        }
        let group = match groups
            .iter()
            .position(|group| group.textures == surface.textures)
        {
            Some(group) => group,
            None => {
                groups.push(ColorGroup {
                    title: title(&surface.textures, names),
                    textures: surface.textures.clone(),
                    swatches: Vec::new(),
                    vertex_surfaces: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[group];
        if surface.base == HsdChannelBase::Vertex {
            group.vertex_surfaces.push(index);
        }
        for (rgba, alpha, uses, place) in colors {
            let swatch = match group.swatches.iter_mut().find(|swatch| {
                swatch.rgba == rgba
                    && matches!(
                        (&swatch.source, &place),
                        (Source::Vertices(_), Place::Vertex(_))
                            | (Source::Diffuse(_), Place::Diffuse)
                    )
            }) {
                Some(swatch) => swatch,
                None => {
                    group.swatches.push(Swatch {
                        rgba,
                        alpha: false,
                        uses: 0,
                        source: match place {
                            Place::Vertex(_) => Source::Vertices(Vec::new()),
                            Place::Diffuse => Source::Diffuse(Vec::new()),
                        },
                    });
                    group.swatches.last_mut().expect("pushed above")
                }
            };
            swatch.alpha |= alpha;
            swatch.uses += uses;
            match (&mut swatch.source, place) {
                (Source::Vertices(colors), Place::Vertex(color)) => colors.push((index, color)),
                (Source::Diffuse(surfaces), Place::Diffuse) => surfaces.push(index),
                _ => unreachable!("matched by source above"),
            }
        }
    }
    for group in &mut groups {
        group
            .swatches
            .sort_by_key(|swatch| std::cmp::Reverse(swatch.uses));
    }
    groups
}

/// The colors a surface's base comes from: `(rgba, keeps alpha, uses, place)`.
/// A surface whose base is white takes none from either.
fn colors_of(surface: &DocumentSurface) -> Vec<([u8; 4], bool, usize, Place)> {
    match surface.base {
        HsdChannelBase::Vertex => surface
            .vertex_colors
            .iter()
            .enumerate()
            .map(|(index, color)| {
                (
                    color.rgba,
                    color.has_alpha(),
                    color.vertices(),
                    Place::Vertex(index),
                )
            })
            .collect(),
        HsdChannelBase::Material => surface
            .material
            .iter()
            .map(|material| {
                (
                    material.get(MaterialColor::Diffuse),
                    true,
                    1,
                    Place::Diffuse,
                )
            })
            .collect(),
        HsdChannelBase::White => Vec::new(),
    }
}

fn title(textures: &[usize], names: &[String]) -> SharedString {
    if textures.is_empty() {
        return "No texture".into();
    }
    textures
        .iter()
        .filter_map(|&texture| names.get(texture).map(String::as_str))
        .collect::<Vec<_>>()
        .join(" + ")
        .into()
}

/// `rgba` with its hue turned by `degrees`, keeping saturation, value and
/// alpha.
pub(crate) fn turn_hue(rgba: [u8; 4], degrees: f32) -> [u8; 4] {
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

fn hex([r, g, b, _]: [u8; 4]) -> String {
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
