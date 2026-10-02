//! Renderer-neutral projection of parsed HSD model data.
//!
//! HSD uses a vocabulary that predates modern scene formats. For readers from
//! a conventional DCC or real-time engine background, the closest mental model is:
//!
//! - **JObj**: a transform node, often also a skeleton joint/bone.
//! - **DObj**: a draw grouping that connects geometry with material state.
//! - **PObj**: a geometry batch/display list, roughly one primitive batch.
//! - **MObj**: material entry point; admitted PE state is explicit and bounded.
//! - **TObj**: an *ordered texture stage*, not merely an image reference. Its
//!   texgen, transform, sampler, blend, and combiner state affect the surface.
//!
//! [`HsdScene`] owns a rendering-oriented view of those objects. It retains DAT
//! offsets as provenance and keeps source ordering, GX topology, currently
//! decoded material-stage state and skinning inputs without selecting a renderer
//! projection. It intentionally does not choose a GPU API, bake transforms,
//! collapse texture stages, or merge geometry. The
//! original [`crate::DatFile`] remains canonical; nothing here writes it back.

mod build;
pub(crate) mod discovery;
mod limits;

#[cfg(test)]
pub(crate) use build::ADMITTED_CUSTOM_PE;
pub use limits::{HSD_SCENE_MAX_DAT_BYTES, HsdSceneLimits, hsd_scene_limits};

use super::channel::HsdColorChannelState;
use super::pe::{HsdDrawPass, HsdPixelEngineState};
use super::tev::{HsdTObjTevEvaluationError, HsdTObjTevProgram, HsdTObjTevRegisters};
use crate::descriptor::DescriptorParseError;
use crate::descriptor::mobj::Material;
use crate::descriptor::pobj::GxAttribute;
use crate::descriptor::traversal::DescriptorKind;
use crate::gx::display_list::PrimitiveGroup;
use crate::gx::vertex::DecodedPrimitive;
use crate::gx::{GxAttrName, GxAttrType};
use crate::math::Mat4;

macro_rules! source_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(pub u32);
    };
}

source_id!(
    /// Stable identity of a JObj within one DAT data section.
    JObjId
);
source_id!(
    /// Stable identity of a DObj within one DAT data section.
    DObjId
);
source_id!(
    /// Stable identity of a PObj within one DAT data section.
    PObjId
);
source_id!(
    /// Stable identity of an MObj within one DAT data section.
    MObjId
);
source_id!(
    /// Stable identity of a TObj within one DAT data section.
    TObjId
);
source_id!(
    /// Stable identity of an HSD image descriptor within one DAT data section.
    ImageDescId
);
source_id!(
    /// Stable identity of an HSD palette descriptor within one DAT data section.
    TlutDescId
);
source_id!(
    /// Source identity of an HSD pixel-engine descriptor.
    PeDescId
);
source_id!(
    /// Source identity of an HSD texture-LOD descriptor.
    TexLodDescId
);
source_id!(
    /// Source identity of a per-texture TEV descriptor.
    TevDescId
);

/// Root-local joint index used for hierarchy references.
///
/// This is intentionally separate from [`JObjId`]: two discovered model roots
/// can reach the same source object, while each flattened hierarchy needs its
/// own dense array index for parent/child traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HsdJointIndex(pub usize);

/// Index into [`HsdScene::textures`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HsdTextureIndex(pub usize);

/// A rendering-oriented projection of the model roots in a DAT file.
#[derive(Debug)]
pub struct HsdScene {
    pub roots: Vec<HsdSceneRoot>,
    /// Decoded image/palette pairs interned by descriptor identity.
    pub textures: Vec<HsdTexture>,
}

#[derive(Debug)]
pub struct HsdSceneRoot {
    pub source_id: JObjId,
    pub name: Option<String>,
    /// Depth-first JObj order, with hierarchy expressed by root-local indices.
    pub joints: Vec<HsdJoint>,
}

/// Transform node/bone derived from a JObj.
#[derive(Debug)]
pub struct HsdJoint {
    pub source_id: JObjId,
    pub parent: Option<HsdJointIndex>,
    /// HSD draw edges: owned children normally; exactly one referenced target for
    /// INSTANCE joints. A referenced target keeps its original `parent`.
    pub children: Vec<HsdJointIndex>,
    pub flags: u32,
    /// Source local SRT. World matrices are a renderer/exporter concern.
    pub local: HsdTransform,
    /// Inverse bind matrix used by skinned geometry, when HSD provides one.
    pub inverse_bind_transform: Option<Mat4>,
    pub display_objects: Vec<HsdDisplayObject>,
}

/// HSD's scale/rotation/translation triplet.
#[derive(Clone, Copy, Debug)]
pub struct HsdTransform {
    pub scale: [f32; 3],
    /// Euler XYZ rotation in radians; HSD composes it as `Rz * Ry * Rx`.
    pub rotation: [f32; 3],
    pub translation: [f32; 3],
}

/// Draw grouping that associates PObj geometry with MObj material state.
#[derive(Debug)]
pub struct HsdDisplayObject {
    pub source_id: DObjId,
    pub material: Option<HsdMaterial>,
    pub polygons: Vec<HsdPolygon>,
}

impl HsdDisplayObject {
    /// The MObj render mode, or zero without an MObj: a preview fallback, not
    /// a source rule for a null MObj.
    fn render_flags(&self) -> u32 {
        self.material
            .as_ref()
            .map_or(0, |material| material.render_flags)
    }

    /// Pixel-engine state: the material's PEDesc, else its render mode.
    pub fn pixel_engine(&self) -> HsdPixelEngineState {
        self.material.as_ref().map_or_else(
            || HsdPixelEngineState::from_render_flags(0),
            HsdMaterial::pixel_engine_state,
        )
    }

    /// The display pass the render mode selects. `None` for NO_ZUPDATE
    /// without XLU, which the game itself refuses.
    pub fn pass(&self) -> Option<HsdDrawPass> {
        HsdDrawPass::from_render_flags(self.render_flags())
    }

    /// How the render mode uses the color channels.
    pub fn channels(&self) -> HsdColorChannelState {
        HsdColorChannelState::from_render_flags(self.render_flags())
    }
}

/// One GX geometry/display-list batch.
#[derive(Debug)]
pub struct HsdPolygon {
    pub source_id: PObjId,
    /// Raw PObj flags, including front/back culling and envelope mode.
    pub flags: u16,
    /// GX attribute layout, analogous to a vertex input declaration.
    pub attributes: Vec<GxAttribute>,
    /// Original GX primitive grouping. Keeping this avoids making triangles the
    /// canonical representation and preserves strips/fans/quads for fidelity.
    pub primitive_groups: Vec<PrimitiveGroup>,
    /// Convenience decoded/triangulated view used by today's renderers.
    pub decoded: DecodedPrimitive,
    pub binding: HsdPolygonBinding,
}

/// Geometry-to-joint binding before any exporter-specific world-space baking.
#[derive(Debug)]
pub enum HsdPolygonBinding {
    /// Rigid PObjs can carry a source joint pointer. The scene preserves that
    /// relation so adapters do not have to substitute the parent JObj.
    Rigid { joint: Option<JObjId> },
    /// Skinned PObjs select an ordered envelope with `PnMtxIdx / 3`. This is a
    /// GameCube GX matrix-palette convention, not a direct modern bone index.
    Envelope {
        source_offset: Option<u32>,
        entries: Vec<HsdEnvelope>,
    },
}

impl HsdPolygon {
    pub fn has_envelope(&self) -> bool {
        matches!(self.binding, HsdPolygonBinding::Envelope { .. })
    }

    /// Bit i marks an enabled GX TEXi descriptor, whether or not it decoded.
    pub fn tex_coord_attribute_mask(&self) -> u8 {
        self.attributes.iter().fold(0, |mask, attribute| {
            if attribute.attr_name.is_tex_coord() && attribute.attr_type != GxAttrType::None {
                mask | (1 << (attribute.attr_name as u32 - GxAttrName::Tex0 as u32))
            } else {
                mask
            }
        })
    }

    pub fn envelopes(&self) -> &[HsdEnvelope] {
        match &self.binding {
            HsdPolygonBinding::Envelope { entries, .. } => entries,
            HsdPolygonBinding::Rigid { .. } => &[],
        }
    }
}

#[derive(Debug)]
pub struct HsdEnvelope {
    pub source_offset: u32,
    /// Source order and weights are retained without normalization.
    pub weights: Vec<HsdWeight>,
}

#[derive(Debug)]
pub struct HsdWeight {
    pub joint: JObjId,
    pub weight: f32,
}

#[derive(Debug)]
pub struct HsdMaterial {
    pub source_id: MObjId,
    pub render_flags: u32,
    /// Present when the MObj carries a PEDesc, decoded to the state it selects.
    pub custom_pe: Option<HsdCustomPe>,
    pub colors: Option<Material>,
    /// Ordered texture stages. Selecting one "base color texture" is a lossy
    /// renderer policy and therefore does not happen in the scene model.
    pub textures: Vec<HsdTextureObject>,
}

/// A custom PEDesc and the PE state it selects in place of the render flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HsdCustomPe {
    pub source_id: PeDescId,
    pub state: HsdPixelEngineState,
}

/// Validated custom color/alpha expression and its static register state.
#[derive(Clone, Copy, Debug)]
pub struct HsdCustomTev {
    pub program: HsdTObjTevProgram,
    pub registers: HsdTObjTevRegisters,
}

/// One TObj texture-stage usage, separate from the shared decoded image asset.
#[derive(Debug)]
pub struct HsdTextureObject {
    pub source_id: TObjId,
    /// Source descriptor identities are retained even when decoding fails.
    pub image_descriptor: Option<ImageDescId>,
    pub palette_descriptor: Option<TlutDescId>,
    pub texture: Option<HsdTextureIndex>,
    pub tex_map_id: u32,
    /// Raw GX descriptor provenance; consumers use `coordinates()` instead.
    pub tex_gen_src: u32,
    pub transform: HsdTransform,
    pub wrap_s: u32,
    pub wrap_t: u32,
    pub repeat_s: u8,
    pub repeat_t: u8,
    pub blending: f32,
    pub mag_filter: u32,
    /// Opaque descriptor provenance retained for later LOD/TEV decoding.
    pub lod_descriptor: Option<TexLodDescId>,
    pub tev_descriptor: Option<TevDescId>,
    /// Validated only when either serialized custom color/alpha gate is active.
    pub custom_tev: Option<HsdCustomTev>,
    /// Raw TObj state, including coordinate and color/alpha combiner behavior.
    pub flags: u32,
}

impl HsdTextureObject {
    /// Resolve source texture semantics without allocating or copying image data.
    pub fn coordinates(&self) -> super::texture::HsdTextureCoordinates {
        super::texture::resolve_texture_coordinates(
            self.tex_gen_src,
            self.flags,
            &self.transform,
            [self.repeat_s, self.repeat_t],
            self.wrap_t,
        )
    }
}

/// Exact source descriptor-pair identity of a decoded image/palette pair.
///
/// Descriptor identity is intentionally stronger than a pixel-pointer cache key:
/// distinct descriptors may share pixel pointers while differing in dimensions
/// or palette metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HsdTextureSourceId {
    pub image: ImageDescId,
    pub palette: Option<TlutDescId>,
}

#[derive(Debug)]
pub struct HsdTexture {
    pub id: HsdTextureSourceId,
    pub image: HsdImageSource,
    pub palette: Option<HsdPaletteSource>,
    /// Renderer-friendly RGBA8. `None` retains a valid source reference whose
    /// format/data could not be decoded by the current GX decoder.
    pub rgba: Option<Vec<u8>>,
}

impl HsdTexture {
    /// Identity of the decoded pixels. HAL archives often give each TObj its
    /// own image descriptor over shared pixel data, so scene textures with
    /// equal keys decode to the same RGBA; about half of the stock costume
    /// scene textures share their pixels with another.
    pub fn content_key(&self) -> HsdTextureContentKey {
        HsdTextureContentKey {
            image_data: self.image.data_offset,
            width: self.image.width,
            height: self.image.height,
            format: self.image.format,
            palette: self
                .palette
                .as_ref()
                .map(|palette| (palette.data_offset, palette.format, palette.color_count)),
        }
    }
}

/// See [`HsdTexture::content_key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HsdTextureContentKey {
    pub image_data: Option<u32>,
    pub width: u16,
    pub height: u16,
    pub format: u32,
    /// Palette data offset, format, and color count.
    pub palette: Option<(Option<u32>, u32, u16)>,
}

#[derive(Debug)]
pub struct HsdImageSource {
    pub descriptor_id: ImageDescId,
    pub data_offset: Option<u32>,
    pub width: u16,
    pub height: u16,
    pub format: u32,
}

#[derive(Debug)]
pub struct HsdPaletteSource {
    pub descriptor_id: TlutDescId,
    pub data_offset: Option<u32>,
    pub format: u32,
    pub color_count: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HsdSceneError {
    #[error("HSD scene exceeds the {resource} budget of {limit}")]
    LimitExceeded {
        resource: &'static str,
        limit: usize,
    },
    #[error("invalid HSD scene data: {context}")]
    InvalidData { context: &'static str },
    #[error("invalid {resource} index {index}; length is {len}")]
    InvalidReference {
        resource: &'static str,
        index: usize,
        len: usize,
    },
    #[error("unsupported custom TObj TEV descriptor {source_id:#010x}: {error}")]
    UnsupportedCustomTev {
        source_id: u32,
        error: HsdTObjTevEvaluationError,
    },
    #[error("unsupported custom PE descriptor {source_id:#010x}")]
    UnsupportedCustomPe { source_id: u32 },

    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),

    #[error("PObj {polygon:#010x} has a display list that cannot be read: {error}")]
    DisplayList {
        polygon: u32,
        #[source]
        error: crate::gx::display_list::DisplayListError,
    },

    #[error(transparent)]
    ExternalFixup(#[from] crate::DatExternError),

    #[error("instance joint {instance:#010x} target {target:?} is not loaded in this model root")]
    UnresolvedInstanceTarget { instance: u32, target: Option<u32> },

    #[error("repeated source pointer {kind} at {offset:#010x}")]
    RepeatedSourcePointer { kind: DescriptorKind, offset: u32 },
}

impl HsdScene {
    pub fn validate(&self) -> Result<(), HsdSceneError> {
        for root in &self.roots {
            let joint_count = root.joints.len();
            for (joint_index, joint) in root.joints.iter().enumerate() {
                if let Some(parent) = joint.parent
                    && parent.0 >= joint_index
                {
                    return Err(HsdSceneError::InvalidReference {
                        resource: "parent joint",
                        index: parent.0,
                        len: joint_index,
                    });
                }
                if joint.flags & crate::descriptor::jobj::flags::INSTANCE != 0
                    && joint.children.len() != 1
                {
                    return Err(HsdSceneError::InvalidData {
                        context: "INSTANCE requires exactly one referenced joint",
                    });
                }
                for child in &joint.children {
                    if child.0 >= joint_count {
                        return Err(HsdSceneError::InvalidReference {
                            resource: "child joint",
                            index: child.0,
                            len: joint_count,
                        });
                    }
                    if joint.flags & crate::descriptor::jobj::flags::INSTANCE == 0
                        && root.joints[child.0].parent != Some(HsdJointIndex(joint_index))
                    {
                        return Err(HsdSceneError::InvalidData {
                            context: "owned child does not point back to its parent",
                        });
                    }
                }
                for display_object in &joint.display_objects {
                    if let Some(material) = &display_object.material {
                        for texture_object in &material.textures {
                            if let Some(texture) = texture_object.texture
                                && texture.0 >= self.textures.len()
                            {
                                return Err(HsdSceneError::InvalidReference {
                                    resource: "texture",
                                    index: texture.0,
                                    len: self.textures.len(),
                                });
                            }
                        }
                    }
                    for polygon in &display_object.polygons {
                        for triangle in &polygon.decoded.triangles {
                            for vertex_index in triangle {
                                if *vertex_index >= polygon.decoded.vertices.len() {
                                    return Err(HsdSceneError::InvalidReference {
                                        resource: "polygon vertex",
                                        index: *vertex_index,
                                        len: polygon.decoded.vertices.len(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub(super) fn root_has_renderable_geometry(root: &HsdSceneRoot) -> bool {
    root.joints.iter().any(|joint| {
        joint.display_objects.iter().any(|dobj| {
            dobj.polygons
                .iter()
                .any(|pobj| !pobj.decoded.vertices.is_empty() && !pobj.decoded.triangles.is_empty())
        })
    })
}

impl From<crate::descriptor::map_head::MapHeadError> for HsdSceneError {
    fn from(error: crate::descriptor::map_head::MapHeadError) -> Self {
        use crate::descriptor::map_head::MapHeadError;
        match error {
            MapHeadError::Descriptor(error) => Self::Descriptor(error),
            MapHeadError::ExternalFixup(error) => Self::ExternalFixup(error),
            MapHeadError::NullModelGroupTable => Self::InvalidData {
                context: "nonempty stage descriptor table has a null pointer",
            },
            MapHeadError::ModelGroupCountExceedsData => Self::InvalidData {
                context: "stage descriptor count exceeds its data table",
            },
            MapHeadError::NullGeneralPointTable => Self::InvalidData {
                context: "stage general-point table has a count without a pointer",
            },
            MapHeadError::GeneralPointCountExceedsData => Self::InvalidData {
                context: "stage general-point count exceeds its data table",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gx::{GxCompType, GxComponent};

    fn display_object(render_flags: Option<u32>) -> HsdDisplayObject {
        HsdDisplayObject {
            source_id: DObjId(4),
            material: render_flags.map(|render_flags| HsdMaterial {
                source_id: MObjId(16),
                render_flags,
                custom_pe: None,
                colors: None,
                textures: Vec::new(),
            }),
            polygons: Vec::new(),
        }
    }

    #[test]
    fn a_display_object_without_a_material_draws_as_render_mode_zero() {
        let bare = display_object(None);
        assert_eq!(
            bare.pixel_engine(),
            HsdPixelEngineState::from_render_flags(0)
        );
        assert_eq!(bare.pass(), Some(HsdDrawPass::Opaque));
        assert_eq!(bare.channels(), HsdColorChannelState::from_render_flags(0));
    }

    #[test]
    fn enabled_texture_inputs_are_reported_whether_or_not_they_decoded() {
        let polygon = |attr_type| HsdPolygon {
            source_id: PObjId(8),
            flags: 0,
            attributes: [GxAttrName::Tex0, GxAttrName::Tex1, GxAttrName::Tex7]
                .into_iter()
                .map(|attr_name| GxAttribute {
                    attr_name,
                    attr_type,
                    comp_count: 1,
                    comp_type: GxComponent::Number(GxCompType::Float),
                    scale: 0,
                    stride: 8,
                    buffer_ptr: None,
                })
                .collect(),
            primitive_groups: Vec::new(),
            decoded: DecodedPrimitive {
                vertices: Vec::new(),
                triangles: Vec::new(),
            },
            binding: HsdPolygonBinding::Rigid { joint: None },
        };
        assert_eq!(polygon(GxAttrType::Direct).tex_coord_attribute_mask(), 0x83);
        assert_eq!(polygon(GxAttrType::Index8).tex_coord_attribute_mask(), 0x83);
        assert_eq!(polygon(GxAttrType::None).tex_coord_attribute_mask(), 0);
    }

    #[test]
    fn textures_over_the_same_pixels_share_a_content_key() {
        let texture = |descriptor: u32, palette: Option<u32>| HsdTexture {
            id: HsdTextureSourceId {
                image: ImageDescId(descriptor),
                palette: palette.map(TlutDescId),
            },
            image: HsdImageSource {
                descriptor_id: ImageDescId(descriptor),
                data_offset: Some(0x400),
                width: 8,
                height: 8,
                format: 9,
            },
            palette: palette.map(|descriptor| HsdPaletteSource {
                descriptor_id: TlutDescId(descriptor),
                data_offset: Some(descriptor + 0x10),
                format: 1,
                color_count: 16,
            }),
            rgba: None,
        };
        // Two image descriptors over one pixel block and one palette.
        assert_eq!(
            texture(0x20, Some(0x80)).content_key(),
            texture(0x40, Some(0x80)).content_key()
        );
        // The same indices through another palette are other pixels.
        assert_ne!(
            texture(0x20, Some(0x80)).content_key(),
            texture(0x20, Some(0xA0)).content_key()
        );
    }
}
