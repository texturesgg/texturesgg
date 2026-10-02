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
    /// its bytes, in the attribute's component format. `None` when the list
    /// ended before the whole color.
    pub color0_offset: Option<u32>,
    /// Direct color 1 (RGBA bytes), if attribute is DIRECT.
    pub color1: Option<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DisplayListLimitExceeded {
    #[error("GX display list exceeds the vertex budget of {limit}")]
    Vertices { limit: usize },
    #[error("GX display list exceeds the primitive-group budget of {limit}")]
    PrimitiveGroups { limit: usize },
    #[error("GX display list exceeds the vertex-attribute decode budget of {limit}")]
    VertexAttributeDecodes { limit: usize },
}

/// Parse all primitive groups from a display list buffer.
pub fn parse_display_list(
    dat: &DatFile,
    dl_offset: u32,
    dl_size: usize,
    attributes: &[GxAttribute],
) -> Vec<PrimitiveGroup> {
    parse_display_list_limited(
        dat,
        dl_offset,
        dl_size,
        attributes,
        usize::MAX,
        usize::MAX,
        usize::MAX,
    )
    .unwrap_or_default()
}

/// Parse primitive groups while bounding attacker-controlled vertex expansion.
pub fn parse_display_list_limited(
    dat: &DatFile,
    dl_offset: u32,
    dl_size: usize,
    attributes: &[GxAttribute],
    max_vertices: usize,
    max_primitive_groups: usize,
    max_vertex_attribute_decodes: usize,
) -> Result<Vec<PrimitiveGroup>, DisplayListLimitExceeded> {
    let dl_data = match dat.data_slice(dl_offset, dl_size) {
        Some(data) => data,
        None => return Ok(Vec::new()),
    };

    let mut groups = Vec::new();
    let mut total_vertices = 0usize;
    let mut total_vertex_attribute_decodes = 0usize;
    let mut pos = 0usize;

    while pos < dl_data.len() {
        let opcode = dl_data[pos];
        pos += 1;

        if opcode == 0 {
            // NOP / end of display list
            continue;
        }

        let prim_type = match GxPrimitiveType::from_byte(opcode) {
            Some(pt) => pt,
            None => continue, // Skip unknown opcodes
        };

        if pos + 2 > dl_data.len() {
            break;
        }
        let vertex_count = u16::from_be_bytes([dl_data[pos], dl_data[pos + 1]]) as usize;
        pos += 2;
        if groups.len() >= max_primitive_groups {
            return Err(DisplayListLimitExceeded::PrimitiveGroups {
                limit: max_primitive_groups,
            });
        }
        total_vertices = total_vertices
            .checked_add(vertex_count)
            .filter(|total| *total <= max_vertices)
            .ok_or(DisplayListLimitExceeded::Vertices {
                limit: max_vertices,
            })?;
        let attribute_decodes = vertex_count.checked_mul(attributes.len()).ok_or(
            DisplayListLimitExceeded::VertexAttributeDecodes {
                limit: max_vertex_attribute_decodes,
            },
        )?;
        total_vertex_attribute_decodes = total_vertex_attribute_decodes
            .checked_add(attribute_decodes)
            .filter(|total| *total <= max_vertex_attribute_decodes)
            .ok_or(DisplayListLimitExceeded::VertexAttributeDecodes {
                limit: max_vertex_attribute_decodes,
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
                    GxAttrType::Direct => {
                        if attr.attr_name == GxAttrName::Color0
                            || attr.attr_name == GxAttrName::Color1
                        {
                            // Read direct color from display list stream
                            let start = pos;
                            let clr = read_direct_color(dl_data, &mut pos, attr.comp_type_raw);
                            if attr.attr_name == GxAttrName::Color0 {
                                color0 = Some(clr);
                                let whole = GxCompTypeClr::from_u32(attr.comp_type_raw)
                                    .is_some_and(|format| pos - start == format.byte_len());
                                color0_offset = whole.then(|| dl_offset + start as u32);
                            } else {
                                color1 = Some(clr);
                            }
                        } else {
                            // Direct non-color: single byte (matrix indices etc.)
                            if pos < dl_data.len() {
                                indices[i] = dl_data[pos] as u16;
                                pos += 1;
                            }
                        }
                    }
                    GxAttrType::Index8 => {
                        if pos < dl_data.len() {
                            indices[i] = dl_data[pos] as u16;
                            pos += 1;
                        }
                    }
                    GxAttrType::Index16 => {
                        if pos + 2 <= dl_data.len() {
                            indices[i] = u16::from_be_bytes([dl_data[pos], dl_data[pos + 1]]);
                            pos += 2;
                        }
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
    let mut pos = 0;
    let color = read_direct_color(bytes, &mut pos, format as u32);
    (pos == format.byte_len()).then_some(color)
}

/// Read a direct GX color value from the display list stream.
fn read_direct_color(data: &[u8], pos: &mut usize, comp_type_raw: u32) -> [u8; 4] {
    let mut clr = [255u8; 4];

    match GxCompTypeClr::from_u32(comp_type_raw) {
        Some(GxCompTypeClr::Rgb565) => {
            if *pos + 2 <= data.len() {
                let b = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
                *pos += 2;
                clr[0] = (((b >> 11) & 0x1F) << 3) as u8;
                clr[1] = (((b >> 5) & 0x3F) << 2) as u8;
                clr[2] = ((b & 0x1F) << 3) as u8;
                clr[3] = 255;
            }
        }
        Some(GxCompTypeClr::Rgb8) => {
            if *pos + 3 <= data.len() {
                clr[0] = data[*pos];
                clr[1] = data[*pos + 1];
                clr[2] = data[*pos + 2];
                clr[3] = 255;
                *pos += 3;
            }
        }
        Some(GxCompTypeClr::Rgbx8) | Some(GxCompTypeClr::Rgba8) => {
            if *pos + 4 <= data.len() {
                clr[0] = data[*pos];
                clr[1] = data[*pos + 1];
                clr[2] = data[*pos + 2];
                clr[3] = data[*pos + 3];
                *pos += 4;
            }
        }
        Some(GxCompTypeClr::Rgba4) => {
            if *pos + 2 <= data.len() {
                let b = u16::from_be_bytes([data[*pos], data[*pos + 1]]);
                *pos += 2;
                clr[0] = (((b >> 12) & 0xF) * 17) as u8;
                clr[1] = (((b >> 8) & 0xF) * 17) as u8;
                clr[2] = (((b >> 4) & 0xF) * 17) as u8;
                clr[3] = ((b & 0xF) * 17) as u8;
            }
        }
        Some(GxCompTypeClr::Rgba6) => {
            if *pos + 3 <= data.len() {
                let p = (data[*pos] as u32) << 16
                    | (data[*pos + 1] as u32) << 8
                    | data[*pos + 2] as u32;
                *pos += 3;
                clr[0] = (((p >> 18) & 0x3F) << 2) as u8;
                clr[1] = (((p >> 12) & 0x3F) << 2) as u8;
                clr[2] = (((p >> 6) & 0x3F) << 2) as u8;
                clr[3] = ((p & 0x3F) << 2) as u8;
            }
        }
        None => {
            // Unknown color format, skip a byte
            *pos += 1;
        }
    }

    clr
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
            parse_display_list_limited(&dat, 1, 3, &[], 10, 10, 10).unwrap_err(),
            DisplayListLimitExceeded::Vertices { limit: 10 }
        );
    }

    #[test]
    fn rejects_zero_vertex_groups_before_growing_the_group_vector() {
        let dat = dat_with_data(vec![0, 0x90, 0, 0, 0x90, 0, 0]);

        assert_eq!(
            parse_display_list_limited(&dat, 1, 6, &[], 10, 1, 10).unwrap_err(),
            DisplayListLimitExceeded::PrimitiveGroups { limit: 1 }
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
            parse_display_list_limited(&dat, 1, 3, &attributes, 10, 10, 1).unwrap_err(),
            DisplayListLimitExceeded::VertexAttributeDecodes { limit: 1 }
        );
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
        let groups = parse_display_list(&dat, 1, 7, &[direct_color(0)]);
        let vertices = &groups[0].vertices;
        assert_eq!(vertices[0].color0, Some([96, 224, 96, 255]));
        assert_eq!(vertices[0].color0_offset, Some(4));
        assert_eq!(vertices[1].color0_offset, Some(6));

        // A list that ends inside a color has no place to write one.
        let groups = parse_display_list(&dat, 1, 6, &[direct_color(0)]);
        assert_eq!(groups[0].vertices[1].color0_offset, None);
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
                let mut pos = 0;
                let read = read_direct_color(&bytes, &mut pos, format);
                assert_eq!(pos, bytes.len());
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
