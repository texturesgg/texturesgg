use super::{DatFile, DatPointerError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DescriptorParseError {
    #[error("{descriptor} at {offset:#010x} is truncated")]
    Truncated {
        descriptor: &'static str,
        offset: u32,
    },

    #[error("{descriptor}.{field} pointer at {field_offset:#010x} is invalid: {source}")]
    InvalidPointer {
        descriptor: &'static str,
        field: &'static str,
        field_offset: u32,
        #[source]
        source: DatPointerError,
    },

    #[error("{descriptor}.{field} at {field_offset:#010x} holds {value}, which it cannot be")]
    InvalidValue {
        descriptor: &'static str,
        field: &'static str,
        field_offset: u32,
        value: u32,
    },
}

/// Bounded reads of one serialized descriptor: every access is checked against
/// the archive, and a pointer is followed only where the relocation table says
/// one is. Errors name the descriptor and the field.
#[derive(Clone, Copy)]
pub struct DescriptorReader<'a> {
    dat: &'a DatFile,
    descriptor: &'static str,
    offset: u32,
}

impl<'a> DescriptorReader<'a> {
    pub fn new(dat: &'a DatFile, descriptor: &'static str, offset: u32) -> Self {
        Self {
            dat,
            descriptor,
            offset,
        }
    }

    pub fn require_extent(self, size: usize) -> Result<Self, DescriptorParseError> {
        self.dat
            .data_slice(self.offset, size)
            .ok_or(self.truncated())?;
        Ok(self)
    }

    pub fn bytes(self, relative: u32, len: usize) -> Result<&'a [u8], DescriptorParseError> {
        self.dat
            .data_slice(self.absolute(relative)?, len)
            .ok_or(self.truncated())
    }

    pub fn array<const N: usize>(self, relative: u32) -> Result<[u8; N], DescriptorParseError> {
        self.bytes(relative, N)?
            .try_into()
            .map_err(|_| self.truncated())
    }

    pub fn u8(self, relative: u32) -> Result<u8, DescriptorParseError> {
        self.dat
            .read_u8(self.absolute(relative)?)
            .ok_or(self.truncated())
    }

    pub fn u16(self, relative: u32) -> Result<u16, DescriptorParseError> {
        self.dat
            .read_u16(self.absolute(relative)?)
            .ok_or(self.truncated())
    }

    pub fn u32(self, relative: u32) -> Result<u32, DescriptorParseError> {
        self.dat
            .read_u32(self.absolute(relative)?)
            .ok_or(self.truncated())
    }

    pub fn f32(self, relative: u32) -> Result<f32, DescriptorParseError> {
        self.dat
            .read_f32(self.absolute(relative)?)
            .ok_or(self.truncated())
    }

    fn absolute(self, relative: u32) -> Result<u32, DescriptorParseError> {
        self.offset.checked_add(relative).ok_or(self.truncated())
    }

    pub fn pointer(
        self,
        field: &'static str,
        relative: u32,
    ) -> Result<Option<u32>, DescriptorParseError> {
        let field_offset = self.absolute(relative)?;

        self.dat.resolve_pointer(field_offset).map_err(|source| {
            DescriptorParseError::InvalidPointer {
                descriptor: self.descriptor,
                field,
                field_offset,
                source,
            }
        })
    }

    /// The error for a field whose `value` is not one its type defines.
    pub fn invalid_value(
        self,
        field: &'static str,
        relative: u32,
        value: u32,
    ) -> DescriptorParseError {
        DescriptorParseError::InvalidValue {
            descriptor: self.descriptor,
            field,
            field_offset: self.offset.saturating_add(relative),
            value,
        }
    }

    fn truncated(self) -> DescriptorParseError {
        DescriptorParseError::Truncated {
            descriptor: self.descriptor,
            offset: self.offset,
        }
    }
}
