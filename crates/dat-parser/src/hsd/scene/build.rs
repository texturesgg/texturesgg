use super::super::pe::HsdPixelEngineState;
use super::super::tev::{HsdTObjTevProgram, HsdTObjTevRegisters};
use super::discovery::discover_model_roots;
use super::limits::checked_budget;
use super::{
    DObjId, HsdCustomPe, HsdCustomTev, HsdDisplayObject, HsdEnvelope, HsdImageSource, HsdJoint,
    HsdJointIndex, HsdMaterial, HsdPaletteSource, HsdPolygon, HsdPolygonBinding, HsdScene,
    HsdSceneError, HsdSceneLimits, HsdSceneRoot, HsdTexture, HsdTextureIndex, HsdTextureObject,
    HsdTextureSourceId, HsdTransform, HsdWeight, ImageDescId, JObjId, MObjId, PObjId, PeDescId,
    TObjId, TevDescId, TexLodDescId, TlutDescId, root_has_renderable_geometry,
};
use crate::DatFile;
use crate::descriptor::tobj::{TObj, TObjTevDesc};
use crate::descriptor::traversal::{self, DescriptorKind, TraversalIssue};
use crate::descriptor::{mobj, pobj};
use crate::gx::{display_list, texture, vertex};
use std::collections::HashMap;

impl HsdScene {
    /// Derive an owned render scene under conservative browser-safe limits.
    pub fn from_dat(dat: &DatFile) -> Result<Self, HsdSceneError> {
        Self::from_dat_with_limits(dat, HsdSceneLimits::default())
    }

    pub fn from_dat_with_limits(
        dat: &DatFile,
        limits: HsdSceneLimits,
    ) -> Result<Self, HsdSceneError> {
        SceneBuilder::new(dat, limits).build()
    }
}
struct SceneBuilder<'a> {
    dat: &'a DatFile,
    limits: HsdSceneLimits,
    textures: Vec<HsdTexture>,
    texture_indices: HashMap<HsdTextureSourceId, HsdTextureIndex>,
    root_names: HashMap<u32, &'a str>,
    joints: usize,
    display_objects: usize,
    polygons: usize,
    texture_objects: usize,
    gx_attributes: usize,
    display_list_bytes: usize,
    primitive_groups: usize,
    vertex_attribute_decodes: usize,
    vertices: usize,
    triangles: usize,
    decoded_texture_bytes: usize,
    envelopes: usize,
    envelope_weights: usize,
}

#[derive(Clone, Copy)]
struct SceneCheckpoint {
    textures: usize,
}

impl<'a> SceneBuilder<'a> {
    fn new(dat: &'a DatFile, limits: HsdSceneLimits) -> Self {
        Self {
            dat,
            limits,
            textures: Vec::new(),
            texture_indices: HashMap::new(),
            root_names: dat
                .roots
                .iter()
                .map(|root| (root.data_offset, root.name.as_str()))
                .collect(),
            joints: 0,
            display_objects: 0,
            polygons: 0,
            texture_objects: 0,
            gx_attributes: 0,
            display_list_bytes: 0,
            primitive_groups: 0,
            vertex_attribute_decodes: 0,
            vertices: 0,
            triangles: 0,
            decoded_texture_bytes: 0,
            envelopes: 0,
            envelope_weights: 0,
        }
    }

    fn checkpoint(&self) -> SceneCheckpoint {
        SceneCheckpoint {
            textures: self.textures.len(),
        }
    }

    fn restore(&mut self, checkpoint: SceneCheckpoint) {
        // Work counters deliberately remain monotonic: rejected stage roots
        // still consumed parser work and count against aggregate budgets.
        self.textures.truncate(checkpoint.textures);
        self.texture_indices
            .retain(|_, index| index.0 < checkpoint.textures);
    }

    fn build(mut self) -> Result<HsdScene, HsdSceneError> {
        let discovered_roots = discover_model_roots(self.dat, self.limits.max_roots)?;
        let mut roots = Vec::new();
        for discovered_root in discovered_roots {
            let root_offset = discovered_root.offset;
            let require_renderable = discovered_root.require_renderable;
            let checkpoint = self.checkpoint();
            let remaining_joints = self.limits.max_joints.saturating_sub(self.joints);
            let joint_outcome = traversal::walk_joint_tree(self.dat, root_offset, remaining_joints);
            let source_joints = joint_outcome.nodes;
            if let Some(issue) = joint_outcome.issues.into_iter().next() {
                return Err(self.traversal_error(issue));
            }
            self.joints = checked_budget(
                self.joints,
                source_joints.len(),
                self.limits.max_joints,
                "joint",
            )?;
            if source_joints.is_empty() {
                continue;
            }

            // Resolve references only against objects actually loaded by this root.
            // INSTANCE targets must not trigger another owned traversal.
            let instance_targets = if source_joints
                .iter()
                .any(|joint| joint.jobj.flags & crate::descriptor::jobj::flags::INSTANCE != 0)
            {
                source_joints
                    .iter()
                    .map(|joint| (joint.jobj.offset, HsdJointIndex(joint.index)))
                    .collect::<HashMap<_, _>>()
            } else {
                HashMap::new()
            };
            let mut joints = Vec::with_capacity(source_joints.len());
            for source_joint in source_joints {
                let display_objects = match source_joint.jobj.dobj_ptr {
                    Some(start) => self.build_display_objects(start)?,
                    None => Vec::new(),
                };
                let children =
                    if source_joint.jobj.flags & crate::descriptor::jobj::flags::INSTANCE != 0 {
                        let target = source_joint.jobj.child_ptr;
                        vec![
                            target
                                .and_then(|offset| instance_targets.get(&offset).copied())
                                .ok_or(HsdSceneError::UnresolvedInstanceTarget {
                                    instance: source_joint.jobj.offset,
                                    target,
                                })?,
                        ]
                    } else {
                        source_joint
                            .children
                            .into_iter()
                            .map(HsdJointIndex)
                            .collect()
                    };
                joints.push(HsdJoint {
                    source_id: JObjId(source_joint.jobj.offset),
                    parent: source_joint.parent_index.map(HsdJointIndex),
                    children,
                    flags: source_joint.jobj.flags,
                    local: HsdTransform {
                        scale: source_joint.jobj.scale,
                        rotation: source_joint.jobj.rotation,
                        translation: source_joint.jobj.translation,
                    },
                    inverse_bind_transform: source_joint.jobj.inverse_bind_transform(self.dat)?,
                    display_objects,
                });
            }

            let root = HsdSceneRoot {
                source_id: JObjId(root_offset),
                name: self
                    .root_names
                    .get(&root_offset)
                    .map(|name| (*name).to_owned()),
                joints,
            };
            if !require_renderable || root_has_renderable_geometry(&root) {
                roots.push(root);
            } else {
                self.restore(checkpoint);
            }
        }

        let scene = HsdScene {
            roots,
            textures: self.textures,
        };
        scene.validate()?;
        Ok(scene)
    }

    fn traversal_error(&self, issue: TraversalIssue) -> HsdSceneError {
        match issue {
            TraversalIssue::Descriptor { source, .. } => HsdSceneError::Descriptor(source),
            TraversalIssue::RepeatedPointer { kind, offset } => {
                HsdSceneError::RepeatedSourcePointer { kind, offset }
            }
            TraversalIssue::LimitExceeded {
                kind,
                limit: issue_limit,
            } => {
                let (resource, limit) = match kind {
                    DescriptorKind::JObj => ("joint", self.limits.max_joints),
                    DescriptorKind::DObj => ("display object", self.limits.max_display_objects),
                    DescriptorKind::PObj => ("polygon", self.limits.max_polygons),
                    DescriptorKind::TObj => ("texture object", self.limits.max_texture_objects),
                    DescriptorKind::MatAnimJoint
                    | DescriptorKind::MatAnim
                    | DescriptorKind::TexAnim
                    | DescriptorKind::AObj
                    | DescriptorKind::FObj => ("material animation descriptor", issue_limit),
                };
                HsdSceneError::LimitExceeded { resource, limit }
            }
        }
    }

    fn build_display_objects(
        &mut self,
        start: u32,
    ) -> Result<Vec<HsdDisplayObject>, HsdSceneError> {
        let remaining = self
            .limits
            .max_display_objects
            .saturating_sub(self.display_objects);
        let outcome = traversal::read_dobj_list(self.dat, start, remaining);
        let issue = outcome.issues.into_iter().next();
        let mut result = Vec::with_capacity(outcome.nodes.len());
        for source in outcome.nodes {
            self.display_objects = checked_budget(
                self.display_objects,
                1,
                self.limits.max_display_objects,
                "display object",
            )?;
            let polygons = match source.pobj_ptr {
                Some(start) => self.build_polygons(start)?,
                None => Vec::new(),
            };
            let material = match source.mobj_ptr {
                Some(offset) => self.build_material(offset)?,
                None => None,
            };
            result.push(HsdDisplayObject {
                source_id: DObjId(source.offset),
                material,
                polygons,
            });
        }
        if let Some(issue) = issue {
            return Err(self.traversal_error(issue));
        }
        Ok(result)
    }

    fn build_polygons(&mut self, start: u32) -> Result<Vec<HsdPolygon>, HsdSceneError> {
        let remaining = self.limits.max_polygons.saturating_sub(self.polygons);
        let outcome = traversal::read_pobj_list(self.dat, start, remaining);
        let issue = outcome.issues.into_iter().next();
        let mut result = Vec::with_capacity(outcome.nodes.len());
        for source in outcome.nodes {
            self.polygons = checked_budget(self.polygons, 1, self.limits.max_polygons, "polygon")?;
            result.push(self.build_polygon(source)?);
        }
        if let Some(issue) = issue {
            return Err(self.traversal_error(issue));
        }
        Ok(result)
    }

    fn build_polygon(&mut self, source: pobj::PObj) -> Result<HsdPolygon, HsdSceneError> {
        if source.attributes_truncated {
            return Err(HsdSceneError::LimitExceeded {
                resource: "GX attribute per polygon",
                limit: pobj::MAX_GX_ATTRIBUTES,
            });
        }
        self.gx_attributes = checked_budget(
            self.gx_attributes,
            source.attributes.len(),
            self.limits.max_gx_attributes,
            "GX attribute",
        )?;
        self.display_list_bytes = checked_budget(
            self.display_list_bytes,
            source.display_list_size,
            self.limits.max_display_list_bytes,
            "display-list byte",
        )?;
        let remaining_vertices = self.limits.max_vertices.saturating_sub(self.vertices);
        let remaining_primitive_groups = self
            .limits
            .max_primitive_groups
            .saturating_sub(self.primitive_groups);
        let remaining_vertex_attribute_decodes = self
            .limits
            .max_vertex_attribute_decodes
            .saturating_sub(self.vertex_attribute_decodes);
        let primitive_groups = match source.display_list_offset {
            Some(offset) => display_list::parse_display_list_limited(
                self.dat,
                offset,
                source.display_list_size,
                &source.attributes,
                remaining_vertices,
                remaining_primitive_groups,
                remaining_vertex_attribute_decodes,
            )
            .map_err(|error| match error {
                display_list::DisplayListLimitExceeded::Vertices { .. } => {
                    HsdSceneError::LimitExceeded {
                        resource: "vertex",
                        limit: self.limits.max_vertices,
                    }
                }
                display_list::DisplayListLimitExceeded::PrimitiveGroups { .. } => {
                    HsdSceneError::LimitExceeded {
                        resource: "primitive group",
                        limit: self.limits.max_primitive_groups,
                    }
                }
                display_list::DisplayListLimitExceeded::VertexAttributeDecodes { .. } => {
                    HsdSceneError::LimitExceeded {
                        resource: "vertex attribute decode",
                        limit: self.limits.max_vertex_attribute_decodes,
                    }
                }
            })?,
            None => Vec::new(),
        };
        self.primitive_groups = checked_budget(
            self.primitive_groups,
            primitive_groups.len(),
            self.limits.max_primitive_groups,
            "primitive group",
        )?;
        let primitive_vertex_count = primitive_groups
            .iter()
            .map(|group| group.vertices.len())
            .sum::<usize>();
        self.vertices = checked_budget(
            self.vertices,
            primitive_vertex_count,
            self.limits.max_vertices,
            "vertex",
        )?;
        let attribute_decodes = primitive_vertex_count
            .checked_mul(source.attributes.len())
            .ok_or(HsdSceneError::LimitExceeded {
                resource: "vertex attribute decode",
                limit: self.limits.max_vertex_attribute_decodes,
            })?;
        self.vertex_attribute_decodes = checked_budget(
            self.vertex_attribute_decodes,
            attribute_decodes,
            self.limits.max_vertex_attribute_decodes,
            "vertex attribute decode",
        )?;
        let decoded = vertex::decode_primitives(self.dat, &source.attributes, &primitive_groups);
        self.triangles = checked_budget(
            self.triangles,
            decoded.triangles.len(),
            self.limits.max_triangles,
            "triangle",
        )?;

        let binding = if source.has_envelope() {
            let entries = source
                .envelope_entries_limited(
                    self.dat,
                    self.limits.max_envelopes.saturating_sub(self.envelopes),
                    self.limits
                        .max_envelope_weights
                        .saturating_sub(self.envelope_weights),
                )
                .map_err(|error| match error {
                    pobj::EnvelopeParseError::Descriptor(source) => {
                        HsdSceneError::Descriptor(source)
                    }
                    pobj::EnvelopeParseError::Entries { .. } => HsdSceneError::LimitExceeded {
                        resource: "envelope",
                        limit: self.limits.max_envelopes,
                    },
                    pobj::EnvelopeParseError::Weights { .. } => HsdSceneError::LimitExceeded {
                        resource: "envelope weight",
                        limit: self.limits.max_envelope_weights,
                    },
                })?;
            let entry_count = entries.len();
            let weight_count = entries.iter().map(|entry| entry.weights.len()).sum();
            self.envelopes = checked_budget(
                self.envelopes,
                entry_count,
                self.limits.max_envelopes,
                "envelope",
            )?;
            self.envelope_weights = checked_budget(
                self.envelope_weights,
                weight_count,
                self.limits.max_envelope_weights,
                "envelope weight",
            )?;
            HsdPolygonBinding::Envelope {
                source_offset: source.union_ptr,
                entries: entries
                    .into_iter()
                    .map(|entry| HsdEnvelope {
                        source_offset: entry.offset,
                        weights: entry
                            .weights
                            .into_iter()
                            .map(|weight| HsdWeight {
                                joint: JObjId(weight.joint_ptr),
                                weight: weight.weight,
                            })
                            .collect(),
                    })
                    .collect(),
            }
        } else {
            // ShapeAnim PObjs also use rigid model matrices, but their union
            // points to a ShapeSet rather than a JObj. Bind those to the
            // containing joint until shape deformation has an explicit scene
            // contract instead of laundering the ShapeSet offset as a JObj ID.
            HsdPolygonBinding::Rigid {
                joint: source.skin_joint_ptr().map(JObjId),
            }
        };

        Ok(HsdPolygon {
            source_id: PObjId(source.offset),
            flags: source.flags,
            attributes: source.attributes,
            primitive_groups,
            decoded,
            binding,
        })
    }

    fn build_material(&mut self, offset: u32) -> Result<Option<HsdMaterial>, HsdSceneError> {
        let source = mobj::MObj::parse(self.dat, offset)?;
        let mut texture_objects = Vec::new();
        if let Some(start) = source.tobj_ptr {
            let remaining = self
                .limits
                .max_texture_objects
                .saturating_sub(self.texture_objects);
            let outcome = traversal::read_tobj_list(self.dat, start, remaining);
            let issue = outcome.issues.into_iter().next();
            for tobj in outcome.nodes {
                self.texture_objects = checked_budget(
                    self.texture_objects,
                    1,
                    self.limits.max_texture_objects,
                    "texture object",
                )?;
                let texture = self.intern_texture(&tobj)?;
                texture_objects.push(HsdTextureObject {
                    source_id: TObjId(tobj.offset),
                    image_descriptor: tobj.image_ptr.map(ImageDescId),
                    palette_descriptor: tobj.tlut_ptr.map(TlutDescId),
                    texture,
                    tex_map_id: tobj.tex_map_id,
                    tex_gen_src: tobj.tex_gen_src,
                    transform: HsdTransform {
                        scale: tobj.scale,
                        rotation: tobj.rotation,
                        translation: tobj.translation,
                    },
                    wrap_s: tobj.wrap_s,
                    wrap_t: tobj.wrap_t,
                    repeat_s: tobj.repeat_s,
                    repeat_t: tobj.repeat_t,
                    blending: tobj.blending,
                    mag_filter: tobj.mag_filter,
                    lod_descriptor: tobj.lod_ptr.map(TexLodDescId),
                    tev_descriptor: tobj.tev_ptr.map(TevDescId),
                    custom_tev: admit_custom_tev(tobj.tev_ptr, tobj.tev_desc.as_ref())?,
                    flags: tobj.flags,
                });
            }
            if let Some(issue) = issue {
                return Err(self.traversal_error(issue));
            }
        }

        Ok(Some(HsdMaterial {
            source_id: MObjId(source.offset),
            render_flags: source.render_flags,
            custom_pe: admit_custom_pe(source.pe_desc_ptr, source.pe_desc.as_ref())?,
            colors: source.material,
            textures: texture_objects,
        }))
    }

    fn intern_texture(&mut self, tobj: &TObj) -> Result<Option<HsdTextureIndex>, HsdSceneError> {
        let Some(image_ptr) = tobj.image_ptr else {
            return Ok(None);
        };
        let descriptor_id = ImageDescId(image_ptr);
        let Some(image) = tobj.image.as_ref() else {
            return Ok(None);
        };
        let palette_id = tobj.tlut_ptr.map(TlutDescId);
        let id = HsdTextureSourceId {
            image: descriptor_id,
            palette: palette_id,
        };
        if let Some(index) = self.texture_indices.get(&id) {
            return Ok(Some(*index));
        }
        if self.textures.len() >= self.limits.max_textures {
            return Err(HsdSceneError::LimitExceeded {
                resource: "texture",
                limit: self.limits.max_textures,
            });
        }

        let rgba = if let Some(data_offset) = image.data_ptr {
            let decoded_len = (image.width as usize)
                .checked_mul(image.height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or(HsdSceneError::InvalidData {
                    context: "texture dimensions overflow",
                })?;
            self.decoded_texture_bytes = checked_budget(
                self.decoded_texture_bytes,
                decoded_len,
                self.limits.max_decoded_texture_bytes,
                "decoded texture byte",
            )?;
            texture::decode_texture(
                self.dat,
                data_offset,
                image.width,
                image.height,
                image.format,
                tobj.tlut.as_ref(),
            )
        } else {
            None
        };
        let palette = palette_id.and_then(|descriptor_id| {
            tobj.tlut.as_ref().map(|palette| HsdPaletteSource {
                descriptor_id,
                data_offset: palette.data_ptr,
                format: palette.format,
                color_count: palette.color_count,
            })
        });
        let index = HsdTextureIndex(self.textures.len());
        self.texture_indices.insert(id, index);
        self.textures.push(HsdTexture {
            id,
            image: HsdImageSource {
                descriptor_id,
                data_offset: image.data_ptr,
                width: image.width,
                height: image.height,
                format: image.format,
            },
            palette,
            rgba,
        });
        Ok(Some(index))
    }
}

/// The one PEDesc on stock costumes: RGB-only source-alpha blending with a
/// LEQUAL depth test and no depth write. Tests build scenes with it.
#[cfg(test)]
pub(crate) const ADMITTED_CUSTOM_PE: mobj::PEDesc = mobj::PEDesc {
    flags: 0x19,
    ref0: 0,
    ref1: 0,
    dst_alpha: 0,
    blend_mode: 1,
    src_factor: 4,
    dst_factor: 5,
    logic_op: 15,
    z_compare: 3,
    alpha_compare0: 7,
    alpha_op: 0,
    alpha_compare1: 7,
};

fn admit_custom_pe(
    source_id: Option<u32>,
    descriptor: Option<&mobj::PEDesc>,
) -> Result<Option<HsdCustomPe>, HsdSceneError> {
    let (source_id, descriptor) = match (source_id, descriptor) {
        (None, None) => return Ok(None),
        (Some(_), None) => {
            return Err(HsdSceneError::InvalidData {
                context: "custom PE source pointer does not resolve a complete descriptor",
            });
        }
        (None, Some(_)) => {
            return Err(HsdSceneError::InvalidData {
                context: "decoded custom PE descriptor lacks source identity",
            });
        }
        (Some(source_id), Some(descriptor)) => (source_id, descriptor),
    };
    let state = HsdPixelEngineState::from_descriptor(descriptor)
        .ok_or(HsdSceneError::UnsupportedCustomPe { source_id })?;
    Ok(Some(HsdCustomPe {
        source_id: PeDescId(source_id),
        state,
    }))
}

fn admit_custom_tev(
    source_id: Option<u32>,
    descriptor: Option<&TObjTevDesc>,
) -> Result<Option<HsdCustomTev>, HsdSceneError> {
    let (source_id, descriptor) = match (source_id, descriptor) {
        (None, None) => return Ok(None),
        (Some(_), None) => {
            return Err(HsdSceneError::InvalidData {
                context: "custom TEV source pointer does not resolve a complete descriptor",
            });
        }
        (None, Some(_)) => {
            return Err(HsdSceneError::InvalidData {
                context: "decoded custom TEV descriptor lacks source identity",
            });
        }
        (Some(source_id), Some(descriptor)) => (source_id, descriptor),
    };
    let program = HsdTObjTevProgram::validate(descriptor)
        .map_err(|error| HsdSceneError::UnsupportedCustomTev { source_id, error })?;
    Ok(
        (program.color_enabled() || program.alpha_enabled()).then(|| HsdCustomTev {
            program,
            registers: HsdTObjTevRegisters::from_descriptor(descriptor),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatPointerError;
    use crate::descriptor::DescriptorParseError;
    use crate::descriptor::tobj::{self, tev_op};
    use crate::hsd::tev::{HsdTObjTevEvaluationError, HsdTObjTevSide};
    use crate::raw::root::RootNode;

    fn write_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }

    fn write_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn dat_with_roots(data: Vec<u8>, roots: Vec<RootNode>) -> DatFile {
        dat_with_roots_and_relocations(data, roots, Vec::new())
    }

    fn dat_with_roots_and_relocations(
        data: Vec<u8>,
        roots: Vec<RootNode>,
        relocation_sites: Vec<u32>,
    ) -> DatFile {
        DatFile::from_parts(data, roots, relocation_sites)
    }

    #[test]
    fn instance_targets_resolve_after_owned_loading_without_reparenting() {
        let mut data = vec![0; 0xC0];
        write_u32(&mut data, 8, 0x40);
        write_u32(&mut data, 0x44, crate::descriptor::jobj::flags::INSTANCE);
        write_u32(&mut data, 0x48, 0x80);
        write_u32(&mut data, 0x4C, 0x80);
        let mut dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                name: "instance_joint".into(),
                data_offset: 0,
            }],
            vec![8, 0x48, 0x4C],
        );
        let scene = HsdScene::from_dat(&dat).unwrap();
        let joints = &scene.roots[0].joints;
        assert_eq!(joints.len(), 3);
        assert_eq!(joints[0].children, [HsdJointIndex(1), HsdJointIndex(2)]);
        assert_eq!(joints[1].children, [HsdJointIndex(2)]);
        assert_eq!(joints[2].parent, Some(HsdJointIndex(0)));
        assert_eq!(joints[2].source_id, JObjId(0x80));

        // Relocated zero resolves to the already-loaded root, not null.
        write_u32(&mut dat.data, 0x48, 0);
        let scene = HsdScene::from_dat(&dat).unwrap();
        assert_eq!(scene.roots[0].joints[1].children, [HsdJointIndex(0)]);

        // A readable or one-past target is not implicitly loaded by a reference.
        write_u32(&mut dat.data, 0x48, 0xC0);
        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::UnresolvedInstanceTarget {
                instance: 0x40,
                target: Some(0xC0)
            }
        );
        write_u32(&mut dat.data, 0x48, 0);
        dat.relocation_sites.retain(|site| *site != 0x48);
        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::UnresolvedInstanceTarget {
                instance: 0x40,
                target: None
            }
        );
    }

    #[test]
    fn scene_preserves_jobj_descriptor_pointer_errors() {
        let mut data = vec![0u8; 0x60];
        write_u32(&mut data, 4 + 0x08, 0x20);
        let dat = dat_with_roots(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "JObj",
                field: "child",
                field_offset: 0x0c,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_rejects_truncated_present_inverse_bind_matrix() {
        let mut data = vec![0u8; 0x60];
        write_u32(&mut data, 4 + 0x38, 0x50);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![4 + 0x38],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::Truncated {
                descriptor: "InverseBindTransform",
                offset: 0x50,
            })
        );
    }

    #[test]
    fn scene_rejects_repeated_jobj_pointers() {
        let mut data = vec![0u8; 0x50];
        write_u32(&mut data, 4 + 0x08, 4);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![4 + 0x08],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::RepeatedSourcePointer {
                kind: DescriptorKind::JObj,
                offset: 4,
            }
        );
    }

    #[test]
    fn scene_rejects_repeated_dobj_pointers() {
        let mut data = vec![0u8; 0x80];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x04, 0x50);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![4 + 0x10, 0x50 + 0x04],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::RepeatedSourcePointer {
                kind: DescriptorKind::DObj,
                offset: 0x50,
            }
        );
    }

    #[test]
    fn scene_preserves_tobj_descriptor_pointer_errors() {
        let mut data = vec![0u8; 0x100];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x08, 0x70);
        write_u32(&mut data, 0x70 + 0x08, 0x90);
        write_u32(&mut data, 0x90 + 0x4c, 0x20);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x58, 0x78],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "TObj",
                field: "image",
                field_offset: 0xdc,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_preserves_dobj_descriptor_pointer_errors() {
        let mut data = vec![0u8; 0x80];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x08, 0x20);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "DObj",
                field: "mobj",
                field_offset: 0x58,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_preserves_mobj_descriptor_pointer_errors() {
        let mut data = vec![0u8; 0xa0];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x08, 0x70);
        write_u32(&mut data, 0x70 + 0x0c, 0x20);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x58],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "MObj",
                field: "material",
                field_offset: 0x7c,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_preserves_pobj_descriptor_pointer_errors() {
        let mut data = vec![0u8; 0xa0];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x0c, 0x70);
        write_u32(&mut data, 0x70 + 0x14, 0x20);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x5c],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "PObj",
                field: "union",
                field_offset: 0x84,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_rejects_malformed_envelope_array_pointer() {
        let mut data = vec![0u8; 0xb0];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x0c, 0x70);
        write_u16(&mut data, 0x70 + 0x0c, pobj::flags::ENVELOPE);
        write_u32(&mut data, 0x70 + 0x14, 0x90);
        write_u32(&mut data, 0x90, 0xa0);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x5c, 0x84],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "EnvelopeArray",
                field: "entry",
                field_offset: 0x90,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn scene_preserves_gx_attribute_pointer_errors() {
        let mut data = vec![0u8; 0xc0];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x0c, 0x70);
        write_u32(&mut data, 0x70 + 0x08, 0x90);
        write_u32(&mut data, 0x90, 9);
        write_u32(&mut data, 0x90 + 0x14, 0x20);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x5c, 0x78],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::InvalidPointer {
                descriptor: "GxAttribute",
                field: "buffer",
                field_offset: 0xa4,
                source: DatPointerError::MissingRelocation,
            })
        );
    }

    #[test]
    fn custom_pe_admission_decodes_every_representable_descriptor() {
        use super::super::super::pe::{
            HsdAlphaCompare, HsdAlphaOp, HsdBlendFactor, HsdBlendMode, HsdCompare,
            HsdDepthCompareLocation,
        };
        assert!(admit_custom_pe(None, None).unwrap().is_none());
        assert_eq!(
            admit_custom_pe(Some(0x40), None).unwrap_err(),
            HsdSceneError::InvalidData {
                context: "custom PE source pointer does not resolve a complete descriptor",
            }
        );

        let costume = admit_custom_pe(Some(0x40), Some(&ADMITTED_CUSTOM_PE))
            .unwrap()
            .expect("admitted custom PE");
        assert_eq!(costume.source_id, PeDescId(0x40));
        assert_eq!(
            costume.state,
            HsdPixelEngineState {
                color_update: true,
                alpha_update: false,
                destination_alpha: None,
                blend: HsdBlendMode::SOURCE_ALPHA,
                depth_test: true,
                depth_compare: HsdCompare::LessEqual,
                depth_write: false,
                depth_compare_location: HsdDepthCompareLocation::BeforeTexturing,
                alpha_compare: HsdAlphaCompare::ALWAYS,
                dither: false,
            }
        );

        // The most common stage PEDesc (Battlefield and others): an alpha
        // cut-out that keeps alpha in [229, 255], writing depth.
        let cutout = mobj::PEDesc {
            flags: 0x31,
            ref0: 229,
            ref1: 255,
            alpha_compare0: 6,
            alpha_compare1: 3,
            ..ADMITTED_CUSTOM_PE
        };
        let state = admit_custom_pe(Some(0x40), Some(&cutout))
            .unwrap()
            .unwrap()
            .state;
        assert!(state.depth_write);
        assert_eq!(
            state.depth_compare_location,
            HsdDepthCompareLocation::AfterTexturing
        );
        assert_eq!(
            state.alpha_compare,
            HsdAlphaCompare {
                first: HsdCompare::GreaterEqual,
                first_reference: 229,
                op: HsdAlphaOp::And,
                second: HsdCompare::LessEqual,
                second_reference: 255,
            }
        );
        assert!(!state.alpha_compare.passes(228));
        assert!(state.alpha_compare.passes(229));

        // Additive glow: source alpha onto ONE.
        let additive = mobj::PEDesc {
            dst_factor: 1,
            logic_op: 5,
            ..ADMITTED_CUSTOM_PE
        };
        assert_eq!(
            admit_custom_pe(Some(0x40), Some(&additive))
                .unwrap()
                .unwrap()
                .state
                .blend,
            HsdBlendMode::Blend {
                source: HsdBlendFactor::SourceAlpha,
                destination: HsdBlendFactor::One,
            }
        );

        // GX numbers a source factor's color terms as the destination color.
        let modulate = mobj::PEDesc {
            src_factor: 2,
            dst_factor: 2,
            ..ADMITTED_CUSTOM_PE
        };
        assert_eq!(
            admit_custom_pe(Some(0x40), Some(&modulate))
                .unwrap()
                .unwrap()
                .state
                .blend,
            HsdBlendMode::Blend {
                source: HsdBlendFactor::DestinationColor,
                destination: HsdBlendFactor::SourceColor,
            }
        );

        // Without the depth-test flag GX neither tests nor writes depth.
        let no_depth = mobj::PEDesc {
            flags: 0x29,
            blend_mode: 0,
            ..ADMITTED_CUSTOM_PE
        };
        let state = admit_custom_pe(Some(0x40), Some(&no_depth))
            .unwrap()
            .unwrap()
            .state;
        assert!(!state.depth_test);
        assert_eq!(state.blend, HsdBlendMode::None);

        // A COPY logic op is a plain write; other logic ops and values outside
        // their GX enums are not representable.
        let copy = mobj::PEDesc {
            blend_mode: 2,
            logic_op: 3,
            ..ADMITTED_CUSTOM_PE
        };
        assert_eq!(
            admit_custom_pe(Some(0x40), Some(&copy))
                .unwrap()
                .unwrap()
                .state
                .blend,
            HsdBlendMode::None
        );
        for descriptor in [
            mobj::PEDesc {
                blend_mode: 2,
                logic_op: 5,
                ..ADMITTED_CUSTOM_PE
            },
            mobj::PEDesc {
                blend_mode: 4,
                ..ADMITTED_CUSTOM_PE
            },
            mobj::PEDesc {
                src_factor: 8,
                ..ADMITTED_CUSTOM_PE
            },
            mobj::PEDesc {
                z_compare: 8,
                ..ADMITTED_CUSTOM_PE
            },
            mobj::PEDesc {
                alpha_compare1: 8,
                ..ADMITTED_CUSTOM_PE
            },
            mobj::PEDesc {
                alpha_op: 4,
                ..ADMITTED_CUSTOM_PE
            },
        ] {
            assert_eq!(
                admit_custom_pe(Some(0x40), Some(&descriptor)).unwrap_err(),
                HsdSceneError::UnsupportedCustomPe { source_id: 0x40 }
            );
        }
    }

    #[test]
    fn custom_tev_admission_preserves_inactive_descriptors_and_rejects_unsupported_active_ones() {
        let mut descriptor = TObjTevDesc {
            color_op: tev_op::ADD,
            alpha_op: tev_op::ADD,
            color_bias: tobj::tev_bias::ZERO,
            alpha_bias: tobj::tev_bias::ZERO,
            color_scale: tobj::tev_scale::SCALE_1,
            alpha_scale: tobj::tev_scale::SCALE_1,
            color_clamp: 1,
            alpha_clamp: 1,
            color_inputs: [
                tobj::tev_color_input::TEX0_RGB,
                tobj::tev_color_input::KONST_RGB,
                tobj::tev_color_input::TEXC,
                tobj::tev_color_input::ZERO,
            ],
            alpha_inputs: [tobj::tev_alpha_input::ZERO; 4],
            konst: [1, 2, 3, 4],
            tev0: [5, 6, 7, 8],
            tev1: [0; 4],
            active: 0,
        };
        assert!(
            admit_custom_tev(Some(0x40), Some(&descriptor))
                .unwrap()
                .is_none()
        );

        assert!(admit_custom_tev(None, None).unwrap().is_none());
        assert_eq!(
            admit_custom_tev(Some(0x40), None).unwrap_err(),
            HsdSceneError::InvalidData {
                context: "custom TEV source pointer does not resolve a complete descriptor",
            }
        );

        descriptor.active = tobj::tev_active::COLOR_TEV;
        descriptor.color_op = tobj::tev_op::SUB;
        assert_eq!(
            admit_custom_tev(Some(0x40), Some(&descriptor)).unwrap_err(),
            HsdSceneError::UnsupportedCustomTev {
                source_id: 0x40,
                error: HsdTObjTevEvaluationError::UnsupportedOperation {
                    side: HsdTObjTevSide::Color,
                    operation: tev_op::SUB,
                },
            }
        );
    }

    #[test]
    fn rejects_stage_descriptor_counts_beyond_the_data_table() {
        let mut data = vec![0u8; 0x100];
        write_u32(&mut data, 4 + 0x08, 0x40);
        write_u32(&mut data, 4 + 0x0C, u32::MAX);
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "map_head".into(),
            }],
            vec![0x0C],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::InvalidData {
                context: "stage descriptor count exceeds its data table"
            }
        );
    }

    #[test]
    fn aggregate_joint_limit_reports_the_configured_budget() {
        let dat = dat_with_roots(
            vec![0u8; 0x90],
            vec![
                RootNode {
                    data_offset: 4,
                    name: "first_joint".into(),
                },
                RootNode {
                    data_offset: 0x50,
                    name: "second_joint".into(),
                },
            ],
        );
        let limits = HsdSceneLimits {
            max_joints: 1,
            ..Default::default()
        };

        assert_eq!(
            HsdScene::from_dat_with_limits(&dat, limits).unwrap_err(),
            HsdSceneError::LimitExceeded {
                resource: "joint",
                limit: 1,
            }
        );
    }

    #[test]
    fn zero_vertex_display_lists_still_consume_primitive_group_budget() {
        let mut data = vec![0u8; 0x140];
        write_u32(&mut data, 4 + 0x10, 0x50);
        write_u32(&mut data, 0x50 + 0x0c, 0x70);
        write_u16(&mut data, 0x70 + 0x0e, 1);
        write_u32(&mut data, 0x70 + 0x10, 0x100);
        data[0x100] = 0x90;
        let dat = dat_with_roots_and_relocations(
            data,
            vec![RootNode {
                data_offset: 4,
                name: "synthetic_joint".into(),
            }],
            vec![0x14, 0x5C, 0x80],
        );
        let limits = HsdSceneLimits {
            max_primitive_groups: 0,
            ..Default::default()
        };

        assert_eq!(
            HsdScene::from_dat_with_limits(&dat, limits).unwrap_err(),
            HsdSceneError::LimitExceeded {
                resource: "primitive group",
                limit: 0
            }
        );
    }

    #[test]
    fn preserves_out_of_range_named_root_descriptor_errors() {
        let dat = dat_with_roots(
            vec![0u8; 0x50],
            vec![RootNode {
                data_offset: u32::MAX,
                name: "hostile_joint".into(),
            }],
        );

        assert_eq!(
            HsdScene::from_dat(&dat).unwrap_err(),
            HsdSceneError::Descriptor(DescriptorParseError::Truncated {
                descriptor: "JObj",
                offset: u32::MAX,
            })
        );
    }

    #[test]
    fn rejected_root_restore_keeps_work_budgets_monotonic() {
        let dat = dat_with_roots(Vec::new(), Vec::new());
        let mut builder = SceneBuilder::new(&dat, HsdSceneLimits::default());
        builder.joints = 3;
        let checkpoint = builder.checkpoint();
        builder.joints = 8;
        builder.vertices = 13;

        builder.restore(checkpoint);

        assert_eq!(builder.joints, 8);
        assert_eq!(builder.vertices, 13);
    }

    #[test]
    fn validation_rejects_forward_parent_references() {
        let scene = HsdScene {
            roots: vec![HsdSceneRoot {
                source_id: JObjId(4),
                name: None,
                joints: vec![HsdJoint {
                    source_id: JObjId(4),
                    parent: Some(HsdJointIndex(1)),
                    children: Vec::new(),
                    flags: 0,
                    local: HsdTransform {
                        scale: [1.0; 3],
                        rotation: [0.0; 3],
                        translation: [0.0; 3],
                    },
                    inverse_bind_transform: None,
                    display_objects: Vec::new(),
                }],
            }],
            textures: Vec::new(),
        };

        assert!(matches!(
            scene.validate(),
            Err(HsdSceneError::InvalidReference {
                resource: "parent joint",
                ..
            })
        ));
    }
}
