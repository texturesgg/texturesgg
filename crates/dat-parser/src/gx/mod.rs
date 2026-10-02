pub mod display_list;
pub mod texture;
pub mod vertex;

/// GX primitive type opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GxPrimitiveType {
    Quads = 0x80,
    Triangles = 0x90,
    TriangleStrip = 0x98,
    TriangleFan = 0xA0,
    Lines = 0xA8,
    LineStrip = 0xB0,
    Points = 0xB8,
}

impl GxPrimitiveType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x80 => Some(Self::Quads),
            0x90 => Some(Self::Triangles),
            0x98 => Some(Self::TriangleStrip),
            0xA0 => Some(Self::TriangleFan),
            0xA8 => Some(Self::Lines),
            0xB0 => Some(Self::LineStrip),
            0xB8 => Some(Self::Points),
            _ => None,
        }
    }
}

/// GX vertex attribute names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GxAttrName {
    PnMtxIdx = 0,
    Tex0MtxIdx = 1,
    Tex1MtxIdx = 2,
    Tex2MtxIdx = 3,
    Tex3MtxIdx = 4,
    Tex4MtxIdx = 5,
    Tex5MtxIdx = 6,
    Tex6MtxIdx = 7,
    Tex7MtxIdx = 8,
    Position = 9,
    Normal = 10,
    Color0 = 11,
    Color1 = 12,
    Tex0 = 13,
    Tex1 = 14,
    Tex2 = 15,
    Tex3 = 16,
    Tex4 = 17,
    Tex5 = 18,
    Tex6 = 19,
    Tex7 = 20,
    Nbt = 25,
    Null = 0xFF,
}

impl GxAttrName {
    /// The attribute `v` names; `None` for a value GX gives no attribute.
    pub fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::PnMtxIdx,
            1 => Self::Tex0MtxIdx,
            2 => Self::Tex1MtxIdx,
            3 => Self::Tex2MtxIdx,
            4 => Self::Tex3MtxIdx,
            5 => Self::Tex4MtxIdx,
            6 => Self::Tex5MtxIdx,
            7 => Self::Tex6MtxIdx,
            8 => Self::Tex7MtxIdx,
            9 => Self::Position,
            10 => Self::Normal,
            11 => Self::Color0,
            12 => Self::Color1,
            13 => Self::Tex0,
            14 => Self::Tex1,
            15 => Self::Tex2,
            16 => Self::Tex3,
            17 => Self::Tex4,
            18 => Self::Tex5,
            19 => Self::Tex6,
            20 => Self::Tex7,
            25 => Self::Nbt,
            0xFF => Self::Null,
            _ => return None,
        })
    }

    /// Whether this is the position/normal or a texture matrix index.
    pub fn is_matrix_index(&self) -> bool {
        (*self as u32) <= Self::Tex7MtxIdx as u32
    }

    pub fn is_tex_coord(&self) -> bool {
        matches!(
            self,
            Self::Tex0
                | Self::Tex1
                | Self::Tex2
                | Self::Tex3
                | Self::Tex4
                | Self::Tex5
                | Self::Tex6
                | Self::Tex7
        )
    }
}

/// GX attribute type (how data is stored in the display list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GxAttrType {
    None = 0,
    Direct = 1,
    Index8 = 2,
    Index16 = 3,
}

impl GxAttrType {
    pub fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::None,
            1 => Self::Direct,
            2 => Self::Index8,
            3 => Self::Index16,
            _ => return None,
        })
    }
}

/// GX component data type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GxCompType {
    UInt8 = 0,
    Int8 = 1,
    UInt16 = 2,
    Int16 = 3,
    Float = 4,
}

impl GxCompType {
    pub fn from_u32(v: u32) -> Option<Self> {
        Some(match v {
            0 => Self::UInt8,
            1 => Self::Int8,
            2 => Self::UInt16,
            3 => Self::Int16,
            4 => Self::Float,
            _ => return None,
        })
    }

    /// Bytes one component takes.
    pub fn byte_len(self) -> usize {
        match self {
            Self::UInt8 | Self::Int8 => 1,
            Self::UInt16 | Self::Int16 => 2,
            Self::Float => 4,
        }
    }
}

/// What a vertex attribute's components are stored as. GX numbers the formats
/// of colors and of every other attribute in separate enums, so the same
/// descriptor value means one or the other by the attribute it sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GxComponent {
    /// A position, normal, texture coordinate or matrix index.
    Number(GxCompType),
    /// `GX_VA_CLR0` or `GX_VA_CLR1`.
    Color(GxCompTypeClr),
}

impl GxComponent {
    pub fn number(self) -> Option<GxCompType> {
        match self {
            Self::Number(number) => Some(number),
            Self::Color(_) => None,
        }
    }

    pub fn color(self) -> Option<GxCompTypeClr> {
        match self {
            Self::Number(_) => None,
            Self::Color(color) => Some(color),
        }
    }
}

/// GX color component type (different enum space from GxCompType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum GxCompTypeClr {
    Rgb565 = 0,
    Rgb8 = 1,
    Rgbx8 = 2,
    Rgba4 = 3,
    Rgba6 = 4,
    Rgba8 = 5,
}

impl GxCompTypeClr {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Self::Rgb565),
            1 => Some(Self::Rgb8),
            2 => Some(Self::Rgbx8),
            3 => Some(Self::Rgba4),
            4 => Some(Self::Rgba6),
            5 => Some(Self::Rgba8),
            _ => None,
        }
    }

    /// Bytes one color takes in a display list or color array.
    pub const fn byte_len(self) -> usize {
        match self {
            Self::Rgb565 | Self::Rgba4 => 2,
            Self::Rgb8 | Self::Rgba6 => 3,
            Self::Rgbx8 | Self::Rgba8 => 4,
        }
    }
}
