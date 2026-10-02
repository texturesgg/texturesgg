# What the renderer does not do yet

Where a preview differs from the game, and why. An entry is deleted when it is
fixed. References are to the Melee decompilation
([doldecomp/melee](https://github.com/doldecomp/melee)).

## Visible today

- **Model-part runtime state is not modeled.** Kirby's copy-ability hat
  (`ftKb_SpecialN_800F1BAC`), metal forms (table 2), and action scripts
  other than Wait1's `set_dobj_flags` are not applied. Yoshi's `OnLoad`
  also freezes material animations (`ftYs_Init_8012B6E8`), which previews
  don't reproduce.
- **Preview lighting is not a Melee scene.** Lit materials use HSD's GX
  channel math, but the lights come from `NEUTRAL_PREVIEW_LIGHTING`
  (`neutral_preview_lighting` in `crates/hsd-render/src/lighting.rs`). A preset copied from a real
  scene (e.g. the character select screen) is still to be sourced. Presets
  model only ambient and infinite LObjs; point/spot lights and alpha
  lights (`LOBJ_ALPHA`) are not modeled.
- **Jigglypuff costume hats don't follow the body.** For the Red, Blue,
  Green and Yellow costumes, `ftPr_Init_8013C360` (`ftpurin.c`) loads the
  hat root (`PlyPurin??Hat_TopN_joint`) as a separate model, and
  `ftPr_Init_UnkMtxFunc0` copies a fighter part's matrix
  (`FtPart_LLegJA`) onto it every frame. The scene renders the hat root in
  its bind pose, so during idle it stays still while the body moves.
- **Emboss bump mapping is not implemented.** BUMP TObjs (`TEX_BUMP`,
  emboss texgen) are left out of the TEV chain so the rest of the material
  renders; all stock Samus costumes carry two such stages.

## Stages

- **Only joint animation 0 of each model group plays.** A stage's own
  code is not run (`melee-dat`'s `stage/playback.rs`): Rainbow Cruise and Corneria
  place their scenery with `HSD_JObjSetTranslate`, so both show it in the
  serialized pose, on top of the play area; Pokemon Stadium does not
  transform; a group's other animations are never chosen.
- **Stage material and shape animations do not play.** The model groups'
  MatAnim (`+0x08`) and ShapeAnim (`+0x0C`) banks are not attached, so
  scrolling textures and water stay still.
- **PATH joint tracks are left out.** A joint that follows a spline
  holds its serialized position.
  The SETBYTE/SETFLOAT user tracks and RObj animations are left out too.
- **Stage lights and fog are not read.** Stages draw under the neutral
  preview lighting; `map_head`'s light list and each group's fog are
  layout only.
- **The stage scale is not applied.** The game parents each model group
  to a joint scaled by the stage's `grGroundParam` (Battlefield: 0.8).
  Every group shares it, so a stage alone looks the same; it matters
  once a fighter stands on one.

## Not hit by stock costumes

- **Parent-scale compensation is missing.** For joints without the
  classical-scale flag (`0x8`), `HSD_JObjMakeMatrix` (`jobj.c:138`) passes
  the parent's accumulated scale to `HSD_MtxSRT` (`mtx.c:362`).
  `hsd/draw/` uses plain `Mat4::from_srt`.
- **Unsupported JObj flags are ignored instead of rejected:** billboards
  (`0xE00`, `PBILLBOARD`), IK (`JOINT1`/`JOINT2`/`EFFECTOR`),
  `USER_DEF_MTX`, `MTX_INDEP_*`.
- **Shared-vertex SKIN PObjs apply one matrix to every vertex.** HSD loads
  the owning JObj as PNMTX0 and `pobj->u.jobj` as PNMTX1, and each vertex
  selects one through PnMtxIdx (`pobj.c:1050`). The rigid path in
  `hsd/draw/` ignores `pn_mtx_idx`.
- **JObj pass gates are ignored.** `HSD_JObjDispAll` (`jobj.c:567`) draws a
  joint's DObjs only when its flag bits 18–20 include the current pass,
  and visits children only when bits 28–30 do. `hsd-render` draws every
  DObj in its pass regardless. No costume sets inconsistent gates.
- **DObjs without an MObj are drawn.** `hsdNew` zero-fills DObj flags and
  `DObjLoad` (`dobj.c:182`) sets pass bits only from an MObj, so HSD never
  draws such a DObj. The scene contract gives it the preview fallback
  material in the opaque pass instead. No costume has one.
- **Quaternion joint rotations are not applied.** A joint the game rotates
  by quaternion at run time (`JOBJ_USE_QUATERNION`) keeps its Euler pose.
- **A runtime scale outside the joint tree is not applied.** Flat Zone
  flattens its fighters this way.
- **Vertex decode fails silently.** Unreadable indexed attributes leave
  zeros (`gx/vertex.rs`, `GxAttribute::decode_at` returns an empty `Vec`).

## Decoder nits

- Integer normals are divided by the descriptor's scale field.
  `GXSetVtxAttrFmt` writes no fraction for NRM/NBT, so hardware uses a
  fixed fraction. This is harmless while normals are renormalized.

## Per-frame cost

- Binormals and tangents are evaluated every frame (`hsd/draw/`), but
  `hsd-render` only checks their lengths and never draws with them.
