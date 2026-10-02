//! Stages: their general points, the animations they start when they load,
//! and HAL's names for their textures.

pub mod playback;
pub mod points;
pub mod texture_names;

pub use points::{
    StagePoint, StagePointKind, StagePoints, StagePointsError, StageRect, camera_focus,
};
pub use texture_names::StageTextureNames;

#[cfg(test)]
mod test_support {
    use dat_parser::DatFile;
    use dat_parser::raw::root::RootNode;

    pub(super) fn dat_with_roots_and_relocations(
        data: Vec<u8>,
        roots: Vec<RootNode>,
        relocation_sites: Vec<u32>,
    ) -> DatFile {
        DatFile::from_parts(data, roots, relocation_sites)
    }
}
