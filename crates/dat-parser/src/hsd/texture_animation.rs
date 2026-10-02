//! Texture animations: the images a TObj flips through, such as a fighter's
//! blinking eyes.
//!
//! A model root `X_joint` pairs with a material-animation root
//! `X_matanim_joint` whose MatAnimJoint tree mirrors the JObj tree node for
//! node, each MatAnim list mirrors its joint's DObj list, and each TexAnim
//! drives the TObj with the same texture map ID (`HSD_TObjAnim`). A TexAnim
//! lists its frames as ImageDesc pointers and, for CI images, a TLUT per
//! frame.
//!
//! Pairing follows the trees' shape, not only their preorder position: the
//! game walks both trees together (`HSD_JObjAddAnimAll`), so pairing by
//! position alone would let a matanim tree missing one child credit every
//! later frame to another object's TObj.
//!
//! Frame 0 is normally the TObj's own image; later frames are images the
//! scene never draws statically, so [`HsdScene::textures`] doesn't list them.

use crate::DatFile;
use crate::descriptor::tobj::ImageDesc;
use crate::descriptor::traversal::material_animation::walk_material_animation_tree;
use crate::hsd::HsdScene;
use crate::hsd::scene::{DObjId, HsdTextureSourceId, ImageDescId, TlutDescId};

/// Bounds hostile material-animation trees; stock fighters use a few hundred
/// nodes.
const MAX_NODES: usize = 4096;
/// Bounds a frame table; stock fighters use at most six frames.
const MAX_FRAMES: usize = 256;

/// One TObj's texture animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HsdTextureAnimation {
    /// Index into [`HsdScene::roots`].
    pub root_index: usize,
    pub display_object: DObjId,
    pub tex_map_id: u32,
    /// The TObj's own image and palette, as the scene draws it.
    pub base: Option<HsdTextureSourceId>,
    /// Each frame's image and palette, in frame order, up to the first null
    /// image. A CI frame uses its own TLUT, else the TObj's palette; any
    /// other frame has no palette.
    pub frames: Vec<HsdTextureSourceId>,
}

/// Every texture animation on `scene`'s model roots. A root without a
/// matching material-animation root contributes nothing; a tree that doesn't
/// mirror the model's contributes what pairs up before the first node whose
/// place in the tree differs.
pub fn texture_animations(dat: &DatFile, scene: &HsdScene) -> Vec<HsdTextureAnimation> {
    let mut animations = Vec::new();
    for (root_index, root) in scene.roots.iter().enumerate() {
        let Some(prefix) = root
            .name
            .as_deref()
            .and_then(|name| name.strip_suffix("_joint"))
        else {
            continue;
        };
        let matanim_name = format!("{prefix}_matanim_joint");
        let Some(matanim) = dat.roots.iter().find(|root| root.name == matanim_name) else {
            continue;
        };
        // Both trees list nodes in preorder, and the walk keeps the valid
        // prefix. Two preorder lists describe the same tree up to a node
        // exactly while every node's parent index agrees.
        let nodes = walk_material_animation_tree(dat, matanim.data_offset, MAX_NODES).nodes;
        let paired =
            root.joints.iter().zip(&nodes).take_while(|(joint, node)| {
                joint.parent.map(|parent| parent.0) == node.parent_index
            });
        for (joint, node) in paired {
            for (object, material_animation) in
                joint.display_objects.iter().zip(&node.material_animations)
            {
                for texture_animation in &material_animation.texture_animations {
                    let descriptor = &texture_animation.descriptor;
                    let tobj = object.material.as_ref().and_then(|material| {
                        material
                            .textures
                            .iter()
                            .find(|tobj| tobj.tex_map_id == descriptor.texture_map_id)
                    });
                    let base = tobj.and_then(|tobj| {
                        Some(HsdTextureSourceId {
                            image: tobj.image_descriptor?,
                            palette: tobj.palette_descriptor,
                        })
                    });
                    let base_palette = base.and_then(|base| base.palette);
                    let tluts =
                        pointer_table(dat, descriptor.tlut_table_ptr, descriptor.tlut_count);
                    // A frame number indexes the table, so a null image ends it.
                    let frames =
                        pointer_table(dat, descriptor.image_table_ptr, descriptor.image_count)
                            .into_iter()
                            .map_while(|image| image)
                            .enumerate()
                            .map(|(frame, image)| HsdTextureSourceId {
                                image: ImageDescId(image),
                                palette: if uses_palette(dat, image) {
                                    tluts
                                        .get(frame)
                                        .copied()
                                        .flatten()
                                        .map(TlutDescId)
                                        .or(base_palette)
                                } else {
                                    None
                                },
                            })
                            .collect();
                    animations.push(HsdTextureAnimation {
                        root_index,
                        display_object: object.source_id,
                        tex_map_id: descriptor.texture_map_id,
                        base,
                        frames,
                    });
                }
            }
        }
    }
    animations
}

/// Whether the ImageDesc at `image` has a color-indexed format (CI4, CI8,
/// CI14X2), the only kind that reads a palette.
fn uses_palette(dat: &DatFile, image: u32) -> bool {
    ImageDesc::parse(dat, image).is_ok_and(|image| matches!(image.format, 8..=10))
}

/// A table of `count` descriptor pointers, keeping null entries in place.
/// Stops at the first entry that is neither null nor a relocated pointer.
fn pointer_table(dat: &DatFile, table: Option<u32>, count: u16) -> Vec<Option<u32>> {
    let Some(table) = table else {
        return Vec::new();
    };
    (0..usize::from(count).min(MAX_FRAMES))
        .map_while(|entry| {
            let offset = table.checked_add(u32::try_from(entry * 4).ok()?)?;
            dat.resolve_pointer(offset).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hsd::scene::{
        HsdDisplayObject, HsdJoint, HsdJointIndex, HsdMaterial, HsdSceneRoot, HsdTextureObject,
        HsdTransform, JObjId, MObjId, TObjId,
    };
    use crate::raw::root::RootNode;

    const CI8: u32 = 9;
    const RGB5A3: u32 = 5;
    const BASE_PALETTE: u32 = 0xF100;
    /// The TObj's own image and palette; CI frames fall back to this palette.
    const BASE: HsdTextureSourceId = HsdTextureSourceId {
        image: ImageDescId(0xF000),
        palette: Some(TlutDescId(BASE_PALETTE)),
    };

    /// Lays out material-animation descriptors, recording each pointer's
    /// relocation.
    #[derive(Default)]
    struct Matanim {
        data: Vec<u8>,
        relocation_sites: Vec<u32>,
    }

    impl Matanim {
        fn alloc(&mut self, size: usize) -> u32 {
            let offset = self.data.len() as u32;
            self.data
                .resize(self.data.len() + size.next_multiple_of(4), 0);
            offset
        }

        fn write(&mut self, site: u32, value: u32) {
            let site = site as usize;
            self.data[site..site + 4].copy_from_slice(&value.to_be_bytes());
        }

        fn pointer(&mut self, site: u32, target: u32) {
            self.write(site, target);
            self.relocation_sites.push(site);
        }

        fn image(&mut self, format: u32) -> u32 {
            let image = self.alloc(0x18);
            self.write(image + 0x08, format);
            image
        }

        fn tlut(&mut self) -> u32 {
            self.alloc(0x10)
        }

        /// A table of pointers, with null entries left unrelocated.
        fn table(&mut self, entries: &[Option<u32>]) -> u32 {
            let table = self.alloc(entries.len() * 4);
            for (entry, target) in entries.iter().enumerate() {
                if let Some(target) = target {
                    self.pointer(table + entry as u32 * 4, *target);
                }
            }
            table
        }

        /// A TexAnim for texture map 0 over `images`, with `tluts` per frame.
        fn texture_animation(&mut self, images: &[Option<u32>], tluts: &[Option<u32>]) -> u32 {
            let texture_animation = self.alloc(0x18);
            let image_table = self.table(images);
            self.pointer(texture_animation + 0x0c, image_table);
            if !tluts.is_empty() {
                let tlut_table = self.table(tluts);
                self.pointer(texture_animation + 0x10, tlut_table);
            }
            let counts = ((images.len() as u32) << 16) | tluts.len() as u32;
            self.write(texture_animation + 0x14, counts);
            texture_animation
        }

        /// A MatAnimJoint whose one MatAnim runs `texture_animation`.
        fn joint(&mut self, texture_animation: Option<u32>) -> u32 {
            let joint = self.alloc(0x0c);
            if let Some(texture_animation) = texture_animation {
                let material_animation = self.alloc(0x10);
                self.pointer(material_animation + 0x08, texture_animation);
                self.pointer(joint + 0x08, material_animation);
            }
            joint
        }

        fn child(&mut self, joint: u32, child: u32) {
            self.pointer(joint, child);
        }

        fn next(&mut self, joint: u32, next: u32) {
            self.pointer(joint + 0x04, next);
        }

        fn finish(mut self, root: u32) -> DatFile {
            self.relocation_sites.sort_unstable();
            DatFile::from_parts(
                self.data,
                vec![RootNode {
                    name: "model_matanim_joint".into(),
                    data_offset: root,
                }],
                self.relocation_sites,
            )
        }
    }

    /// A model root from `(parent, display object)` joints in preorder; each
    /// display object draws [`BASE`] through texture map 0.
    fn scene(joints: &[(Option<usize>, Option<u32>)]) -> HsdScene {
        let transform = HsdTransform {
            scale: [1.0; 3],
            rotation: [0.0; 3],
            translation: [0.0; 3],
        };
        let joints = joints
            .iter()
            .enumerate()
            .map(|(index, &(parent, object))| HsdJoint {
                source_id: JObjId(index as u32),
                parent: parent.map(HsdJointIndex),
                children: Vec::new(),
                flags: 0,
                local: transform,
                inverse_bind_transform: None,
                display_objects: object
                    .map(|object| HsdDisplayObject {
                        source_id: DObjId(object),
                        material: Some(HsdMaterial {
                            source_id: MObjId(object),
                            render_flags: 0,
                            custom_pe: None,
                            colors: None,
                            textures: vec![HsdTextureObject {
                                source_id: TObjId(object),
                                image_descriptor: Some(BASE.image),
                                palette_descriptor: BASE.palette,
                                texture: None,
                                tex_map_id: 0,
                                tex_gen_src: 0,
                                transform,
                                wrap_s: 0,
                                wrap_t: 0,
                                repeat_s: 1,
                                repeat_t: 1,
                                blending: 1.0,
                                mag_filter: 0,
                                lod_descriptor: None,
                                tev_descriptor: None,
                                custom_tev: None,
                                flags: 0,
                            }],
                        }),
                        polygons: Vec::new(),
                    })
                    .into_iter()
                    .collect(),
            })
            .collect();
        HsdScene {
            roots: vec![HsdSceneRoot {
                source_id: JObjId(0),
                name: Some("model_joint".into()),
                joints,
            }],
            textures: Vec::new(),
        }
    }

    fn frame(image: u32, palette: Option<u32>) -> HsdTextureSourceId {
        HsdTextureSourceId {
            image: ImageDescId(image),
            palette: palette.map(TlutDescId),
        }
    }

    /// Each animation's display object and frames.
    fn frames_by_object(dat: &DatFile, scene: &HsdScene) -> Vec<(u32, Vec<HsdTextureSourceId>)> {
        texture_animations(dat, scene)
            .into_iter()
            .map(|animation| {
                assert_eq!(animation.base, Some(BASE));
                (animation.display_object.0, animation.frames)
            })
            .collect()
    }

    #[test]
    fn a_mirrored_tree_pairs_each_joint_with_its_node() {
        let mut matanim = Matanim::default();
        let a_image = matanim.image(CI8);
        let b_image = matanim.image(CI8);
        let a_animation = matanim.texture_animation(&[Some(a_image)], &[]);
        let b_animation = matanim.texture_animation(&[Some(b_image)], &[]);
        let root = matanim.joint(None);
        let a = matanim.joint(Some(a_animation));
        let b = matanim.joint(Some(b_animation));
        matanim.child(root, a);
        matanim.next(a, b);
        let dat = matanim.finish(root);
        // root -> a, b
        let scene = scene(&[(None, None), (Some(0), Some(0xA)), (Some(0), Some(0xB))]);

        assert_eq!(
            frames_by_object(&dat, &scene),
            [
                (0xA, vec![frame(a_image, Some(BASE_PALETTE))]),
                (0xB, vec![frame(b_image, Some(BASE_PALETTE))]),
            ]
        );
    }

    #[test]
    fn a_missing_child_stops_pairing_instead_of_shifting() {
        let mut matanim = Matanim::default();
        let a_image = matanim.image(CI8);
        let b_image = matanim.image(CI8);
        let a_animation = matanim.texture_animation(&[Some(a_image)], &[]);
        let b_animation = matanim.texture_animation(&[Some(b_image)], &[]);
        let root = matanim.joint(None);
        let a = matanim.joint(Some(a_animation));
        let b = matanim.joint(Some(b_animation));
        // The matanim tree lacks a's child c, so b sits third in preorder,
        // where the model has c.
        matanim.child(root, a);
        matanim.next(a, b);
        let dat = matanim.finish(root);
        // root -> a -> c; root -> b
        let scene = scene(&[
            (None, None),
            (Some(0), Some(0xA)),
            (Some(1), Some(0xC)),
            (Some(0), Some(0xB)),
        ]);

        assert_eq!(
            frames_by_object(&dat, &scene),
            [(0xA, vec![frame(a_image, Some(BASE_PALETTE))])]
        );
    }

    #[test]
    fn a_null_tlut_entry_keeps_later_frames_palettes() {
        let mut matanim = Matanim::default();
        let images = [matanim.image(CI8), matanim.image(CI8), matanim.image(CI8)];
        let (first, third) = (matanim.tlut(), matanim.tlut());
        let animation =
            matanim.texture_animation(&images.map(Some), &[Some(first), None, Some(third)]);
        let root = matanim.joint(Some(animation));
        let dat = matanim.finish(root);

        assert_eq!(
            frames_by_object(&dat, &scene(&[(None, Some(0xA))])),
            [(
                0xA,
                vec![
                    frame(images[0], Some(first)),
                    frame(images[1], Some(BASE_PALETTE)),
                    frame(images[2], Some(third)),
                ]
            )]
        );
    }

    #[test]
    fn a_non_ci_frame_gets_no_palette() {
        let mut matanim = Matanim::default();
        let indexed = matanim.image(CI8);
        let direct = matanim.image(RGB5A3);
        let (first, second) = (matanim.tlut(), matanim.tlut());
        let animation =
            matanim.texture_animation(&[Some(indexed), Some(direct)], &[Some(first), Some(second)]);
        let root = matanim.joint(Some(animation));
        let dat = matanim.finish(root);

        assert_eq!(
            frames_by_object(&dat, &scene(&[(None, Some(0xA))])),
            [(0xA, vec![frame(indexed, Some(first)), frame(direct, None)])]
        );
    }

    #[test]
    fn a_null_image_ends_the_frames() {
        let mut matanim = Matanim::default();
        let (first, third) = (matanim.image(CI8), matanim.image(CI8));
        let animation = matanim.texture_animation(&[Some(first), None, Some(third)], &[]);
        let root = matanim.joint(Some(animation));
        let dat = matanim.finish(root);

        assert_eq!(
            frames_by_object(&dat, &scene(&[(None, Some(0xA))])),
            [(0xA, vec![frame(first, Some(BASE_PALETTE))])]
        );
    }
}
