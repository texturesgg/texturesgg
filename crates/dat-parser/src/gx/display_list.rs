use super::{GxAttrName, GxAttrType, GxCompTypeClr, GxPrimitiveType};
use crate::DatFile;
use crate::descriptor::pobj::GxAttribute;

/// A single primitive group from the display list.
#[derive(Debug, Clone)]
pub struct PrimitiveGroup {
    pub primitive_type: GxPrimitiveType,
    pub vertices: Vec<RawVertex>,
}

/// Raw vertex data as parsed from the display list — indices into attribute buffers.
#[derive(Debug, Clone)]
pub struct RawVertex {
    /// One index per attribute (parallel to the attribute array).
    pub indices: Vec<u16>,
    /// Direct color 0 (RGBA bytes), if attribute is DIRECT.
    pub color0: Option<[u8; 4]>,
    /// Where the display list holds that color: the data-section offset of
    /// its bytes, in the attribute's component format.
    pub color0_offset: Option<u32>,
    /// Direct color 1 (RGBA bytes), if attribute is DIRECT.
    pub color1: Option<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DisplayListLimitExceeded {
    #[error("GX display list exceeds the vertex budget of {limit}")]
    Vertices { limit: usize },
    #[error("GX display list exceeds the primitive-group budget of {limit}")]
    PrimitiveGroups { limit: usize },
    #[error("GX display list exceeds the vertex-attribute decode budget of {limit}")]
    VertexAttributeDecodes { limit: usize },
}

/// Why a display list was not read. Offsets are into the data section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DisplayListError {
    #[error(transparent)]
    Limit(#[from] DisplayListLimitExceeded),
    #[error("GX display list at {offset:#010x} runs past the data section")]
    OutOfBounds { offset: u32 },
    #[error("GX display list byte {opcode:#04x} at {offset:#010x} is not a draw command")]
    UnknownOpcode { offset: u32, opcode: u8 },
    #[error("GX display list ends inside the primitive group at {offset:#010x}")]
    Truncated { offset: u32 },
    #[error("vertex attribute {name:?} is direct; only colors and matrix indices are read direct")]
    UnsupportedDirectAttribute { name: GxAttrName },
    #[error("direct color component format {format} is not a GX color format")]
    UnknownColorFormat { format: u32 },
}

/// How much one call may expand a display list into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayListLimits {
    pub vertices: usize,
    pub primitive_groups: usize,
    pub vertex_attribute_decodes: usize,
}

/// Parse the primitive groups of a display list, within `limits`.
///
/// The list is a run of draw commands padded with NOPs (`pobj.c` hands it to
/// `GXCallDisplayList` whole). Anything else in it, or a vertex layout this
/// reader does not cover, is an error: skipping a byte would misread every
/// vertex after it.
pub fn parse_display_list(
    dat: &DatFile,
    dl_offset: u32,
    dl_size: usize,
    attributes: &[GxAttribute],
    limits: DisplayListLimits,
) -> Result<Vec<PrimitiveGroup>, DisplayListError> {
    let dl_data = dat
        .data_slice(dl_offset, dl_size)
        .ok_or(DisplayListError::OutOfBounds { offset: dl_offset })?;
    // `data_slice` checked the whole range, so an offset into it fits a u32.
    let at = |pos: usize| dl_offset + pos as u32;

    let mut groups = Vec::new();
    let mut total_vertices = 0usize;
    let mut total_vertex_attribute_decodes = 0usize;
    let mut pos = 0usize;

    while pos < dl_data.len() {
        let group_start = pos;
        let opcode = dl_data[pos];
        pos += 1;

        if opcode == 0 {
            // GX_NOP pads the list to its 32-byte length.
            continue;
        }

        let prim_type =
            GxPrimitiveType::from_byte(opcode).ok_or(DisplayListError::UnknownOpcode {
                offset: at(group_start),
                opcode,
            })?;
        let truncated = DisplayListError::Truncated {
            offset: at(group_start),
        };

        let vertex_count = dl_data
            .get(pos..pos + 2)
            .map(|count| usize::from(u16::from_be_bytes([count[0], count[1]])))
            .ok_or(truncated)?;
        pos += 2;
        if groups.len() >= limits.primitive_groups {
            return Err(DisplayListLimitExceeded::PrimitiveGroups {
                limit: limits.primitive_groups,
            }
            .into());
        }
        total_vertices = total_vertices
            .checked_add(vertex_count)
            .filter(|total| *total <= limits.vertices)
            .ok_or(DisplayListLimitExceeded::Vertices {
                limit: limits.vertices,
            })?;
        let attribute_decodes = vertex_count.checked_mul(attributes.len()).ok_or(
            DisplayListLimitExceeded::VertexAttributeDecodes {
                limit: limits.vertex_attribute_decodes,
            },
        )?;
        total_vertex_attribute_decodes = total_vertex_attribute_decodes
            .checked_add(attribute_decodes)
            .filter(|total| *total <= limits.vertex_attribute_decodes)
            .ok_or(DisplayListLimitExceeded::VertexAttributeDecodes {
                limit: limits.vertex_attribute_decodes,
            })?;

        let mut vertices = Vec::with_capacity(vertex_count);

        for _ in 0..vertex_count {
            let mut indices = vec![0u16; attributes.len()];
            let mut color0 = None;
            let mut color0_offset = None;
            let mut color1 = None;

            for (i, attr) in attributes.iter().enumerate() {
                if attr.attr_name == GxAttrName::Null {
                    continue;
                }

                match attr.attr_type {
                    GxAttrType::Direct => match attr.attr_name {
                        GxAttrName::Color0 | GxAttrName::Color1 => {
                            let format = GxCompTypeClr::from_u32(attr.comp_type_raw).ok_or(
                                DisplayListError::UnknownColorFormat {
                                    format: attr.comp_type_raw,
                                },
                            )?;
                            let start = pos;
                            let color =
                                read_direct_color(dl_data, &mut pos, format).ok_or(truncated)?;
                            if attr.attr_name == GxAttrName::Color0 {
                                color0 = Some(color);
                                color0_offset = Some(at(start));
                            } else {
                                color1 = Some(color);
                            }
                        }
                        // A direct matrix index is one byte.
                        name if name.is_matrix_index() => {
                            indices[i] = u16::from(*dl_data.get(pos).ok_or(truncated)?);
                            pos += 1;
                        }
                        name => return Err(DisplayListError::UnsupportedDirectAttribute { name }),
                    },
                    GxAttrType::Index8 => {
                        indices[i] = u16::from(*dl_data.get(pos).ok_or(truncated)?);
                        pos += 1;
                    }
                    GxAttrType::Index16 => {
                        let index = dl_data.get(pos..pos + 2).ok_or(truncated)?;
                        indices[i] = u16::from_be_bytes([index[0], index[1]]);
                        pos += 2;
                    }
                    GxAttrType::None => {}
                }
            }

            vertices.push(RawVertex {
                indices,
                color0,
                color0_offset,
                color1,
            });
        }

        groups.push(PrimitiveGroup {
            primitive_type: prim_type,
            vertices,
        });
    }

    Ok(groups)
}

/// A color in a direct color attribute's component format: the bytes a
/// display list holds for it. The formats keep fewer bits than RGBA8, so
/// reading the bytes back gives the nearest color the format has. RGB565
/// and RGB8 have no alpha, and RGBX8 keeps the alpha byte as its padding.
pub fn encode_direct_color(format: GxCompTypeClr, rgba: [u8; 4]) -> Vec<u8> {
    let [r, g, b, a] = rgba.map(u32::from);
    match format {
        GxCompTypeClr::Rgb565 => (((r >> 3) << 11 | (g >> 2) << 5 | b >> 3) as u16)
            .to_be_bytes()
            .to_vec(),
        GxCompTypeClr::Rgb8 => rgba[..3].to_vec(),
        GxCompTypeClr::Rgbx8 | GxCompTypeClr::Rgba8 => rgba.to_vec(),
        GxCompTypeClr::Rgba4 => {
            // Decoding multiplies a nibble by 17; rounding picks the nearest.
            let nibble = |value: u32| (value * 15 + 127) / 255;
            ((nibble(r) << 12 | nibble(g) << 8 | nibble(b) << 4 | nibble(a)) as u16)
                .to_be_bytes()
                .to_vec()
        }
        GxCompTypeClr::Rgba6 => {
            let packed = (r >> 2) << 18 | (g >> 2) << 12 | (b >> 2) << 6 | a >> 2;
            packed.to_be_bytes()[1..].to_vec()
        }
    }
}

/// The color `bytes` hold in a direct color attribute's component format;
/// `None` when there are too few bytes for one.
pub fn decode_direct_color(format: GxCompTypeClr, bytes: &[u8]) -> Option<[u8; 4]> {
    read_direct_color(bytes, &mut 0, format)
}

/// Read a direct GX color from the display list stream and step past it;
/// `None`, with `pos` unmoved, when the stream ends inside it.
fn read_direct_color(data: &[u8], pos: &mut usize, format: GxCompTypeClr) -> Option<[u8; 4]> {
    let bytes = data.get(*pos..pos.checked_add(format.byte_len())?)?;
    *pos += bytes.len();
    Some(match format {
        GxCompTypeClr::Rgb565 => {
            let b = u16::from_be_bytes([bytes[0], bytes[1]]);
            [
                (((b >> 11) & 0x1F) << 3) as u8,
                (((b >> 5) & 0x3F) << 2) as u8,
                ((b & 0x1F) << 3) as u8,
                255,
            ]
        }
        GxCompTypeClr::Rgb8 => [bytes[0], bytes[1], bytes[2], 255],
        GxCompTypeClr::Rgbx8 | GxCompTypeClr::Rgba8 => [bytes[0], bytes[1], bytes[2], bytes[3]],
        GxCompTypeClr::Rgba4 => {
            let b = u16::from_be_bytes([bytes[0], bytes[1]]);
            [
                (((b >> 12) & 0xF) * 17) as u8,
                (((b >> 8) & 0xF) * 17) as u8,
                (((b >> 4) & 0xF) * 17) as u8,
                ((b & 0xF) * 17) as u8,
            ]
        }
        GxCompTypeClr::Rgba6 => {
            let p = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
            [
                (((p >> 18) & 0x3F) << 2) as u8,
                (((p >> 12) & 0x3F) << 2) as u8,
                (((p >> 6) & 0x3F) << 2) as u8,
                ((p & 0x3F) << 2) as u8,
            ]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dat_with_data(data: Vec<u8>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), Vec::new())
    }

    #[test]
    fn rejects_claimed_vertices_before_allocating_them() {
        let dat = dat_with_data(vec![0, 0x90, 0xFF, 0xFF]);

        assert_eq!(
            parse_display_list(&dat, 1, 3, &[], limits(10, 10, 10)).unwrap_err(),
            DisplayListLimitExceeded::Vertices { limit: 10 }.into()
        );
    }

    #[test]
    fn rejects_zero_vertex_groups_before_growing_the_group_vector() {
        let dat = dat_with_data(vec![0, 0x90, 0, 0, 0x90, 0, 0]);

        assert_eq!(
            parse_display_list(&dat, 1, 6, &[], limits(10, 1, 10)).unwrap_err(),
            DisplayListLimitExceeded::PrimitiveGroups { limit: 1 }.into()
        );
    }

    #[test]
    fn rejects_vertex_attribute_work_before_allocating_raw_vertices() {
        let dat = dat_with_data(vec![0, 0x90, 0, 2]);
        let attributes = vec![GxAttribute {
            attr_name: GxAttrName::Position,
            attr_type: GxAttrType::Index8,
            comp_count: 1,
            comp_type: super::super::GxCompType::UInt8,
            scale: 0,
            stride: 1,
            buffer_ptr: Some(0),
            comp_type_raw: 0,
        }];

        assert_eq!(
            parse_display_list(&dat, 1, 3, &attributes, limits(10, 10, 1)).unwrap_err(),
            DisplayListLimitExceeded::VertexAttributeDecodes { limit: 1 }.into()
        );
    }

    #[test]
    fn a_list_it_cannot_read_exactly_is_an_error_not_a_guess() {
        let position = |attr_type| GxAttribute {
            attr_name: GxAttrName::Position,
            attr_type,
            comp_count: 1,
            comp_type: super::super::GxCompType::Float,
            scale: 0,
            stride: 12,
            buffer_ptr: Some(0),
            comp_type_raw: 4,
        };
        let parse = |data: Vec<u8>, attributes: &[GxAttribute]| {
            let size = data.len();
            parse_display_list(
                &dat_with_data(data),
                0,
                size,
                attributes,
                limits(10, 10, 10),
            )
        };

        // A byte that is neither a NOP nor a draw command.
        assert_eq!(
            parse(vec![0, 0x61, 0, 0], &[]).unwrap_err(),
            DisplayListError::UnknownOpcode {
                offset: 1,
                opcode: 0x61
            }
        );
        // A group that ends before its count, and one that ends before its
        // last vertex.
        assert_eq!(
            parse(vec![0x90, 0], &[]).unwrap_err(),
            DisplayListError::Truncated { offset: 0 }
        );
        assert_eq!(
            parse(vec![0x90, 0, 2, 7], &[position(GxAttrType::Index8)]).unwrap_err(),
            DisplayListError::Truncated { offset: 0 }
        );
        // A direct position is twelve bytes this reader has nowhere to put.
        assert_eq!(
            parse(
                vec![0x90, 0, 1, 0, 0, 0, 0],
                &[position(GxAttrType::Direct)]
            )
            .unwrap_err(),
            DisplayListError::UnsupportedDirectAttribute {
                name: GxAttrName::Position
            }
        );
        assert_eq!(
            parse(vec![0x90, 0, 1, 0], &[direct_color(6)]).unwrap_err(),
            DisplayListError::UnknownColorFormat { format: 6 }
        );
        // A list outside the data section.
        assert_eq!(
            parse_display_list(&dat_with_data(vec![0; 4]), 2, 4, &[], limits(10, 10, 10))
                .unwrap_err(),
            DisplayListError::OutOfBounds { offset: 2 }
        );
    }

    fn limits(
        vertices: usize,
        primitive_groups: usize,
        vertex_attribute_decodes: usize,
    ) -> DisplayListLimits {
        DisplayListLimits {
            vertices,
            primitive_groups,
            vertex_attribute_decodes,
        }
    }

    fn direct_color(format: u32) -> GxAttribute {
        GxAttribute {
            attr_name: GxAttrName::Color0,
            attr_type: GxAttrType::Direct,
            comp_count: 1,
            comp_type: super::super::GxCompType::UInt8,
            scale: 0,
            stride: 0,
            buffer_ptr: None,
            comp_type_raw: format,
        }
    }

    #[test]
    fn direct_colors_record_where_the_display_list_holds_them() {
        // Pokemon Stadium's platform green, 0x670C, then its arrow yellow.
        let dat = dat_with_data(vec![0xAA, 0x90, 0, 2, 0x67, 0x0C, 0xFF, 0xE6]);
        let groups =
            parse_display_list(&dat, 1, 7, &[direct_color(0)], limits(10, 10, 10)).unwrap();
        let vertices = &groups[0].vertices;
        assert_eq!(vertices[0].color0, Some([96, 224, 96, 255]));
        assert_eq!(vertices[0].color0_offset, Some(4));
        assert_eq!(vertices[1].color0_offset, Some(6));
    }

    #[test]
    fn an_encoded_color_reads_back_as_the_formats_nearest() {
        for format in 0..6 {
            let kind = GxCompTypeClr::from_u32(format).unwrap();
            for rgba in [
                [0, 0, 0, 0],
                [255; 4],
                [96, 224, 96, 255],
                [13, 130, 201, 77],
            ] {
                let bytes = encode_direct_color(kind, rgba);
                assert_eq!(bytes.len(), kind.byte_len());
                let read = decode_direct_color(kind, &bytes).unwrap();
                // What was read encodes to the same bytes: it is a color the
                // format holds exactly.
                assert_eq!(encode_direct_color(kind, read), bytes, "{kind:?} {rgba:?}");
                // And it is within the format's step of what was asked.
                let alpha = !matches!(kind, GxCompTypeClr::Rgb565 | GxCompTypeClr::Rgb8);
                for channel in 0..if alpha { 4 } else { 3 } {
                    let error = read[channel].abs_diff(rgba[channel]);
                    assert!(error <= 9, "{kind:?} {rgba:?} read {read:?}");
                }
            }
        }
        // Decoding 0x670C gives (96, 224, 96): encoding returns the same bits.
        assert_eq!(
            encode_direct_color(GxCompTypeClr::Rgb565, [96, 224, 96, 255]),
            [0x67, 0x0C]
        );
    }
}
