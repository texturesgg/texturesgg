use super::{DatFile, DescriptorParseError, DescriptorReader};
use crate::gx::{GxAttrName, GxAttrType, GxCompType, GxCompTypeClr};

/// GX exposes a small fixed attribute set; this higher guard prevents corrupt
/// unterminated arrays from walking the rest of the DAT as descriptors.
pub const MAX_GX_ATTRIBUTES: usize = 64;

/// Source-defined `HSD_PObjDesc.flags` type and culling fields.
pub mod flags {
    pub const TYPE_MASK: u16 = 0x3000;
    pub const SKIN: u16 = 0 << 12;
    pub const SHAPEANIM: u16 = 1 << 12;
    pub const ENVELOPE: u16 = 2 << 12;
    pub const RESERVED_TYPE: u16 = 3 << 12;

    // HSD_PObjDisp passes these directly to GXSetCullMode. GX considers
    // clockwise triangles front-facing. Pinned HSDLib reverses these names.
    pub const CULLFRONT: u16 = 1 << 14;
    pub const CULLBACK: u16 = 1 << 15;
    pub const CULL_MASK: u16 = CULLFRONT | CULLBACK;

    pub const fn pobj_type(flags: u16) -> u16 {
        flags & TYPE_MASK
    }
}

/// GX Vertex Attribute Descriptor — parsed from the attribute array.
///
/// Layout (0x18 bytes):
///   0x00: attr_name (u32) — GXAttribName enum
///   0x04: attr_type (u32) — GXAttribType (NONE/DIRECT/INDEX8/INDEX16)
///   0x08: comp_count (u32) — GXCompCnt (number of components)
///   0x0C: comp_type (u32) — GXCompType (UInt8/Int8/UInt16/Int16/Float)
///   0x10: scale (u8) — fractional bits (divide decoded value by 2^scale)
///   0x12: stride (u16) — bytes between elements in the buffer
///   0x14: buffer_ptr (u32) — pointer to vertex attribute data buffer
#[derive(Debug, Clone)]
pub struct GxAttribute {
    pub attr_name: GxAttrName,
    pub attr_type: GxAttrType,
    pub comp_count: u32,
    /// The component type of a position, normal or texture coordinate. A
    /// color's is a [`GxCompTypeClr`] in `comp_type_raw`,
    /// and this is `UInt8`.
    pub comp_type: GxCompType,
    pub scale: u8,
    pub stride: u16,
    pub buffer_ptr: Option<u32>,
    /// Raw comp_type value (needed for color format detection).
    pub comp_type_raw: u32,
}

impl GxAttribute {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Option<Self>, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "GxAttribute", offset);
        let attr_name_raw = source.u32(0x00)?;

        if attr_name_raw == GxAttrName::Null as u32 {
            return Ok(None); // End of attribute list
        }
        let source = source.require_extent(0x18)?;
        let attr_name = GxAttrName::from_u32(attr_name_raw).ok_or(source.invalid_value(
            "attr_name",
            0x00,
            attr_name_raw,
        ))?;

        let attr_type_raw = source.u32(0x04)?;
        let attr_type = GxAttrType::from_u32(attr_type_raw).ok_or(source.invalid_value(
            "attr_type",
            0x04,
            attr_type_raw,
        ))?;
        let comp_count = source.u32(0x08)?;
        let comp_type_raw = source.u32(0x0C)?;
        let invalid_comp_type = source.invalid_value("comp_type", 0x0C, comp_type_raw);
        let comp_type = if matches!(attr_name, GxAttrName::Color0 | GxAttrName::Color1) {
            GxCompTypeClr::from_u32(comp_type_raw).ok_or(invalid_comp_type)?;
            GxCompType::UInt8
        } else {
            GxCompType::from_u32(comp_type_raw).ok_or(invalid_comp_type)?
        };
        let scale = source.u8(0x10)?;
        let stride = source.u16(0x12)?;
        let buffer_ptr = source.pointer("buffer", 0x14)?;

        Ok(Some(Self {
            attr_name,
            attr_type,
            comp_count,
            comp_type,
            scale,
            stride,
            buffer_ptr,
            comp_type_raw,
        }))
    }

    /// Number of components one entry of this attribute holds, which GX
    /// takes from the attribute and its component count (`GXSetVtxAttrFmt`),
    /// never from the stride. Zero for a pair this decoder does not read. A
    /// normal with binormal and tangent leads with its three normal components.
    pub fn component_count(&self) -> usize {
        match (self.attr_name, self.comp_count) {
            // GX_POS_XY, GX_POS_XYZ.
            (GxAttrName::Position, 0) => 2,
            (GxAttrName::Position, 1) => 3,
            // GX_NRM_XYZ, GX_NRM_NBT, GX_NRM_NBT3.
            (GxAttrName::Normal, 0..=2) => 3,
            // GX_TEX_S, GX_TEX_ST.
            (name, 0) if name.is_tex_coord() => 1,
            (name, 1) if name.is_tex_coord() => 2,
            _ => 0,
        }
    }

    /// Decode data at a given index from the buffer.
    /// Returns decoded float array.
    pub fn decode_at(&self, dat: &DatFile, index: u16) -> Vec<f32> {
        let buffer_offset = match self.buffer_ptr {
            Some(p) => p,
            None => return Vec::new(),
        };

        let Some(data_offset) = (index as u32)
            .checked_mul(self.stride as u32)
            .and_then(|relative| buffer_offset.checked_add(relative))
        else {
            return Vec::new();
        };
        // Color attributes use a different type space
        if self.attr_name == GxAttrName::Color0 || self.attr_name == GxAttrName::Color1 {
            return self.decode_color_at(dat, data_offset);
        }

        let size = self.component_count();
        // GX applies the fraction to integer components only.
        let scale_divisor = if self.comp_type == GxCompType::Float {
            1.0
        } else {
            match 1u32.checked_shl(self.scale as u32) {
                Some(value) => value as f32,
                None => return Vec::new(),
            }
        };

        let bytes_per_component = self.comp_type.byte_len();
        let Some(byte_len) = size.checked_mul(bytes_per_component) else {
            return Vec::new();
        };
        if dat.data_slice(data_offset, byte_len).is_none() {
            return Vec::new();
        }

        let mut result = Vec::with_capacity(size);
        for i in 0..size {
            let val = match self.comp_type {
                GxCompType::UInt8 => dat.read_u8(data_offset + i as u32).unwrap_or(0) as f32,
                GxCompType::Int8 => dat.read_u8(data_offset + i as u32).unwrap_or(0) as i8 as f32,
                GxCompType::UInt16 => dat.read_u16(data_offset + i as u32 * 2).unwrap_or(0) as f32,
                GxCompType::Int16 => {
                    dat.read_u16(data_offset + i as u32 * 2).unwrap_or(0) as i16 as f32
                }
                GxCompType::Float => dat.read_f32(data_offset + i as u32 * 4).unwrap_or(0.0),
            };
            result.push(val / scale_divisor);
        }
        result
    }

    /// Decode an indexed GX NBT entry as source-ordered normal, binormal, and
    /// tangent vectors. `GX_NRM_NBT3` stores nine scalar components even though
    /// its component-count enum is `1`.
    pub fn decode_nbt_at(&self, dat: &DatFile, index: u16) -> Option<[[f32; 3]; 3]> {
        if self.attr_name != GxAttrName::Nbt
            || self.attr_type == GxAttrType::Direct
            || self.comp_count != 1
        {
            return None;
        }
        let bytes_per_component = match self.comp_type {
            GxCompType::UInt8 | GxCompType::Int8 => 1usize,
            GxCompType::UInt16 | GxCompType::Int16 => 2,
            GxCompType::Float => 4,
        };
        let byte_len = 9usize.checked_mul(bytes_per_component)?;
        if usize::from(self.stride) < byte_len {
            return None;
        }
        let offset = self
            .buffer_ptr?
            .checked_add(u32::from(index).checked_mul(u32::from(self.stride))?)?;
        dat.data_slice(offset, byte_len)?;
        let divisor = 1u32.checked_shl(u32::from(self.scale))? as f32;
        let mut values = [0.0; 9];
        for (component, value) in values.iter_mut().enumerate() {
            let relative = component.checked_mul(bytes_per_component)?;
            let component_offset = offset.checked_add(u32::try_from(relative).ok()?)?;
            let raw = match self.comp_type {
                GxCompType::UInt8 => f32::from(dat.read_u8(component_offset)?),
                GxCompType::Int8 => f32::from(dat.read_u8(component_offset)? as i8),
                GxCompType::UInt16 => f32::from(dat.read_u16(component_offset)?),
                GxCompType::Int16 => f32::from(dat.read_u16(component_offset)? as i16),
                GxCompType::Float => dat.read_f32(component_offset)?,
            };
            *value = raw / divisor;
        }
        Some([
            [values[0], values[1], values[2]],
            [values[3], values[4], values[5]],
            [values[6], values[7], values[8]],
        ])
    }

    fn decode_color_at(&self, dat: &DatFile, offset: u32) -> Vec<f32> {
        let mut c = vec![1.0f32; 4]; // Default white, full alpha
        let byte_len = match self.comp_type_raw {
            0 | 3 => 2,
            1 | 4 => 3,
            2 | 5 => 4,
            _ => return c,
        };
        if dat.data_slice(offset, byte_len).is_none() {
            return c;
        }

        match self.comp_type_raw {
            0 => {
                // RGB565
                let pixel = dat.read_u16(offset).unwrap_or(0);
                c[0] = (((pixel >> 11) & 0x1F) << 3) as f32 / 255.0;
                c[1] = (((pixel >> 5) & 0x3F) << 2) as f32 / 255.0;
                c[2] = ((pixel & 0x1F) << 3) as f32 / 255.0;
                c[3] = 1.0;
            }
            1 => {
                // RGB8
                c[0] = dat.read_u8(offset).unwrap_or(0) as f32 / 255.0;
                c[1] = dat.read_u8(offset + 1).unwrap_or(0) as f32 / 255.0;
                c[2] = dat.read_u8(offset + 2).unwrap_or(0) as f32 / 255.0;
                c[3] = 1.0;
            }
            2 | 5 => {
                // RGBX8 / RGBA8
                c[0] = dat.read_u8(offset).unwrap_or(0) as f32 / 255.0;
                c[1] = dat.read_u8(offset + 1).unwrap_or(0) as f32 / 255.0;
                c[2] = dat.read_u8(offset + 2).unwrap_or(0) as f32 / 255.0;
                c[3] = dat.read_u8(offset + 3).unwrap_or(0) as f32 / 255.0;
            }
            3 => {
                // RGBA4
                let b0 = dat.read_u8(offset).unwrap_or(0);
                let b1 = dat.read_u8(offset + 1).unwrap_or(0);
                c[0] = ((b0 >> 4) * 17) as f32 / 255.0;
                c[1] = ((b0 & 0xF) * 17) as f32 / 255.0;
                c[2] = ((b1 >> 4) * 17) as f32 / 255.0;
                c[3] = ((b1 & 0xF) * 17) as f32 / 255.0;
            }
            4 => {
                // RGBA6
                let b0 = dat.read_u8(offset).unwrap_or(0) as u32;
                let b1 = dat.read_u8(offset + 1).unwrap_or(0) as u32;
                let b2 = dat.read_u8(offset + 2).unwrap_or(0) as u32;
                let p = (b0 << 16) | (b1 << 8) | b2;
                c[0] = ((p >> 18) & 0x3F) as f32 / 63.0;
                c[1] = ((p >> 12) & 0x3F) as f32 / 63.0;
                c[2] = ((p >> 6) & 0x3F) as f32 / 63.0;
                c[3] = (p & 0x3F) as f32 / 63.0;
            }
            _ => {}
        }
        c
    }
}

/// Polygon Object — contains vertex attribute descriptors and display list data.
///
/// A PObj is roughly a GPU primitive batch: vertex input declarations plus a
/// GX display list. Several PObjs may contribute to one visible surface.
///
/// Layout (0x18 bytes):
///   0x00: class_name_ptr (u32)
///   0x04: next_ptr (u32) — linked list
///   0x08: attributes_ptr (u32) — pointer to GxAttribute array
///   0x0C: flags (u16) — POBJ flags
///   0x0E: display_list_size (u16) — size in units of 32 bytes
///   0x10: display_list_ptr (u32) — pointer to display list data
///   0x14: flag-selected union pointer — ShapeSet, envelope array, or bind JObj
#[derive(Debug, Clone)]
pub struct PObj {
    pub offset: u32,
    pub next_ptr: Option<u32>,
    pub attributes: Vec<GxAttribute>,
    pub attributes_truncated: bool,
    pub flags: u16,
    pub display_list_size: usize,
    pub display_list_offset: Option<u32>,
    /// Raw flag-selected union pointer.
    pub union_ptr: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct EnvelopeWeight {
    pub joint_ptr: u32,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub struct EnvelopeEntry {
    pub offset: u32,
    pub weights: Vec<EnvelopeWeight>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EnvelopeParseError {
    #[error(transparent)]
    Descriptor(#[from] DescriptorParseError),
    #[error("PObj exceeds the envelope-entry budget of {limit}")]
    Entries { limit: usize },
    #[error("PObj exceeds the envelope-weight budget of {limit}")]
    Weights { limit: usize },
}

impl PObj {
    pub fn parse(dat: &DatFile, offset: u32) -> Result<Self, DescriptorParseError> {
        let source = DescriptorReader::new(dat, "PObj", offset).require_extent(0x18)?;

        let next_ptr = source.pointer("next", 0x04)?;
        let attrs_ptr = source.pointer("attributes", 0x08)?;
        let flags = source.u16(0x0C)?;
        let dl_count = source.u16(0x0E)? as usize;
        let display_list_offset = source.pointer("display_list", 0x10)?;
        let union_ptr = source.pointer("union", 0x14)?;

        // Parse attribute array (terminated by GX_VA_NULL)
        let mut attributes = Vec::new();
        let mut attributes_truncated = false;
        if let Some(attr_base) = attrs_ptr {
            for index in 0..MAX_GX_ATTRIBUTES {
                let attr_offset = gx_attribute_offset(attr_base, index)?;
                match GxAttribute::parse(dat, attr_offset)? {
                    Some(attr) => attributes.push(attr),
                    None => break, // Hit GX_VA_NULL.
                }
            }
            if attributes.len() == MAX_GX_ATTRIBUTES {
                let attr_offset = gx_attribute_offset(attr_base, MAX_GX_ATTRIBUTES)?;
                attributes_truncated = GxAttribute::parse(dat, attr_offset)?.is_some();
            }
        }

        Ok(Self {
            offset,
            next_ptr,
            attributes,
            attributes_truncated,
            flags,
            display_list_size: dl_count * 32,
            display_list_offset,
            union_ptr,
        })
    }

    pub fn has_envelope(&self) -> bool {
        flags::pobj_type(self.flags) == flags::ENVELOPE
    }

    /// Return the raw flag-selected union pointer.
    pub fn union_ptr(&self) -> Option<u32> {
        self.union_ptr
    }

    /// Return the union pointer only when its serialized meaning is a bind JObj.
    pub fn skin_joint_ptr(&self) -> Option<u32> {
        if flags::pobj_type(self.flags) == flags::SKIN {
            self.union_ptr()
        } else {
            None
        }
    }

    pub fn envelope_entries_limited(
        &self,
        dat: &DatFile,
        max_entries: usize,
        max_weights: usize,
    ) -> Result<Vec<EnvelopeEntry>, EnvelopeParseError> {
        let mut entries = Vec::new();
        let mut total_weights = 0usize;

        if !self.has_envelope() {
            return Ok(entries);
        }
        let Some(env_ptr) = self.union_ptr else {
            return Ok(entries);
        };

        let mut array_offset = env_ptr;
        loop {
            let envelope_ptr =
                DescriptorReader::new(dat, "EnvelopeArray", array_offset).pointer("entry", 0)?;
            let Some(envelope_ptr) = envelope_ptr else {
                break;
            };
            if entries.len() >= max_entries {
                return Err(EnvelopeParseError::Entries { limit: max_entries });
            }
            array_offset = array_offset
                .checked_add(4)
                .ok_or(DescriptorParseError::Truncated {
                    descriptor: "EnvelopeArray",
                    offset: array_offset,
                })?;

            let mut weights = Vec::new();
            let mut entry_offset = envelope_ptr;
            loop {
                let source = DescriptorReader::new(dat, "EnvelopeWeight", entry_offset);
                let joint_ptr = source.pointer("joint", 0)?;
                let Some(joint_ptr) = joint_ptr else {
                    break;
                };
                if total_weights >= max_weights {
                    return Err(EnvelopeParseError::Weights { limit: max_weights });
                }
                let weight = source.f32(4)?;
                weights.push(EnvelopeWeight { joint_ptr, weight });
                total_weights += 1;
                entry_offset =
                    entry_offset
                        .checked_add(8)
                        .ok_or(DescriptorParseError::Truncated {
                            descriptor: "EnvelopeWeight",
                            offset: entry_offset,
                        })?;
            }

            entries.push(EnvelopeEntry {
                offset: envelope_ptr,
                weights,
            });
        }

        Ok(entries)
    }
}

fn gx_attribute_offset(base: u32, index: usize) -> Result<u32, DescriptorParseError> {
    u32::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(0x18))
        .and_then(|relative| base.checked_add(relative))
        .ok_or(DescriptorParseError::Truncated {
            descriptor: "GxAttribute",
            offset: base,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatPointerError;

    fn dat_with_data(data: Vec<u8>) -> DatFile {
        dat_with_relocations(data, Vec::new())
    }

    fn dat_with_relocations(data: Vec<u8>, relocation_sites: Vec<u32>) -> DatFile {
        DatFile::from_parts(data, Vec::new(), relocation_sites)
    }

    fn attribute(stride: u16, scale: u8, buffer_ptr: u32) -> GxAttribute {
        GxAttribute {
            attr_name: GxAttrName::Position,
            attr_type: GxAttrType::Index16,
            comp_count: 1,
            comp_type: GxCompType::UInt8,
            scale,
            stride,
            buffer_ptr: Some(buffer_ptr),
            comp_type_raw: 0,
        }
    }

    #[test]
    fn pobj_pointer_fields_report_exact_relocation_errors() {
        for (relative, field) in [
            (0x04usize, "next"),
            (0x08, "attributes"),
            (0x10, "display_list"),
            (0x14, "union"),
        ] {
            let mut data = vec![0; 0x18];
            data[relative..relative + 4].copy_from_slice(&4u32.to_be_bytes());
            let error = PObj::parse(&dat_with_data(data), 0).unwrap_err();
            assert_eq!(
                error,
                DescriptorParseError::InvalidPointer {
                    descriptor: "PObj",
                    field,
                    field_offset: relative as u32,
                    source: DatPointerError::MissingRelocation,
                }
            );
        }

        let mut data = vec![0; 0x18];
        data[0..4].copy_from_slice(&(GxAttrName::Null as u32).to_be_bytes());
        let relocated_zero =
            PObj::parse(&dat_with_relocations(data, vec![0x04, 0x08, 0x10, 0x14]), 0)
                .expect("PObj with relocated zero pointers");
        assert_eq!(relocated_zero.next_ptr, Some(0));
        assert!(relocated_zero.attributes.is_empty());
        assert_eq!(relocated_zero.display_list_offset, Some(0));
        assert_eq!(relocated_zero.union_ptr, Some(0));
    }

    #[test]
    fn gx_attribute_distinguishes_terminators_from_malformed_descriptors() {
        assert!(matches!(
            GxAttribute::parse(
                &dat_with_data((GxAttrName::Null as u32).to_be_bytes().to_vec()),
                0,
            ),
            Ok(None)
        ));

        let mut truncated = vec![0; 4];
        truncated.copy_from_slice(&9u32.to_be_bytes());
        let error = GxAttribute::parse(&dat_with_data(truncated), 0).unwrap_err();
        assert_eq!(
            error,
            DescriptorParseError::Truncated {
                descriptor: "GxAttribute",
                offset: 0,
            }
        );

        let mut invalid_buffer = vec![0; 0x18];
        invalid_buffer[0..4].copy_from_slice(&9u32.to_be_bytes());
        invalid_buffer[0x14..0x18].copy_from_slice(&4u32.to_be_bytes());
        let error = GxAttribute::parse(&dat_with_data(invalid_buffer), 0).unwrap_err();
        assert_eq!(
            error,
            DescriptorParseError::InvalidPointer {
                descriptor: "GxAttribute",
                field: "buffer",
                field_offset: 0x14,
                source: DatPointerError::MissingRelocation,
            }
        );

        let mut unknown_name = vec![0; 0x18];
        unknown_name[0..4].copy_from_slice(&0xFEu32.to_be_bytes());
        // Only the exact GX_VA_NULL value ends the array; a name GX does not
        // define is not skipped, because nothing says how many bytes it reads.
        assert_eq!(
            GxAttribute::parse(&dat_with_data(unknown_name), 0).unwrap_err(),
            DescriptorParseError::InvalidValue {
                descriptor: "GxAttribute",
                field: "attr_name",
                field_offset: 0,
                value: 0xFE,
            }
        );

        for (relative, field, value) in [(0x04, "attr_type", 4u32), (0x0C, "comp_type", 5)] {
            let mut data = vec![0; 0x18];
            data[0..4].copy_from_slice(&9u32.to_be_bytes());
            data[relative..relative + 4].copy_from_slice(&value.to_be_bytes());
            assert_eq!(
                GxAttribute::parse(&dat_with_data(data), 0).unwrap_err(),
                DescriptorParseError::InvalidValue {
                    descriptor: "GxAttribute",
                    field,
                    field_offset: relative as u32,
                    value,
                }
            );
        }
    }

    #[test]
    fn an_entry_is_as_many_components_as_gx_reads_whatever_the_stride() {
        // Two XYZ float positions spaced 16 bytes apart, with a fraction GX
        // ignores for floats.
        let mut data = vec![0; 32];
        for (index, value) in [1.0f32, 2.0, 3.0, 9.0, 4.0, 5.0, 6.0]
            .into_iter()
            .enumerate()
        {
            data[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
        }
        let dat = dat_with_data(data);
        let position = GxAttribute {
            comp_type: GxCompType::Float,
            comp_type_raw: 4,
            ..attribute(16, 4, 0)
        };
        assert_eq!(position.decode_at(&dat, 0), [1.0, 2.0, 3.0]);
        assert_eq!(position.decode_at(&dat, 1), [4.0, 5.0, 6.0]);

        // An integer component is divided by two to the fraction.
        let dat = dat_with_data(vec![8, 16, 24, 0]);
        assert_eq!(attribute(3, 3, 0).decode_at(&dat, 0), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn gx_attribute_limit_distinguishes_terminator_overflow_and_malformed_probe() {
        fn attribute_array(attribute_count: usize, tail: [u8; 4]) -> DatFile {
            let base = 0x20usize;
            let mut data = vec![0; base + attribute_count * 0x18 + tail.len()];
            data[0x08..0x0C].copy_from_slice(&(base as u32).to_be_bytes());
            for index in 0..attribute_count {
                let offset = base + index * 0x18;
                data[offset..offset + 4].copy_from_slice(&9u32.to_be_bytes());
            }
            let tail_offset = base + attribute_count * 0x18;
            data[tail_offset..tail_offset + tail.len()].copy_from_slice(&tail);
            dat_with_relocations(data, vec![0x08])
        }

        let terminated = PObj::parse(
            &attribute_array(MAX_GX_ATTRIBUTES, (GxAttrName::Null as u32).to_be_bytes()),
            0,
        )
        .expect("64 attributes followed by GX_VA_NULL");
        assert_eq!(terminated.attributes.len(), MAX_GX_ATTRIBUTES);
        assert!(!terminated.attributes_truncated);

        let overflow = PObj::parse(
            &attribute_array(
                MAX_GX_ATTRIBUTES + 1,
                (GxAttrName::Null as u32).to_be_bytes(),
            ),
            0,
        )
        .expect("65 complete attributes");
        assert_eq!(overflow.attributes.len(), MAX_GX_ATTRIBUTES);
        assert!(overflow.attributes_truncated);

        let error =
            PObj::parse(&attribute_array(MAX_GX_ATTRIBUTES, 9u32.to_be_bytes()), 0).unwrap_err();
        assert_eq!(
            error,
            DescriptorParseError::Truncated {
                descriptor: "GxAttribute",
                offset: 0x20 + (MAX_GX_ATTRIBUTES as u32 * 0x18),
            }
        );
    }

    #[test]
    fn gx_attribute_accepts_relocated_zero_data_base_pointer() {
        let mut data = vec![0; 0x60];
        let offset = 0x20usize;
        data[offset..offset + 4].copy_from_slice(&9u32.to_be_bytes());
        data[offset + 4..offset + 8].copy_from_slice(&3u32.to_be_bytes());
        data[offset + 8..offset + 12].copy_from_slice(&1u32.to_be_bytes());
        data[offset + 12..offset + 16].copy_from_slice(&4u32.to_be_bytes());
        data[offset + 18..offset + 20].copy_from_slice(&12u16.to_be_bytes());

        let unrelocated = dat_with_data(data.clone());
        assert_eq!(
            GxAttribute::parse(&unrelocated, offset as u32)
                .unwrap()
                .expect("non-null attribute")
                .buffer_ptr,
            None
        );

        let relocated = dat_with_relocations(data, vec![(offset + 0x14) as u32]);
        assert_eq!(
            GxAttribute::parse(&relocated, offset as u32)
                .unwrap()
                .expect("non-null attribute")
                .buffer_ptr,
            Some(0)
        );
    }

    #[test]
    fn the_type_is_a_two_bit_field_that_decides_what_the_union_holds() {
        assert_eq!(flags::pobj_type(0xA001), flags::ENVELOPE);
        assert_eq!(flags::pobj_type(0xF123), flags::RESERVED_TYPE);

        let pobj_with_flags = |flags| PObj {
            offset: 0,
            next_ptr: None,
            attributes: Vec::new(),
            attributes_truncated: false,
            flags,
            display_list_size: 0,
            display_list_offset: None,
            union_ptr: None,
        };
        assert!(pobj_with_flags(0xA001).has_envelope());
        assert!(!pobj_with_flags(0xF123).has_envelope());

        let mut skin = pobj_with_flags(flags::SKIN);
        skin.union_ptr = Some(0x40);
        assert_eq!(skin.skin_joint_ptr(), Some(0x40));
        skin.flags = flags::SHAPEANIM;
        assert_eq!(skin.skin_joint_ptr(), None);
    }

    #[test]
    fn envelope_entries_preserve_null_termination_relocated_zero_and_resource_errors() {
        let mut data = vec![0u8; 0x2c];
        data[0x10..0x14].copy_from_slice(&0x20u32.to_be_bytes());
        data[0x14..0x18].copy_from_slice(&0u32.to_be_bytes());
        data[0x20..0x24].copy_from_slice(&0u32.to_be_bytes());
        data[0x24..0x28].copy_from_slice(&0.5f32.to_be_bytes());
        data[0x28..0x2c].copy_from_slice(&0u32.to_be_bytes());
        let dat = dat_with_relocations(data, vec![0x10, 0x20]);
        let pobj = PObj {
            offset: 0,
            next_ptr: None,
            attributes: Vec::new(),
            attributes_truncated: false,
            flags: flags::ENVELOPE,
            display_list_size: 0,
            display_list_offset: None,
            union_ptr: Some(0x10),
        };

        let entries = pobj
            .envelope_entries_limited(&dat, usize::MAX, usize::MAX)
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].offset, 0x20);
        assert_eq!(entries[0].weights.len(), 1);
        assert_eq!(entries[0].weights[0].joint_ptr, 0);
        assert_eq!(entries[0].weights[0].weight, 0.5);

        let terminator_at_data_end = PObj {
            union_ptr: Some(0x28),
            ..pobj.clone()
        };
        assert!(
            terminator_at_data_end
                .envelope_entries_limited(&dat, usize::MAX, usize::MAX)
                .unwrap()
                .is_empty()
        );

        assert_eq!(
            pobj.envelope_entries_limited(&dat, 0, usize::MAX)
                .unwrap_err(),
            EnvelopeParseError::Entries { limit: 0 }
        );
        assert_eq!(
            pobj.envelope_entries_limited(&dat, usize::MAX, 0)
                .unwrap_err(),
            EnvelopeParseError::Weights { limit: 0 }
        );

        for (field_offset, descriptor, field) in [
            (0x14u32, "EnvelopeArray", "entry"),
            (0x20, "EnvelopeWeight", "joint"),
        ] {
            let mut malformed = dat.clone();
            malformed.data[field_offset as usize..field_offset as usize + 4]
                .copy_from_slice(&4u32.to_be_bytes());
            malformed
                .relocation_sites
                .retain(|site| *site != field_offset);
            assert_eq!(
                pobj.envelope_entries_limited(&malformed, usize::MAX, usize::MAX)
                    .unwrap_err(),
                EnvelopeParseError::Descriptor(DescriptorParseError::InvalidPointer {
                    descriptor,
                    field,
                    field_offset,
                    source: DatPointerError::MissingRelocation,
                })
            );
        }

        let mut malformed_pointer_data = vec![0u8; 0x20];
        malformed_pointer_data[0x10..0x14].copy_from_slice(&0x18u32.to_be_bytes());
        let malformed_pointer = PObj {
            union_ptr: Some(0x10),
            ..pobj.clone()
        };
        assert_eq!(
            malformed_pointer
                .envelope_entries_limited(
                    &dat_with_data(malformed_pointer_data),
                    usize::MAX,
                    usize::MAX,
                )
                .unwrap_err(),
            EnvelopeParseError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "EnvelopeArray",
                field: "entry",
                field_offset: 0x10,
                source: DatPointerError::MissingRelocation,
            })
        );

        let mut truncated_weight_data = vec![0u8; 0x24];
        truncated_weight_data[0x10..0x14].copy_from_slice(&0x20u32.to_be_bytes());
        let truncated_weight = PObj {
            union_ptr: Some(0x10),
            ..pobj
        };
        assert_eq!(
            truncated_weight
                .envelope_entries_limited(
                    &dat_with_relocations(truncated_weight_data, vec![0x10, 0x20]),
                    usize::MAX,
                    usize::MAX,
                )
                .unwrap_err(),
            EnvelopeParseError::Descriptor(DescriptorParseError::Truncated {
                descriptor: "EnvelopeWeight",
                offset: 0x20,
            })
        );
    }

    #[test]
    fn attribute_decode_rejects_offset_overflow_and_invalid_scale() {
        let dat = dat_with_data(vec![0; 64]);

        assert!(
            attribute(u16::MAX, 0, u32::MAX - 2)
                .decode_at(&dat, 1)
                .is_empty()
        );
        assert!(attribute(1, 32, 4).decode_at(&dat, 0).is_empty());
    }

    #[test]
    fn indexed_nbt_decode_preserves_source_basis_order() {
        let expected = [[1.0_f32, -2.0, 3.5], [4.25, 5.0, -6.0], [7.0, -8.5, 9.0]];
        let data = expected
            .into_iter()
            .flatten()
            .flat_map(f32::to_be_bytes)
            .collect();
        let dat = dat_with_data(data);
        let nbt = GxAttribute {
            attr_name: GxAttrName::Nbt,
            attr_type: GxAttrType::Index16,
            comp_count: 1,
            comp_type: GxCompType::Float,
            scale: 0,
            stride: 36,
            buffer_ptr: Some(0),
            comp_type_raw: 4,
        };

        assert_eq!(nbt.decode_nbt_at(&dat, 0), Some(expected));
    }

    #[test]
    fn nbt_decode_rejects_unsupported_or_incomplete_descriptors() {
        let dat = dat_with_data(vec![0; 36]);
        let nbt = GxAttribute {
            attr_name: GxAttrName::Nbt,
            attr_type: GxAttrType::Index16,
            comp_count: 1,
            comp_type: GxCompType::Float,
            scale: 0,
            stride: 36,
            buffer_ptr: Some(0),
            comp_type_raw: 4,
        };

        let mut direct = nbt.clone();
        direct.attr_type = GxAttrType::Direct;
        assert_eq!(direct.decode_nbt_at(&dat, 0), None);

        let mut wrong_count = nbt.clone();
        wrong_count.comp_count = 0;
        assert_eq!(wrong_count.decode_nbt_at(&dat, 0), None);

        let mut short_stride = nbt.clone();
        short_stride.stride = 35;
        assert_eq!(short_stride.decode_nbt_at(&dat, 0), None);

        let mut invalid_scale = nbt;
        invalid_scale.scale = 32;
        assert_eq!(invalid_scale.decode_nbt_at(&dat, 0), None);
    }
}
