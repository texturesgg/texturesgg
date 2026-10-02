//! Parsers for serialized HSD descriptors (JObj, DObj, PObj, MObj, TObj,
//! animation, and RObj structures) inside a parsed DAT archive.
//!
//! Archive bytes, relocation, and roots belong to `hal-dat-raw` (re-exported as
//! `crate::raw`); these parsers read descriptor fields through it and never
//! duplicate raw binary logic.

pub mod animation;
pub mod aobj;
pub mod dobj;
pub mod generic_animation;
pub mod jobj;
pub mod map_head;
pub mod material_animation;
pub mod mobj;
pub mod pobj;
mod reader;
pub mod tobj;
pub mod traversal;

pub use reader::DescriptorParseError;
pub use reader::DescriptorReader;

pub(crate) use hal_dat_raw::{DatFile, DatPointerError};
