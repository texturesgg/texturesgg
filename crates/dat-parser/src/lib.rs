pub mod descriptor;
pub mod gx;
pub mod hsd;
pub mod math;

/// Raw DAT archive primitives (bytes, header, relocation, roots, externs).
pub use hal_dat_raw as raw;
pub use hal_dat_raw::{DatExternError, DatFile, DatParseError, DatPointerError, DatResource};
