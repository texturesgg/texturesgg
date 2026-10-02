use super::GxAttrName;
use super::display_list::{PrimitiveGroup, RawVertex};
use crate::DatFile;
use crate::descriptor::pobj::GxAttribute;

/// Decoded vertex with all possible attributes.
#[derive(Debug, Clone, Default)]
pub struct DecodedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub binormal: [f32; 3],
    pub tangent: [f32; 3],
    /// True only when this vertex's indexed GX NBT entry decoded completely.
    pub has_nbt: bool,
    /// GX's TEX0 through TEX7.
    pub tex_coords: [[f32; 2]; 8],
    /// Bit `n` marks a TEX`n` attribute that actually decoded, not a default zero.
    pub tex_coord_mask: u8,
    pub color0: [f32; 4],
    /// GX matrix-palette selector. Envelope geometry resolves it as `/ 3`;
    /// it is not directly a modern bone-array index.
    pub pn_mtx_idx: u16,
}

/// A triangle (3 vertex indices into the decoded vertex array).
pub type Triangle = [usize; 3];

/// Decoded mesh data from a single PObj.
#[derive(Debug, Clone)]
pub struct DecodedPrimitive {
    pub vertices: Vec<DecodedVertex>,
    pub triangles: Vec<Triangle>,
}

/// Decode all primitive groups from a PObj into triangulated vertex data.
pub fn decode_primitives(
    dat: &DatFile,
    attributes: &[GxAttribute],
    primitive_groups: &[PrimitiveGroup],
) -> DecodedPrimitive {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();

    for group in primitive_groups {
        let base_idx = vertices.len();

        // Decode each vertex
        for raw_vtx in &group.vertices {
            let vtx = decode_vertex(dat, attributes, raw_vtx);
            vertices.push(vtx);
        }

        // Triangulate based on primitive type
        let count = group.vertices.len();
        match group.primitive_type {
            super::GxPrimitiveType::Triangles => {
                for i in (0..count).step_by(3) {
                    if i + 2 < count {
                        triangles.push([base_idx + i, base_idx + i + 1, base_idx + i + 2]);
                    }
                }
            }
            super::GxPrimitiveType::TriangleStrip => {
                for i in 0..count.saturating_sub(2) {
                    if i % 2 == 0 {
                        triangles.push([base_idx + i, base_idx + i + 1, base_idx + i + 2]);
                    } else {
                        // Flip winding for odd triangles
                        triangles.push([base_idx + i + 1, base_idx + i, base_idx + i + 2]);
                    }
                }
            }
            super::GxPrimitiveType::TriangleFan => {
                for i in 1..count.saturating_sub(1) {
                    triangles.push([base_idx, base_idx + i, base_idx + i + 1]);
                }
            }
            super::GxPrimitiveType::Quads => {
                for i in (0..count).step_by(4) {
                    if i + 3 < count {
                        triangles.push([base_idx + i, base_idx + i + 1, base_idx + i + 2]);
                        triangles.push([base_idx + i, base_idx + i + 2, base_idx + i + 3]);
                    }
                }
            }
            _ => {} // Lines, points — skip for mesh rendering
        }
    }

    DecodedPrimitive {
        vertices,
        triangles,
    }
}

fn decode_vertex(dat: &DatFile, attributes: &[GxAttribute], raw: &RawVertex) -> DecodedVertex {
    let mut vtx = DecodedVertex {
        color0: [1.0, 1.0, 1.0, 1.0], // Default white
        ..Default::default()
    };

    for (i, attr) in attributes.iter().enumerate() {
        let index = raw.indices[i];

        match attr.attr_name {
            GxAttrName::PnMtxIdx => {
                vtx.pn_mtx_idx = index;
            }
            GxAttrName::Position => {
                if attr.attr_type != super::GxAttrType::Direct {
                    let f = attr.decode_at(dat, index);
                    if f.len() >= 3 {
                        vtx.position = [f[0], f[1], f[2]];
                    } else if f.len() >= 2 {
                        vtx.position = [f[0], f[1], 0.0];
                    }
                }
            }
            GxAttrName::Normal => {
                if attr.attr_type != super::GxAttrType::Direct {
                    let f = attr.decode_at(dat, index);
                    if f.len() >= 3 {
                        vtx.normal = [f[0], f[1], f[2]];
                    }
                }
            }
            GxAttrName::Nbt => {
                if let Some([normal, binormal, tangent]) = attr.decode_nbt_at(dat, index) {
                    vtx.normal = normal;
                    vtx.binormal = binormal;
                    vtx.tangent = tangent;
                    vtx.has_nbt = true;
                }
            }
            name if name.is_tex_coord() => {
                if matches!(
                    attr.attr_type,
                    super::GxAttrType::Index8 | super::GxAttrType::Index16
                ) {
                    let f = attr.decode_at(dat, index);
                    if f.len() >= 2 {
                        let set = name as usize - GxAttrName::Tex0 as usize;
                        vtx.tex_coords[set] = [f[0], f[1]];
                        vtx.tex_coord_mask |= 1 << set;
                    }
                }
            }
            GxAttrName::Color0 => {
                if let Some(clr) = &raw.color0 {
                    vtx.color0 = [
                        clr[0] as f32 / 255.0,
                        clr[1] as f32 / 255.0,
                        clr[2] as f32 / 255.0,
                        clr[3] as f32 / 255.0,
                    ];
                } else if attr.attr_type != super::GxAttrType::Direct {
                    let f = attr.decode_at(dat, index);
                    if f.len() >= 4 {
                        vtx.color0 = [f[0], f[1], f[2], f[3]];
                    }
                }
            }
            _ => {} // CLR1, matrix indices — not decoded
        }
    }

    vtx
}
