# Super Smash Bros. Melee — HAL DAT Format Tables

A reference for the HSD structures in HAL `.dat` archives as Super Smash Bros.
Melee NTSC 1.02 uses them. Layouts and rules follow the Melee decompilation at
doldecomp/melee commit
[`90f83f6665648a73122146d981eec48f173b1ca3`](https://github.com/doldecomp/melee/tree/90f83f6665648a73122146d981eec48f173b1ca3),
with HSDLib commit `0e87df7ff6a93fb53f097d03455f9fcd8fff2288` as a second
reading where the decompilation leaves a field unnamed. The base tables derive
from [MKWiiki, **HAL DAT (File Format)**](<https://mkwiiki.org/wiki/HAL_DAT_(File_Format)>),
[permanent revision 323581](<https://mkwiiki.org/w/index.php?title=HAL_DAT_(File_Format)&oldid=323581>),
corrected against the decompilation. The animation formats are in
[`ssbm_hal_dat_animation_tables.md`](ssbm_hal_dat_animation_tables.md).

All multi-byte values are big-endian. Flag words keep every bit: a bit this
reference does not name still belongs to the value.

---

## Archive Layout

An archive is a `0x20`-byte header, the data section, the relocation table, the
root node table, the reference node table, and the symbol string table, in that
order.

### File Header

| Offset | Size | Format   | Description                                                                      |
| ------ | ---: | -------- | -------------------------------------------------------------------------------- |
| `0x00` |    4 | unsigned | File size                                                                        |
| `0x04` |    4 | unsigned | Data-section size; the relocation table begins at file offset `0x20 + data_size` |
| `0x08` |    4 | unsigned | Relocation count                                                                 |
| `0x0C` |    4 | unsigned | Root node count                                                                  |
| `0x10` |    4 | unsigned | Reference node count                                                             |
| `0x14` |    4 | bytes    | Archive `version[4]`; `"001B"` when populated                                    |
| `0x18` |    4 | unsigned | Padding word 0                                                                   |
| `0x1C` |    4 | unsigned | Padding word 1                                                                   |

### Relocation Table

| Offset             |                 Size | Format   | Description                                                         |
| ------------------ | -------------------: | -------- | ------------------------------------------------------------------- |
| `0x20 + data_size` | Relocation Count × 4 | unsigned | Data-relative offset of a field that contains a relocatable pointer |

Each entry names a pointer field, not an object. A pointer field stores an
offset relative to the start of the data section at `0x20`.

`Locate` adds the runtime data-section base to the word at every listed field.
It does not skip a stored zero. A listed field that stores zero is a non-null
pointer to data offset `0`. An unlisted zero in a nullable pointer field is
null. Relocation membership, not the stored word, tells the two apart.

### Root Node

| Offset | Size | Format  | Description                             |
| ------ | ---: | ------- | --------------------------------------- |
| `0x00` |    4 | pointer | Data offset                             |
| `0x04` |    4 | pointer | String offset, relative to string table |

The root node table follows the relocation table and holds one entry per root.
A root resolves as `data + offset`, so a root at offset zero names the start of
the data section.

### Reference Node

The reference node table follows the root node table. Its entries have the root
node layout, but the first word means something else: it is the data offset of
the first pointer field in a fixup chain for the named external symbol. Each
field in the chain stores the data offset of the next field. The chain ends at
`0xFFFFFFFF`. Resolution overwrites every field in the chain with the address of
the external symbol. A zero in a chain is a link to data offset `0`, not a null.

Melee's archive initialization writes null to every field of a chain whose
symbol it does not resolve.

Sources: [`HSD_ArchiveHeader`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/archive.h#L10-L19),
[relocation loop](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/archive.c#L6-L14),
[archive/reference paths](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/archive.c#L17-L107),
[public and extern resolution](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/archive.c#L67-L118),
[unresolved externs patched to null](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbarchive.c#L14-L33),
[HSDLib relocation handling](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/HSDRawFile.cs#L121-L154),
and [DAT Texture Wizard's explicit zero-valued relocation handling](https://github.com/DRGN-DRC/DAT-Texture-Wizard/blob/c411422edeb1cd39aa5df9a41eefe31c902809b7/hsdStructures.py#L188-L202).

---

## Bone / Joint Structure (`HSD_Joint`)

| Offset | Size | Format  | Description                                                                   |
| ------ | ---: | ------- | ----------------------------------------------------------------------------- |
| `0x00` |    4 | pointer | Class-name pointer                                                            |
| `0x04` |    4 | bitmask | `HSD_Joint.flags` (`JOBJ_*`)                                                  |
| `0x08` |    4 | pointer | Child descriptor, or an object-identity reference when `JOBJ_INSTANCE` is set |
| `0x0C` |    4 | pointer | Next Bone structure                                                           |
| `0x10` |    4 | pointer | Union: DObj list, spline, or particle data according to JObj flags            |
| `0x14` |    4 | float   | Rotation X                                                                    |
| `0x18` |    4 | float   | Rotation Y                                                                    |
| `0x1C` |    4 | float   | Rotation Z                                                                    |
| `0x20` |    4 | float   | Scale X                                                                       |
| `0x24` |    4 | float   | Scale Y                                                                       |
| `0x28` |    4 | float   | Scale Z                                                                       |
| `0x2C` |    4 | float   | Location X                                                                    |
| `0x30` |    4 | float   | Location Y                                                                    |
| `0x34` |    4 | float   | Location Z                                                                    |
| `0x38` |    4 | pointer | Optional envelope (inverse-bind) 3×4 matrix                                   |
| `0x3C` |    4 | pointer | `HSD_RObjDesc`                                                                |

The loader copies the `0x38` matrix to the runtime JObj only when the pointer is
non-null. A null pointer leaves the JObj without an envelope matrix; it does not
install an identity or parent matrix. [Envelope matrices](#envelope-matrices)
describes how drawing uses it.

### JObj flag values

| Value or mask | Name                           | Meaning                                                                                 |
| ------------: | ------------------------------ | --------------------------------------------------------------------------------------- |
|  `0x00000001` | `JOBJ_SKELETON`                | Skeleton classification                                                                 |
|  `0x00000002` | `JOBJ_SKELETON_ROOT`           | Skeleton-root classification                                                            |
|  `0x00000004` | `JOBJ_ENVELOPE_MODEL`          | Envelope-model classification                                                           |
|  `0x00000008` | `JOBJ_CLASSICAL_SCALE`         | Classical-scale path                                                                    |
|  `0x00000010` | `JOBJ_HIDDEN`                  | Hidden state; also controlled by NODE/BRANCH animation                                  |
|  `0x00000020` | `JOBJ_PTCL`                    | Selects the particle-list member of the `0x10` union unless SPLINE is also set          |
|  `0x00000040` | `JOBJ_MTX_DIRTY`               | Runtime state, set on every new JObj                                                    |
|  `0x00000080` | `JOBJ_LIGHTING`                | Lighting flag                                                                           |
|  `0x00000100` | `JOBJ_TEXGEN`                  | Texture-generation flag                                                                 |
|  `0x00000E00` | `JOBJ_BILLBOARD_FIELD`         | Field mask for the ordinary billboard modes                                             |
|  `0x00000200` | `JOBJ_BILLBOARD`               | Billboard field value                                                                   |
|  `0x00000400` | `JOBJ_VBILLBOARD`              | Vertical-billboard field value                                                          |
|  `0x00000600` | `JOBJ_HBILLBOARD`              | Horizontal-billboard field value                                                        |
|  `0x00000800` | `JOBJ_RBILLBOARD`              | Rotational-billboard field value                                                        |
|  `0x00001000` | `JOBJ_INSTANCE`                | `child` is resolved as a shared JObj identity rather than loaded as an owned child tree |
|  `0x00002000` | `JOBJ_PBILLBOARD`              | Billboard mode outside `JOBJ_BILLBOARD_FIELD`                                           |
|  `0x00004000` | `JOBJ_SPLINE`                  | Selects the spline member of the `0x10` union; the loader checks this before PTCL       |
|  `0x00008000` | `JOBJ_FLIP_IK`                 | Inverse-kinematics flag                                                                 |
|  `0x00010000` | `JOBJ_SPECULAR`                | Specular flag                                                                           |
|  `0x00020000` | `JOBJ_USE_QUATERNION`          | Selects quaternion rotation behavior                                                    |
|  `0x00040000` | `JOBJ_UNK_B18`                 | Draws this JObj in the OPA pass                                                         |
|  `0x00080000` | `JOBJ_UNK_B19`                 | Draws this JObj in the XLU pass                                                         |
|  `0x00100000` | `JOBJ_UNK_B20`                 | Draws this JObj in the TEXEDGE pass                                                     |
|  `0x00000000` | `JOBJ_NULL_OBJ`                | Null value of the two-bit joint-kind field                                              |
|  `0x00200000` | `JOBJ_JOINT1`                  | First named joint-kind field value                                                      |
|  `0x00400000` | `JOBJ_JOINT2`                  | Second named joint-kind field value                                                     |
|  `0x00600000` | `JOBJ_JOINT` / `JOBJ_EFFECTOR` | Shared value for the third joint-kind field value and effector alias                    |
|  `0x00800000` | `JOBJ_USER_DEF_MTX`            | User-defined-matrix path                                                                |
|  `0x01000000` | `JOBJ_MTX_INDEP_PARENT`        | Parent-matrix-independent path                                                          |
|  `0x02000000` | `JOBJ_MTX_INDEP_SRT`           | S/R/T-independent matrix path                                                           |
|  `0x04000000` | `JOBJ_UNK_B26`                 | Meaning unknown                                                                         |
|  `0x08000000` | `JOBJ_UNK_B27`                 | Meaning unknown                                                                         |
|  `0x10000000` | `JOBJ_ROOT_OPA`                | Enables child traversal for the OPA display pass                                        |
|  `0x20000000` | `JOBJ_ROOT_XLU`                | Enables child traversal for the XLU display pass                                        |
|  `0x40000000` | `JOBJ_ROOT_TEXEDGE`            | Enables child traversal for the TEXEDGE display pass                                    |
|  `0x70000000` | `JOBJ_ROOT_MASK`               | Mask of the three propagated child-traversal bits                                       |

`HSD_JObjDispAll` receives an `HSD_TrspMask` value (`OPA = 1`, `XLU = 2`, or
`TEXEDGE = 4`). It shifts that value by 18 to decide whether to draw the current
JObj and by 28 to decide whether to recurse into ordinary children. Parent
update code propagates a child's local bits 18..20, and its subtree bits 28..30,
into ancestor bits 28..30. The decompilation names bits 18..20 `UNK_B18..20`.

The serialized word is ORed into a newly allocated runtime JObj whose
initializer has already set `JOBJ_MTX_DIRTY`. At `0x10`, SPLINE wins when set,
otherwise PTCL selects the particle list, and neither selects the DObj list.

For `JOBJ_INSTANCE`, the loader does not construct `child` recursively. The
later reference-resolution pass looks up the descriptor identity in the JObj ID
table and references that existing object. Animation traversal and recursive
flag changes stop below an INSTANCE node. Display traversal follows the shared
child with an instance-relative matrix.

Sources: [`HSD_Joint` and flag definitions](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.h#L59-L145),
[`HSD_TrspMask`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/forward.h#L142-L147),
[`HSD_JObjDispAll`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L571-L603),
[parent pass-bit propagation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L803-L829),
[`JObjLoad`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L614-L668),
[`HSD_JObjResolveRefsAll`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L686-L717),
[INSTANCE-aware animation traversal](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L547-L558),
[recursive flag changes](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L997-L1046),
and [`JObjInit`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L1473-L1484).

## Spline (`HSD_Spline`)

Size: `0x18`.

| Offset | Size | Format  | Description                                                                                          |
| ------ | ---: | ------- | ---------------------------------------------------------------------------------------------------- |
| `0x00` |    1 | enum    | Spline type                                                                                          |
| `0x01` |    1 | padding | Alignment before `numcv`                                                                             |
| `0x02` |    2 | signed  | `numcv`; the evaluator partitions the normalized parameter into `numcv - 1` intervals                |
| `0x04` |    4 | float   | Cardinal-spline tension (type `3`)                                                                   |
| `0x08` |    4 | pointer | Control-point `Vec3` array                                                                           |
| `0x0C` |    4 | float   | Total arc length used by nonlinear types `1..3`                                                      |
| `0x10` |    4 | pointer | Cumulative normalized segment-length array                                                           |
| `0x14` |    4 | pointer | Per-segment five-coefficient polynomial used under a square root for arc-length integration (`1..3`) |

| Value | Evaluator selected by `splGetSplinePoint` |
| ----: | ----------------------------------------- |
|   `0` | Linear interpolation                      |
|   `1` | Cubic Bézier                              |
|   `2` | B-spline                                  |
|   `3` | Cardinal spline using `tension`           |

Values outside `0..3` have no evaluator.

The JObj loader does not clone this structure. When `JOBJ_SPLINE` selects the
`HSD_Joint` union, the runtime JObj keeps the relocated spline pointer. PATH
animation uses the precomputed length arrays to convert a normalized distance to
a spline parameter before it evaluates a point.

Let `N = numcv`, with `N >= 2`. `N` counts parameter breakpoints, not always
serialized `Vec3` records:

| Type | Control-point `Vec3` count | Selection for interval `i`             |
| ---: | -------------------------: | -------------------------------------- |
|  `0` |                        `N` | `cv[i]` and `cv[i + 1]`                |
|  `1` |             `3(N - 1) + 1` | four Bézier points beginning `cv[3i]`  |
|  `2` |                    `N + 2` | four B-spline points beginning `cv[i]` |
|  `3` |                    `N + 2` | four cardinal points beginning `cv[i]` |

The cumulative `segLength` array has `N` floats and is indexed through `i + 1`.
The distance lookup assumes it runs from `0` to `1` without decreasing. Nonlinear types `1..3` also read
`N - 1` rows of five `segPoly` coefficients. The evaluator uses each row under a
square root during iterative Simpson integration, and snaps polynomial results
in `(-0.001, 0)` to zero. Type `0` does not read the polynomial rows.

The game trusts every count, pointer, and coefficient, and its segment search
has no bound.

Sources: [`HSD_Spline`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/spline.h#L8-L16),
[HSDLib layout](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_Spline.cs#L6-L48),
[JObj union-pointer load](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L633-L643),
[`splGetSplinePoint`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/spline.c#L83-L136),
and [arc-length parameterization](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/spline.c#L178-L237).

## Stage map head (`map_head`)

Stage archive initialization resolves the public symbol `map_head` as a
`0x30`-byte top-level descriptor. The decompilation leaves several members
unnamed; those rows use the HSDLib name.

| Offset | Size | Format  | Description                                                                                                           |
| ------ | ---: | ------- | --------------------------------------------------------------------------------------------------------------------- |
| `0x00` |    4 | pointer | General-point array (HSDLib name)                                                                                     |
| `0x04` |    4 | signed  | Number of `0x00` entries                                                                                              |
| `0x08` |    4 | pointer | Contiguous `0x34` stage model-group descriptors                                                                       |
| `0x0C` |    4 | signed  | Number of stage model-group descriptors                                                                               |
| `0x10` |    4 | pointer | `HSD_Spline*` array                                                                                                   |
| `0x14` |    4 | signed  | Number of spline pointers                                                                                             |
| `0x18` |    4 | pointer | Light array (HSDLib name)                                                                                             |
| `0x1C` |    4 | signed  | Number of `0x18` entries                                                                                              |
| `0x20` |    4 | pointer | Map-spline descriptor array (HSDLib name)                                                                             |
| `0x24` |    4 | signed  | Number of `0x20` entries                                                                                              |
| `0x28` |    4 | pointer | Material-object pointer array (HSDLib name); the game sets bit `0x04000000` in the `0x04` word of each non-null entry |
| `0x2C` |    4 | signed  | Number of `0x28` entries                                                                                              |

### Stage model-group descriptor

Size: `0x34`.

| Offset | Size | Format  | Description                                                                               |
| ------ | ---: | ------- | ----------------------------------------------------------------------------------------- |
| `0x00` |    4 | pointer | Root `HSD_Joint` descriptor                                                               |
| `0x04` |    4 | pointer | Joint-animation bank (`HSD_AnimJoint**`)                                                  |
| `0x08` |    4 | pointer | Material-animation bank (`HSD_MatAnimJoint**`)                                            |
| `0x0C` |    4 | pointer | Shape-animation bank (`HSD_ShapeAnimJoint**`)                                             |
| `0x10` |    4 | pointer | Perspective camera descriptor                                                             |
| `0x14` |    4 | unknown | Meaning unknown                                                                           |
| `0x18` |    4 | pointer | Light list (HSDLib name)                                                                  |
| `0x1C` |    4 | pointer | Fog descriptor                                                                            |
| `0x20` |    4 | pointer | Collision-link `GrJoint` triples (`s16` collision index, unnamed `s16`, `s16` JObj index) |
| `0x24` |    4 | signed  | Number of collision-link triples                                                          |
| `0x28` |    4 | pointer | Optional byte flags indexed with selected animations                                      |
| `0x2C` |    4 | pointer | `s16` JObj-selection index array                                                          |
| `0x30` |    4 | signed  | Number of JObj-selection indices                                                          |

The array at `0x2C` holds one `s16` per `0x30` entry, each a JObj-selection
index. HSDLib reads it as `0x06`-byte triples; the game does not.

The stage loader bounds model groups with the top-level `0x0C` count and selects
the group at the runtime map ID. It loads `0x00` through `HSD_JObjLoadJoint`.
Animation code selects the three banks independently, then attaches their
AnimJoint, MatAnimJoint, and ShapeAnimJoint descriptors to the loaded graph.

A model-group field can be an external-reference fixup site. Archive
initialization clears such a field to null; its stored word is a chain link, not
an offset.

The archive stores no length for an animation bank. Stage code selects an
animation with two indices: a resource index into the bank's pointer array, then
a contiguous index into the AnimJoint descriptors at that pointer. The second
index can be nonzero, so one bank pointer can reach several adjacent AnimJoint
roots. The Kraid stage does this.

Sources: [stage map-head layouts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/types.h#L1921-L1962),
[`map_head` public-root loading](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/grdatfiles.c#L174-L187),
[model-group selection and loading](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/ground.c#L799-L921),
[stage animation-bank/direct-index selection](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/granime.c#L958-L1138),
[initial animation attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/granime.c#L1156-L1215),
[Kraid direct-index selections](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gr/grkraid.c#L190-L257),
[HSDLib map head](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Melee/Gr/SBM_Map_Head.cs#L5-L70),
and [HSDLib model group](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Melee/Gr/SBM_Map_GOBJ.cs#L6-L87).

## Effect data table root

Each `eff*DataTable` public root begins with two particle-bank pointers followed
by an inline effect-model array:

| Offset | Size | Format  | Description                                  |
| ------ | ---: | ------- | -------------------------------------------- |
| `0x00` |    4 | pointer | Particle-data bank                           |
| `0x04` |    4 | pointer | Particle texture-graphics bank               |
| `0x08` |    — | array   | Embedded `0x14`-byte `EF_EffectDesc` records |

An `EF_EffectDesc` contains a lifetime followed by the four-pointer
`StaticModelDesc`:

| Offset | Size | Format  | Description               |
| ------ | ---: | ------- | ------------------------- |
| `0x00` |    4 | float   | Lifetime / frame count    |
| `0x04` |    4 | pointer | Root `HSD_Joint`          |
| `0x08` |    4 | pointer | Root `HSD_AnimJoint`      |
| `0x0C` |    4 | pointer | Root `HSD_MatAnimJoint`   |
| `0x10` |    4 | pointer | Root `HSD_ShapeAnimJoint` |

Archive loading resolves the registered public root, passes its first two words
to particle-bank initialization, and stores the address of `0x08` as the
model-array base. Generic effect creation divides its numeric effect ID by
decimal `1000` for the archive-bank index and takes the remainder as the
embedded-record index. It loads the model JObj and, when any animation pointer
is non-null, calls `HSD_JObjAddAnimAll`.

The array has no serialized count, and the creation path does not check the
record index.

Sources: [effect and registry layouts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ef/types.h#L80-L100),
[`StaticModelDesc`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/sc/types.h#L9-L15),
[archive/root registry and model-base initialization](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ef/efasync.c#L1138-L1318),
[generic record selection and attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ef/eflib.c#L437-L539),
and [HSDLib layout](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Melee/Ef/SBM_EffectTable.cs#L6-L29).

## Stage-select data table

The public `MnSelectStageDataTable` root begins with scene descriptors followed
by twelve contiguous `StaticModelDesc` records. The game reads the first `0xD0`
bytes:

| Offset |   Size | Format                | Description                     |
| ------ | -----: | --------------------- | ------------------------------- |
| `0x00` |      4 | pointer               | Camera descriptor               |
| `0x04` |      4 | pointer               | First light descriptor          |
| `0x08` |      4 | pointer               | Second light descriptor         |
| `0x0C` |      4 | pointer               | Fog descriptor                  |
| `0x10` | `0xC0` | `StaticModelDesc[12]` | Model and three animation roots |

Stage-select code loads `MnSlMap.usd` when the saved language is US and
`MnSlMap.dat` otherwise, resolves this public root, and treats `0x10` as the
descriptor-array base. Its model-loading and animation-attachment calls use all
twelve descriptors.

Sources: [stage-select descriptor layout](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnstagesel.static.h#L53-L69),
[`StaticModelDesc`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/sc/types.h#L9-L15),
[archive and public-root selection](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnstagesel.c#L376-L405),
[`xB0` and `x30` descriptor selections](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnstagesel.c#L101-L225),
and [remaining descriptor selections](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnstagesel.c#L441-L748).

## Main-menu public-root model quartets

Archive: `MnMaAll`.

Unlike the inline effect and stage-select records above, this archive stores
each model component as a separate named public root. Menu code resolves those
roots into runtime `StaticModelDesc` storage:

| Public-root suffix     | Runtime descriptor type |
| ---------------------- | ----------------------- |
| `_Top_joint`           | `HSD_Joint`             |
| `_Top_animjoint`       | `HSD_AnimJoint`         |
| `_Top_matanim_joint`   | `HSD_MatAnimJoint`      |
| `_Top_shapeanim_joint` | `HSD_ShapeAnimJoint`    |

The archive has no serialized `StaticModelDesc` table or count. The set of
models is the set of public-root names the menu code requests. Initial main-menu
entry loads `MnMaAll` and requests 20 complete quartets. Menu-specific loaders
bring the total to 57 distinct JObj roots, 51 of them with all three animation
roots.

The game requests six names as JObj only: `MenMainCursorB1_Top`,
`MenMainCursorB3_Top`, `MenMainCursorVi_Top`, `MenMainLoadSn_Top`,
`MenMainMarkEv_Top`, and `MenMainPhotoSn_Top`. The archive holds animation roots
under those names too, but the game does not request them.

Sources: [`StaticModelDesc`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/sc/types.h#L9-L15),
[public-root resolution](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbarchive.c#L36-L49),
[initial `MnMaAll` manifest](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnmain.c#L2760-L2908),
[name-entry roots](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnname.c#L1656-L1701),
[diagram roots](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mndiagram.c#L2984-L3026),
[snapshot roots](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnsnap.c#L2531-L2552),
[event roots](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnevent.c#L89-L118),
[event selection](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnevent.c#L700-L724),
[vibration roots](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnvibration.c#L133-L145),
[vibration selection](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnvibration.c#L1040-L1066),
[count](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mncount.c#L815-L832),
[data-delete](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mndatadel.c#L849-L874),
[deflicker](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mndeflicker.c#L177-L194),
[gallery](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mngallery.c#L464-L491),
[multi-man](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnhyaku.c#L198-L218),
[bonus records](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mninfobonus.c#L315-L334),
[language](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnlanguage.c#L202-L216),
[sound](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnsound.c#L333-L349),
and [sound test](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnsoundtest.c#L849-L867).

## Extension-menu public-root model quartets

Archive family: `MnExtAll`.

Character select opens `MnExtAll.dat` for Japanese and `MnExtAll.usd` otherwise,
then passes the archive to either the return-to-main-menu loader or the
name-entry loader. Tournament setup opens `MnExtAll` without an extension; the
shared file resolver appends `.usd` for US and `.dat` otherwise.

The return-to-main-menu loader requests 18 complete model quartets using the
four suffixes above. Name entry requests six, of which the background and panel
quartets overlap, making 22 distinct quartets.

`MnExtAll.dat` also holds the quartets `MenMainConTop_Top` and
`MenMainCursor_Top`. The game requests those names from `MnMaAll`, not from
`MnExtAll`. A public-root name does not tell which archive the game reads it
from.

Sources: [character-select archive choice](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mncharsel.c#L4958-L4967),
[character-select consumers](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mncharsel.c#L5072-L5087),
[18-quartet return-to-main-menu manifest](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnmainrule.c#L1391-L1524),
[six-quartet name-entry manifest](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/mn/mnnamenew.c#L2077-L2148),
[tournament archive load](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gm/gmtou_0.c#L2925-L2937),
[tournament main-menu consumer](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gm/gmtou_0.c#L2208-L2220),
[tournament name-entry consumer](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gm/gmtou_0.c#L2656-L2663),
[archive-to-file-loader flow](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbarchive.c#L53-L68),
and [locale extension resolution](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbfile.c#L47-L80).

## Reference object (`HSD_RObjDesc`)

Size: `0x0C`.

| Offset | Size | Format  | Description                                                                                     |
| ------ | ---: | ------- | ----------------------------------------------------------------------------------------------- |
| `0x00` |    4 | pointer | Next RObj descriptor                                                                            |
| `0x04` |    4 | bitmask | Active bit, reference type, and type-specific low 28 bits                                       |
| `0x08` |    4 | union   | Expression pointer, JObj identity, limit float, bytecode-expression pointer, or IK-hint pointer |

|               Flags/mask | Name               | Use                                                                                      |
| -----------------------: | ------------------ | ---------------------------------------------------------------------------------------- |
|             `0x80000000` | no public macro    | Active bit; copied on load and controlled by RObj animation                              |
|             `0x70000000` | `ROBJ_TYPE_MASK`   | Selects the `0x08` union branch                                                          |
|             `0x00000000` | `REFTYPE_EXP`      | Function-expression descriptor pointer                                                   |
|             `0x10000000` | `REFTYPE_JOBJ`     | `HSD_Joint` identity resolved to a runtime JObj                                          |
|             `0x20000000` | `REFTYPE_LIMIT`    | Inline `f32` transform limit                                                             |
|             `0x30000000` | `REFTYPE_BYTECODE` | Bytecode-expression descriptor pointer; normalized to EXP in runtime flags after loading |
|             `0x40000000` | `REFTYPE_IKHINT`   | Pointer to two floats: bone length and rotation X hint                                   |
| `0x50000000..0x70000000` | —                  | Invalid; the loader panics                                                               |
|             `0x0FFFFFFF` | no public macro    | Type-specific subtype; EXP/BYTECODE uses a JObj output channel                           |

JObj-reference and expression consumers require the active bit. The general
`resolveLimits` pass ignores it. Other LIMIT lookups go through
`HSD_RObjGetByType`, which requires it. The
[RObj active-state channel](ssbm_hal_dat_animation_tables.md#robj-active-state-channel) animates the bit.

### Expression payload descriptors

`REFTYPE_EXP` selects `HSD_ExpDesc`; `REFTYPE_BYTECODE` selects
`HSD_ByteCodeExpDesc`. Both descriptors are `0x08` bytes:

| Offset | Size | `HSD_ExpDesc`                  | `HSD_ByteCodeExpDesc` |
| ------ | ---: | ------------------------------ | --------------------- |
| `0x00` |    4 | Function pointer/reference     | Bytecode pointer      |
| `0x04` |    4 | `HSD_RvalueList` array pointer | Same                  |

Each `HSD_RvalueList` entry is `0x08` bytes: flags at `0x00`, then an
`HSD_Joint` identity pointer at `0x04`. `loadRvalue` consumes consecutive
entries until the joint pointer is null; the flags word is not the terminator.
Reference resolution later maps the serialized joint identity to the loaded
JObj.

#### Rvalue argument order and expression output

On first evaluation, the game caches `nb_args` as the population count of every
Rvalue flags word. It then walks Rvalues in list order and set bits from least
to most significant, calling `HSD_JObjSetupMatrix` before it reads each
referenced JObj:

| Rvalue flag | Argument appended to the shared float buffer   |
| ----------: | ---------------------------------------------- |
|       `0x1` | Local rotation X, radians converted to degrees |
|       `0x2` | Local rotation Y, radians converted to degrees |
|       `0x4` | Local rotation Z, radians converted to degrees |
|       `0x8` | No argument written                            |
|      `0x10` | Local translation X                            |
|      `0x20` | Local translation Y                            |
|      `0x40` | Local translation Z                            |
|      `0x80` | Local scale X                                  |
|     `0x100` | Local scale Y                                  |
|     `0x200` | Local scale Z                                  |
|     `0x400` | No argument written                            |
|     `0x800` | No argument written                            |
|   `0x10000` | Matrix rotation X, converted to degrees        |
|   `0x20000` | Matrix rotation Y, converted to degrees        |
|   `0x40000` | Matrix rotation Z, converted to degrees        |
|  `0x100000` | Matrix translation X                           |
|  `0x200000` | Matrix translation Y                           |
|  `0x400000` | Matrix translation Z                           |
|  `0x800000` | Matrix scale X                                 |
| `0x1000000` | Matrix scale Y                                 |
| `0x2000000` | Matrix scale Z                                 |

Any other set bit adds to the cached count but writes no argument and does not
advance the write cursor. A bytecode argument index that only such a bit admits
reads a stale slot in the reusable buffer.

`REFTYPE_BYTECODE` is normalized to runtime `REFTYPE_EXP` during loading.
`HSD_RObjUpdateAll` evaluates an expression only while that reference type and
the active bit are both set. A bytecode or function expression returns one
float. The low RObj bits go to the update callback as the output channel; values
`1`, `2`, and `3` are converted from degrees to radians first. RObj evaluation
runs during JObj matrix setup with `JObjUpdateFunc`, so the low bits use the
JObj channel numbers listed in
[`ssbm_hal_dat_animation_tables.md`](ssbm_hal_dat_animation_tables.md).

#### Bytecode VM

`HSD_ByteCodeEval` is a stack machine over raw 32-bit values. Multi-byte
operands are stored most-significant byte first. The bytecode has no stored
length: the interpreter reads until `RETURN`.

|          Opcode | Operand | Stack/control behavior                                                        |
| --------------: | ------- | ----------------------------------------------------------------------------- |
|          `0x00` | -       | No operation                                                                  |
|          `0x01` | -       | Return the top value as `f32` and free the whole stack                        |
|          `0x02` | `u16`   | Push argument at the given index; assert `index < nb_args`                    |
|          `0x03` | `u16`   | Pop an integer condition; skip that many bytes forward when nonzero           |
|          `0x04` | `u16`   | Skip that many bytes forward unconditionally                                  |
|          `0x05` | `u8`    | Remove that many stack entries; excess removes leave an empty stack           |
|          `0x06` | `u32`   | Push the operand's raw 32-bit value                                           |
|          `0x07` | -       | Convert top `f32` to signed integer                                           |
|          `0x08` | -       | Convert top signed integer to `f32`                                           |
| `0x09` / `0x0A` | -       | Negate top as `f32` / signed integer                                          |
| `0x0B` / `0x0C` | -       | Replace top with random integer `0..1` / random float                         |
|    `0x0D..0x0F` | -       | `sin`, `cos`, `tan`; input is degrees                                         |
|    `0x10..0x12` | -       | `asin`, `acos`, `atan`; result is degrees                                     |
| `0x13` / `0x14` | -       | Natural log / exponential                                                     |
|          `0x15` | -       | Negate top only when its `f32` value compares less than zero                  |
|          `0x16` | -       | Square root                                                                   |
|    `0x17..0x1A` | -       | Float `+`, `-`, `*`, `/`; compute below op top                                |
|          `0x1B` | -       | File-local `fmodf(top divisor, below dividend)`; computes below remainder top |
|    `0x1C..0x20` | -       | Signed integer `+`, `-`, `*`, `/`, remainder in the same order                |
|          `0x21` | -       | Float power: below raised to top                                              |
| `0x22` / `0x23` | -       | Replace below with top when below is greater / less, respectively             |
| `0x24` / `0x25` | -       | Same comparisons as signed integers                                           |
|          `0x26` | -       | `atan2(below, top)` in degrees; top ±0 yields +90 when below ≥0, else -90     |
|          `0x27` | -       | Inclusive random signed integer from below through top                        |
|          `0x28` | -       | Negate top only when its signed value compares less than zero                 |
|    `0x29..0x2E` | -       | Signed integer `<`, `>`, `<=`, `>=`, `==`, `!=`                               |
| `0x2F` / `0x30` | -       | Boolean AND / OR using zero/nonzero truth                                     |
| `0x31` / `0x32` | -       | Boolean NOT / XOR                                                             |
|    `0x33..0x38` | -       | Float `<`, `>`, `<=`, `>=`, `==`, `!=`                                        |
|    `0x39..0x3B` | -       | Signed integer bitwise AND, OR, XOR                                           |
|          `0x3C` | `u8`    | Duplicate stack entry at zero-based depth                                     |
|          `0xFF` | `u8`    | Reserved; panics as not implemented                                           |
|           other | -       | Report the opcode and panic                                                   |

Binary operators pop the top value and update the value below it. Most
malformed stack uses assert, but `POP` can empty the stack without an error.
Jumps have no target or instruction-boundary check.

### LIMIT subtypes

| Low subtype | Load conversion    | Constraint            |
| ----------: | ------------------ | --------------------- |
|         `1` | Degrees to radians | Minimum rotation X    |
|         `2` | Degrees to radians | Maximum rotation X    |
|         `3` | Degrees to radians | Minimum rotation Y    |
|         `4` | Degrees to radians | Maximum rotation Y    |
|         `5` | Degrees to radians | Minimum rotation Z    |
|         `6` | Degrees to radians | Maximum rotation Z    |
|         `7` | None               | Minimum translation X |
|         `8` | None               | Maximum translation X |
|         `9` | None               | Minimum translation Y |
|        `10` | None               | Maximum translation Y |
|        `11` | None               | Minimum translation Y |
|        `12` | None               | Maximum translation Y |

The game applies subtypes `11` and `12` to translation Y, the same as `9` and
`10`. HSDLib names them minimum and maximum translation Z.

HSDLib differs from the game in two more places. It masks the limit subtype to
the low 24 bits, where the game uses the low 28. Its bytecode accessor reads a
byte where the game stores a bytecode pointer.

Sources: [`HSD_RObjDesc` and flags](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.h#L12-L88),
[RObj loader](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L607-L650),
[expression/Rvalue loading and reference resolution](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L851-L918),
[Rvalue extraction and expression evaluation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L697-L819),
[argument-count population helper](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/util.c#L19-L29),
the [`HSD_ByteCodeEval` VM](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/bytecode.c#L34-L545),
[JObj expression receiver semantics](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L355-L423),
[world rotation/translation/scale decomposition](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mtx.c#L220-L323),
[matrix-time RObj evaluation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L1438-L1444),
[JObj identity resolution](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L579-L601),
[active-gated consumers](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L52-L77),
[active-bit animation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L80-L98),
[limit application](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L448-L550),
and [HSDLib layout and limit names](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_ROBJ.cs#L5-L163).

## Matrix (3×4)

The matrix is affine. Its nine linear elements can hold scale as well as
rotation.

| Offset | Size | Format | Description            |
| ------ | ---: | ------ | ---------------------- |
| `0x00` |    4 | float  | Matrix row 1, column 1 |
| `0x04` |    4 | float  | Matrix row 1, column 2 |
| `0x08` |    4 | float  | Matrix row 1, column 3 |
| `0x0C` |    4 | float  | Translation X          |
| `0x10` |    4 | float  | Matrix row 2, column 1 |
| `0x14` |    4 | float  | Matrix row 2, column 2 |
| `0x18` |    4 | float  | Matrix row 2, column 3 |
| `0x1C` |    4 | float  | Translation Y          |
| `0x20` |    4 | float  | Matrix row 3, column 1 |
| `0x24` |    4 | float  | Matrix row 3, column 2 |
| `0x28` |    4 | float  | Matrix row 3, column 3 |
| `0x2C` |    4 | float  | Translation Z          |

## Object Structure (`HSD_DObjDesc`)

| Offset | Size | Format  | Description           |
| ------ | ---: | ------- | --------------------- |
| `0x00` |    4 | pointer | Class-name pointer    |
| `0x04` |    4 | pointer | Next Object structure |
| `0x08` |    4 | pointer | Material structure    |
| `0x0C` |    4 | pointer | Mesh structure        |

## Material Structure (`HSD_MObjDesc`)

| Offset | Size | Format  | Description                                          |
| ------ | ---: | ------- | ---------------------------------------------------- |
| `0x00` |    4 | pointer | Class-name pointer                                   |
| `0x04` |    4 | bitmask | `HSD_MObjDesc.rendermode` (`RENDER_*`)               |
| `0x08` |    4 | pointer | Texture structure                                    |
| `0x0C` |    4 | pointer | Color structure                                      |
| `0x10` |    4 | pointer | `renderdesc`; the base Melee loader does not read it |
| `0x14` |    4 | pointer | Pixel Processing structure                           |

### MObj render-mode values

|            Value or mask | Name(s)                                 | Meaning                                                          |
| -----------------------: | --------------------------------------- | ---------------------------------------------------------------- |
|             `0x00000003` | `RENDER_DIFFUSE_BITS`                   | Two-bit diffuse-source field mask                                |
|             `0x00000000` | `RENDER_DIFFUSE_MAT0`                   | Zero field value                                                 |
|             `0x00000001` | `RENDER_DIFFUSE_MAT`, `RENDER_CONSTANT` | Overlapping field value / channel bit                            |
|             `0x00000002` | `RENDER_DIFFUSE_VTX`, `RENDER_VERTEX`   | Overlapping field value / vertex channel bit                     |
|             `0x00000003` | `RENDER_DIFFUSE_BOTH`                   | Combined diffuse-source field value                              |
|             `0x00000004` | `RENDER_DIFFUSE`                        | Diffuse-lighting channel bit                                     |
|             `0x00000008` | `RENDER_SPECULAR`                       | Specular-lighting channel bit                                    |
|             `0x0000000F` | `CHANNEL_FIELD`                         | Mask of constant, vertex, diffuse, and specular bits             |
| `0x00000010..0x00000800` | `RENDER_TEX0..7`                        | Eight texture-presence bits                                      |
|             `0x00000FF0` | `RENDER_TEXTURES`                       | Mask of all texture bits                                         |
|             `0x00001000` | `RENDER_TOON`                           | Toon texture path; the base loader sets this on the runtime MObj |
|             `0x00006000` | `RENDER_ALPHA_BITS`                     | Two-bit alpha-source field mask                                  |
|             `0x00000000` | `RENDER_ALPHA_COMPAT`                   | Zero alpha-source field value                                    |
|             `0x00002000` | `RENDER_ALPHA_MAT`                      | Material alpha field value                                       |
|             `0x00004000` | `RENDER_ALPHA_VTX`                      | Vertex alpha field value                                         |
|             `0x00006000` | `RENDER_ALPHA_BOTH`                     | Combined alpha field value                                       |
|             `0x04000000` | `RENDER_SHADOW`                         | Enables the shadow-texture path when a global shadow TObj exists |
|             `0x08000000` | `RENDER_ZMODE_ALWAYS`                   | Selects GX `ALWAYS` rather than `LEQUAL` in default PE setup     |
|             `0x20000000` | `RENDER_NO_ZUPDATE`                     | Disables depth-buffer updates in default PE setup                |
|             `0x40000000` | `RENDER_XLU`                            | Enables blending in default PE setup                             |
|             `0x60000000` | `RENDER_BLENDING`                       | Mask/combination of XLU and NO_ZUPDATE                           |

The low names are aliases, not independent fields. The TEV builder tests
`RENDER_VERTEX`, `RENDER_DIFFUSE`, and `RENDER_SPECULAR` individually, while
channel setup switches on `rendermode & 7`. No branch consumes bit 0 alone. The
alpha-source field is defined in the header, but no code in the Melee tree reads
it.

DObj loading accepts three values under `RENDER_BLENDING`: zero, XLU, and
XLU|NO_ZUPDATE. NO_ZUPDATE alone panics.

When the PE pointer at `0x14` is null, the render mode sets the pixel state: XLU
enables blending, NO_ZUPDATE disables depth writes, and ZMODE_ALWAYS selects the
GX `ALWAYS` depth comparison instead of `LEQUAL`. A non-null PE descriptor
supplies those states instead.

The base loader copies the serialized `rendermode`, then sets `RENDER_TOON` on
the runtime MObj whatever the serialized bit says.

HSDLib names bits 24, 25, 28, and 31 `ZOFST`, `EFFECT`, `DF_ALL`, and `USER`.
Melee's `mobj.h` does not define them.

Sources: [Melee render-mode definitions](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.h#L33-L71),
[MObj load and renderdesc omission](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L153-L165),
[TEV/channel-bit consumption](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L192-L306),
[shadow/toon texture paths](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L340-L375),
[channel and default PE setup](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/state.c#L146-L242),
[DObj blending classification](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/dobj.c#L179-L201),
and [HSDLib flag names](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_MOBJ.cs#L7-L36).

## Texture Structure (`HSD_TObjDesc`)

| Offset | Size | Format   | Description                |
| ------ | ---: | -------- | -------------------------- |
| `0x00` |    4 | pointer  | Class-name pointer         |
| `0x04` |    4 | pointer  | Next Texture structure     |
| `0x08` |    4 | unsigned | `GXTexMapID`               |
| `0x0C` |    4 | unsigned | `GXTexGenSrc`              |
| `0x10` |    4 | float    | Rotation X                 |
| `0x14` |    4 | float    | Rotation Y                 |
| `0x18` |    4 | float    | Rotation Z                 |
| `0x1C` |    4 | float    | Scale X                    |
| `0x20` |    4 | float    | Scale Y                    |
| `0x24` |    4 | float    | Scale Z                    |
| `0x28` |    4 | float    | Translation X              |
| `0x2C` |    4 | float    | Translation Y              |
| `0x30` |    4 | float    | Translation Z              |
| `0x34` |    4 | unsigned | Wrap S                     |
| `0x38` |    4 | unsigned | Wrap T                     |
| `0x3C` |    1 | unsigned | Repeat S                   |
| `0x3D` |    1 | unsigned | Repeat T                   |
| `0x3E` |    2 | unsigned | Padding                    |
| `0x40` |    4 | bitmask  | `HSD_TObjDesc.blend_flags` |
| `0x44` |    4 | float    | Blending                   |
| `0x48` |    4 | unsigned | Mag filter (`GXTexFilter`) |
| `0x4C` |    4 | pointer  | Image structure            |
| `0x50` |    4 | pointer  | Palette structure          |
| `0x54` |    4 | pointer  | LOD structure              |
| `0x58` |    4 | pointer  | TEV structure              |

### TObj blend-flag fields

The coordinate selector, color-map selector, and alpha-map selector are fields,
not independent bits. The light-map roles and BUMP are independent bits.

| Field/mask              |     Value | Name                                                                                                | Meaning                                         |
| ----------------------- | --------: | --------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Coordinate `0x0000000F` |       `0` | `TEX_COORD_UV`                                                                                      | UV path                                         |
|                         |       `1` | `TEX_COORD_REFLECTION`                                                                              | Reflection matrix/normal source path            |
|                         |       `2` | `TEX_COORD_HILIGHT`                                                                                 | Highlight matrix/normal source path             |
|                         |       `3` | `TEX_COORD_SHADOW`                                                                                  | Shadow position/matrix path                     |
|                         |       `4` | `TEX_COORD_TOON`                                                                                    | Special toon resource and SRTG path             |
|                         |       `5` | `TEX_COORD_GRADATION`                                                                               | Defined; no dedicated branch in the Melee tree  |
|                         |       `6` | `TEX_COORD_BACKLIGHT`                                                                               | Defined; no dedicated branch in the Melee tree  |
| Light-map `0x000001F0`  |  `1 << 4` | `TEX_LIGHTMAP_DIFFUSE`                                                                              | Diffuse material-expression pass                |
|                         |  `1 << 5` | `TEX_LIGHTMAP_SPECULAR`                                                                             | Specular material-expression pass               |
|                         |  `1 << 6` | `TEX_LIGHTMAP_AMBIENT`                                                                              | Shares the first diffuse/ambient pass           |
|                         |  `1 << 7` | `TEX_LIGHTMAP_EXT`                                                                                  | Extension material-expression pass              |
|                         |  `1 << 8` | `TEX_LIGHTMAP_SHADOW`                                                                               | Volatile shadow-modulation TEV path             |
| Color map `0x000F0000`  |    `0..8` | `TEX_COLORMAP_NONE`, `ALPHA_MASK`, `RGB_MASK`, `BLEND`, `MODULATE`, `REPLACE`, `PASS`, `ADD`, `SUB` | Color operation; any other field value asserts  |
| Alpha map `0x00F00000`  |    `0..7` | `TEX_ALPHAMAP_NONE`, `ALPHA_MASK`, `BLEND`, `MODULATE`, `REPLACE`, `PASS`, `ADD`, `SUB`             | Alpha operation; any other field value asserts  |
| Other                   | `1 << 24` | `TEX_BUMP`                                                                                          | Bump resource/coordinate and volatile TEV paths |
|                         | `1 << 31` | `TEX_MTX_DIRTY`                                                                                     | Runtime matrix-dirty bit                        |

The loader copies the serialized `blend_flags`, then sets `TEX_MTX_DIRTY` on the
runtime TObj. For non-TOON coordinates, matrix setup clears that bit after it
rebuilds the texture matrix; TOON returns before either step. Resource
assignment handles reflection, highlight, shadow, UV, toon, and bump separately.

`MObjMakeTExp` schedules TObjs by light-map role: diffuse/ambient, then
specular, then EXT. A separate volatile pass handles SHADOW and BUMP. A TObj
with a zero light-map field has no diffuse role.

HSDLib's flag and coordinate enums stop at GRADATION and omit
`TEX_COORD_BACKLIGHT`.

Sources: [Melee flag definitions](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.h#L86-L131),
[TObj load/runtime dirty bit](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L252-L278),
[matrix and coordinate dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L410-L566),
[color/alpha operation switches](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L932-L1057),
[resource assignment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L1059-L1130),
[material light-map scheduling](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L237-L335),
[volatile shadow/bump TEV](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L618-L633),
and [HSDLib flags/enums](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_TOBJ.cs#L9-L55).

## Image Structure

| Offset | Size | Format   | Description                   |
| ------ | ---: | -------- | ----------------------------- |
| `0x00` |    4 | pointer  | Image data offset             |
| `0x04` |    2 | unsigned | Width                         |
| `0x06` |    2 | unsigned | Height                        |
| `0x08` |    4 | unsigned | Image format; see table below |
| `0x0C` |    4 | unsigned | Mipmap (`GXBool`)             |
| `0x10` |    4 | float    | Min LOD                       |
| `0x14` |    4 | float    | Max LOD                       |

### Image Format Values

| Format | Stride                                            | Description                                                           |
| ------ | ------------------------------------------------- | --------------------------------------------------------------------- |
| `0x0`  | `0x1` per 2 pixels                                | I4                                                                    |
| `0x1`  | `0x1`                                             | I8                                                                    |
| `0x2`  | `0x1`                                             | IA4                                                                   |
| `0x3`  | `0x2`                                             | IA8                                                                   |
| `0x4`  | `0x2`                                             | RGB565                                                                |
| `0x5`  | `0x2`                                             | RGB5A3: `1RRRRRGGGGGBBBBB` opaque RGB555 or `0AAARRRRGGGGBBBB` A3RGB4 |
| `0x6`  | `0x4`                                             | RGBA8                                                                 |
| `0x8`  | `0x1` per 2 pixels                                | CI4, 4-bit color index                                                |
| `0x9`  | `0x1`                                             | CI8                                                                   |
| `0xA`  | `0x2`                                             | CI14x2                                                                |
| `0xE`  | `0x8` per 4×4 pixels (`0x20` per tiled 8×8 block) | CMPR; see below                                                       |

In RGB5A3, bit 15 of each pixel selects the layout.

CMPR is a 4-bit-per-pixel block compression close to S3TC/DXT1, not a palette
format. An 8×8 tile holds four 4×4 sub-blocks of eight bytes each. A sub-block
stores two RGB565 endpoints and sixteen 2-bit selectors. The other two selector
colors are derived from the endpoints. In the opaque mode they blend the
endpoints 5/8 and 3/8. In the transparent mode one is the endpoint average and
the other is that average with zero alpha.

Sources: [GX texture-format enum](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/extern/dolphin/include/dolphin/gx/GXEnum.h#L132-L145)
and [Dolphin's CMPR block addressing and decode](https://github.com/dolphin-emu/dolphin/blob/430138f468effe6bf396adf3cd4d46df4cbd9050/Source/Core/VideoCommon/TextureDecoder_Common.cpp#L552-L618).

## Palette Structure

| Offset | Size | Format   | Description                             |
| ------ | ---: | -------- | --------------------------------------- |
| `0x00` |    4 | pointer  | Palette/TLUT data offset                |
| `0x04` |    4 | unsigned | Palette format (`GXTlutFmt`); see below |
| `0x08` |    4 | unsigned | Name (`GXTlut`)                         |
| `0x0C` |    2 | unsigned | Color count                             |
| `0x0E` |    2 | unsigned | Padding                                 |

### Palette Format Values

| Format | Stride | Description                                                           |
| ------ | ------ | --------------------------------------------------------------------- |
| `0x0`  | `0x2`  | IA8                                                                   |
| `0x1`  | `0x2`  | RGB565                                                                |
| `0x2`  | `0x2`  | RGB5A3: `1RRRRRGGGGGBBBBB` opaque RGB555 or `0AAARRRRGGGGBBBB` A3RGB4 |

## LOD Structure

| Offset | Size | Format   | Description                     |
| ------ | ---: | -------- | ------------------------------- |
| `0x00` |    4 | unsigned | Min filter (`GXTexFilter`)      |
| `0x04` |    4 | float    | LOD bias                        |
| `0x08` |    1 | unsigned | Bias clamp (`GXBool`)           |
| `0x09` |    1 | unsigned | Edge LOD enable (`GXBool`)      |
| `0x0A` |    2 | unsigned | Padding                         |
| `0x0C` |    4 | unsigned | Max anisotropy (`GXAnisotropy`) |

## TEV Structure (Texture Environment)

Size: `0x20`. The operation, bias, and scale bytes are GX enums, not booleans.

| Offset | Size | Format              | Description                                               |
| ------ | ---: | ------------------- | --------------------------------------------------------- |
| `0x00` |    1 | enum (`GXTevOp`)    | Color op                                                  |
| `0x01` |    1 | enum (`GXTevOp`)    | Alpha op                                                  |
| `0x02` |    1 | enum (`GXTevBias`)  | Color bias                                                |
| `0x03` |    1 | enum (`GXTevBias`)  | Alpha bias                                                |
| `0x04` |    1 | enum (`GXTevScale`) | Color scale                                               |
| `0x05` |    1 | enum (`GXTevScale`) | Alpha scale                                               |
| `0x06` |    1 | bool (`GXBool`)     | Color clamp                                               |
| `0x07` |    1 | bool (`GXBool`)     | Alpha clamp                                               |
| `0x08` |    1 | selector            | Color A; accepted values below                            |
| `0x09` |    1 | selector            | Color B; accepted values below                            |
| `0x0A` |    1 | selector            | Color C; accepted values below                            |
| `0x0B` |    1 | selector            | Color D; accepted values below                            |
| `0x0C` |    1 | selector            | Alpha A; accepted values below                            |
| `0x0D` |    1 | selector            | Alpha B; accepted values below                            |
| `0x0E` |    1 | selector            | Alpha C; accepted values below                            |
| `0x0F` |    1 | selector            | Alpha D; accepted values below                            |
| `0x10` |    4 | `GXColor`           | RGBA `konst` register                                     |
| `0x14` |    4 | `GXColor`           | RGBA `tev0` register                                      |
| `0x18` |    4 | `GXColor`           | RGBA `tev1` register                                      |
| `0x1C` |    4 | bitmask             | Declared register components plus color/alpha TEV enables |

### TObj TEV enum, selector, and active values

The loader copies all `0x20` bytes without normalization. Operation, bias, and
scale use the GX ordinals:

| Field        | Values                                                                                                                                       |
| ------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `GXTevOp`    | `0 ADD`, `1 SUB`, `8 R8_GT`, `9 R8_EQ`, `10 GR16_GT`, `11 GR16_EQ`, `12 BGR24_GT`, `13 BGR24_EQ`, `14 RGB8_GT`/`A8_GT`, `15 RGB8_EQ`/`A8_EQ` |
| `GXTevBias`  | `0 ZERO`, `1 ADDHALF`, `2 SUBHALF`                                                                                                           |
| `GXTevScale` | `0 SCALE_1`, `1 SCALE_2`, `2 SCALE_4`, `3 DIVIDE_2`                                                                                          |

The custom color-expression switch accepts five ordinary GX color arguments plus
HSD's high-bit selectors:

|          Value | Color input                                                                                  |
| -------------: | -------------------------------------------------------------------------------------------- |
|            `8` | `GX_CC_TEXC`                                                                                 |
|            `9` | `GX_CC_TEXA`                                                                                 |
|           `12` | `GX_CC_ONE`                                                                                  |
|           `13` | `GX_CC_HALF`                                                                                 |
|           `15` | `GX_CC_ZERO`                                                                                 |
| `0x80`..`0x84` | `KONST_RGB`, `KONST_RRR`, `KONST_GGG`, `KONST_BBB`, `KONST_AAA`                              |
| `0x85`..`0x88` | `TEX0_RGB`, `TEX0_AAA`, `TEX1_RGB`, `TEX1_AAA` from the descriptor's `tev0`/`tev1` registers |

The alpha switch accepts `GX_CA_TEXA = 4`, `GX_CA_ZERO = 7`, HSD
`KONST_R/G/B/A = 0x40..0x43`, and `TEX0_A/TEX1_A = 0x44..0x45`. Any other
selector asserts when its color or alpha expression is enabled.

HSD's `TEX0` and `TEX1` selectors read the descriptor's `tev0` and `tev1`
constant registers. The sampled texture comes from the GX `TEXC` and `TEXA`
inputs.

|   Active mask | Meaning                                                       |
| ------------: | ------------------------------------------------------------- |
|   bits `0..3` | Declared `KONST_R/G/B/A` components                           |
|   bits `4..7` | Declared `TEV0_R/G/B/A` components                            |
|  bits `8..11` | Declared `TEV1_R/G/B/A` components                            |
| bits `12..29` | Not named                                                     |
|     `1 << 30` | Build the custom color expression and consume color op/inputs |
|     `1 << 31` | Build the custom alpha expression and consume alpha op/inputs |

The TObj expression path tests only bits 30 and 31. If either is set, setup
first scans both selector arrays for referenced register constants. Then each
enabled side dispatches its selector switch and operation; a disabled side does
not. Bits 0..11 are declared but nothing in that path reads them.

When both enable bits are clear, the ordinary TObj color-map and alpha-map
behavior applies, and the operation and input bytes are not read.

An enabled side is lowered to a GX TEV stage. With `ADD`, zero bias, and
`SCALE_1`, the stage computes `(A * (1 - C) + B * C) + D`, clamped when the
clamp byte is set.

Texture animation channels 12..23 change the three RGBA registers. They do not
set active bits.

Sources: [selector/active declarations](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.h#L45-L84),
[serialized/runtime layout](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.h#L215-L243),
[descriptor copy](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L267-L317),
[selector and active dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L636-L930),
[expression-to-TEV lowering](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/texp.c#L911-L1023),
[GX state issue](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tev.c#L226-L237),
[texture animation callback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L136-L227),
and [GX TEV enums](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/extern/dolphin/include/dolphin/gx/GXEnum.h#L552-L629).

## Color Structure

`HSD_Material`.

| Offset |  Size | Format   | Description           |
| ------ | ----: | -------- | --------------------- |
| `0x00` | 1 × 4 | unsigned | RGBA ambient          |
| `0x04` | 1 × 4 | unsigned | RGBA diffuse          |
| `0x08` | 1 × 4 | unsigned | RGBA specular         |
| `0x0C` |     4 | float    | Alpha; `1.0` = opaque |
| `0x10` |     4 | float    | Shininess             |

Source: [`HSD_Material`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.h#L84-L90).

## Pixel Processing Structure

`HSD_PEDesc`. Size: `0x0C`.

| Offset | Size | Format   | Description                          |
| ------ | ---: | -------- | ------------------------------------ |
| `0x00` |    1 | bitmask  | `HSD_PEDesc.flags`                   |
| `0x01` |    1 | unsigned | Alpha Ref0                           |
| `0x02` |    1 | unsigned | Alpha Ref1                           |
| `0x03` |    1 | unsigned | Destination alpha                    |
| `0x04` |    1 | unsigned | Type (`GXBlendMode`)                 |
| `0x05` |    1 | unsigned | Source factor (`GXBlendFactor`)      |
| `0x06` |    1 | unsigned | Destination factor (`GXBlendFactor`) |
| `0x07` |    1 | unsigned | Blend op (`GXLogicOp`)               |
| `0x08` |    1 | unsigned | Depth function (`GXCompare`)         |
| `0x09` |    1 | unsigned | Alpha Comp0 (`GXCompare`)            |
| `0x0A` |    1 | unsigned | Alpha op (`GXAlphaOp`)               |
| `0x0B` |    1 | unsigned | Alpha Comp1 (`GXCompare`)            |

### PE flag and GX enum values

Melee has no named macros for the PE flags; `HSD_SetupPEMode` tests each bit
directly. The names below are HSDLib's.

|     Mask | HSDLib name    | Effect                                       |
| -------: | -------------- | -------------------------------------------- |
| `1 << 0` | `COLOR_UPDATE` | Enable color-buffer updates                  |
| `1 << 1` | `ALPHA_UPDATE` | Enable alpha-buffer updates                  |
| `1 << 2` | `DST_ALPHA`    | Enable destination alpha and use `dst_alpha` |
| `1 << 3` | `BEFORE_TEX`   | Pass true to GX depth-compare-location state |
| `1 << 4` | `COMPARE`      | Enable depth comparison and use `z_comp`     |
| `1 << 5` | `ZUPDATE`      | Enable depth-buffer updates                  |
| `1 << 6` | `DITHER`       | Enable dithering                             |

Bit 7 has no name and `HSD_SetupPEMode` does not read it. The remaining bytes
go to GX state setters with their declared enum types:

| Field           | Ordinal values                                                                                                                                                           |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GXBlendMode`   | `0 NONE`, `1 BLEND`, `2 LOGIC`, `3 SUBTRACT`                                                                                                                             |
| `GXBlendFactor` | `0 ZERO`, `1 ONE`, `2 SRCCLR`/`DSTCLR`, `3 INVSRCCLR`/`INVDSTCLR`, `4 SRCALPHA`, `5 INVSRCALPHA`, `6 DSTALPHA`, `7 INVDSTALPHA`                                          |
| `GXLogicOp`     | `0 CLEAR`, `1 AND`, `2 REVAND`, `3 COPY`, `4 INVAND`, `5 NOOP`, `6 XOR`, `7 OR`, `8 NOR`, `9 EQUIV`, `10 INV`, `11 REVOR`, `12 INVCOPY`, `13 INVOR`, `14 NAND`, `15 SET` |
| `GXCompare`     | `0 NEVER`, `1 LESS`, `2 EQUAL`, `3 LEQUAL`, `4 GREATER`, `5 NEQUAL`, `6 GEQUAL`, `7 ALWAYS`                                                                              |
| `GXAlphaOp`     | `0 AND`, `1 OR`, `2 XOR`, `3 XNOR`                                                                                                                                       |

The MObj loader copies a non-null descriptor into runtime-owned memory. A null
PE pointer selects the default state described under
[MObj render-mode values](#mobj-render-mode-values).

Sources: [serialized/runtime structures](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.h#L92-L114),
[MObj PE copy](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L153-L165),
[PE state setup](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/state.c#L206-L243),
[GX compare/alpha enums](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/extern/dolphin/include/dolphin/gx/GXEnum.h#L20-L39),
[GX blend/factor/logic enums](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/extern/dolphin/include/dolphin/gx/GXEnum.h#L347-L388),
and [HSDLib PE flag names](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_MOBJ.cs#L38-L48).

## Mesh Structure (`HSD_PObjDesc`)

| Offset | Size | Format   | Description                                                                          |
| ------ | ---: | -------- | ------------------------------------------------------------------------------------ |
| `0x00` |    4 | pointer  | Class-name pointer                                                                   |
| `0x04` |    4 | pointer  | Next Mesh structure                                                                  |
| `0x08` |    4 | pointer  | Mesh Attribute structure array; ends at the entry with `CP_ID == 0xFF`               |
| `0x0C` |    2 | bitmask  | `HSD_PObjDesc.flags` (type and cull fields)                                          |
| `0x0E` |    2 | unsigned | Display-list size in 32-byte units                                                   |
| `0x10` |    4 | pointer  | Display-list data offset                                                             |
| `0x14` |    4 | pointer  | Flag-selected union: rigid/skin JObj, ShapeSet descriptor, or envelope-pointer array |

### PObj type and culling fields

`pobj_type` masks bits 12..13. The type selects both the `0x14` union member and
the matrix and display behavior:

| Masked value | Name             | `0x14` interpretation and behavior                                                   |
| -----------: | ---------------- | ------------------------------------------------------------------------------------ |
|     `0x0000` | `POBJ_SKIN`      | JObj reference; null uses rigid model matrices, non-null uses shared-vertex matrices |
|     `0x1000` | `POBJ_SHAPEANIM` | `HSD_ShapeSetDesc`; rigid model matrices and shape-animation drawing                 |
|     `0x2000` | `POBJ_ENVELOPE`  | Null-terminated envelope-pointer array; envelope matrix palette                      |
|     `0x3000` | none             | Invalid; the loader panics. It is not an envelope                                    |

`HSD_PObjDisp` masks bits 14..15 independently:

|                Masked value | Effect                 |
| --------------------------: | ---------------------- |
|                    `0x0000` | `GX_CULL_NONE`         |
| `0x4000` (`POBJ_CULLFRONT`) | `GX_CULL_FRONT`        |
|  `0x8000` (`POBJ_CULLBACK`) | `GX_CULL_BACK`         |
|                    `0xC000` | Return without drawing |

HSDLib reverses the names of the two culling bits.

The game defines no meaning for bits 0..11 and no branch reads them from the
serialized flags. HSDLib labels bits 0 and 1 `SHAPESET_AVERAGE` and
`SHAPESET_ADDITIVE`, bit 2 `UNKNOWN2`, and bit 3 `ANIM`. In the game, average
and additive belong to the separate `HSD_ShapeSetDesc.flags`, and `POBJ_ANIM`
bit 3 is a flag in animation-request function arguments, not in the serialized
PObj flags.

Sources: [constants and type mask](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/forward.h#L115-L131),
[serialized/runtime layouts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.h#L24-L100),
[ShapeSet flags/load/update](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L137-L282),
[loader union dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L285-L306),
[matrix and culling dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L1199-L1256),
[animation-request mask use](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L46-L123),
and [HSDLib labels](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/HSD_POBJ.cs#L8-L19).

## ShapeSet Structure (`HSD_ShapeSetDesc`)

Size: `0x1C`. A PObj of type `0x1000` interprets its `0x14` union member as this
descriptor:

| Offset | Size | Format   | Description                                                                  |
| ------ | ---: | -------- | ---------------------------------------------------------------------------- |
| `0x00` |    2 | bitmask  | ShapeSet flags                                                               |
| `0x02` |    2 | unsigned | `nb_shape`; absolute-shape count or additive-vector count, depending on mode |
| `0x04` |    4 | signed   | Number of blended position-index slots                                       |
| `0x08` |    4 | pointer  | Position `HSD_VtxDescList` descriptor                                        |
| `0x0C` |    4 | pointer  | Array of pointers to per-shape position-index arrays                         |
| `0x10` |    4 | signed   | Number of blended normal/NBT-index slots                                     |
| `0x14` |    4 | pointer  | Optional normal or NBT `HSD_VtxDescList` descriptor                          |
| `0x18` |    4 | pointer  | Array of pointers to per-shape normal/NBT-index arrays                       |

The ShapeSet flag word is separate from `HSD_PObjDesc.flags`. The game names two
bits:

| Mask | Name                | Behavior                                                                                      |
| ---: | ------------------- | --------------------------------------------------------------------------------------------- |
|  `1` | `SHAPESET_AVERAGE`  | `nb_shape` absolute shapes; one scalar selects and interpolates adjacent shapes               |
|  `2` | `SHAPESET_ADDITIVE` | One base shape plus `nb_shape` additive vectors; one independently animated weight per vector |

Loading and update code choose additive storage when bit 1 is set. Drawing
chooses the average branch when bit 0 is set and the additive branch otherwise.
The game does not reject a word with neither or both bits, although storage and
drawing then disagree.

Every ShapeSet in the game's files has flags `0x0005`: average mode with bit 2
set. The meaning of bit 2 is unknown. Additive mode and NBT normals are not
observed in the game's files.

Average mode reads `nb_shape` index-array pointers. For blend scalar `b`,
drawing clamps an integer shape index to `0..nb_shape-1`, clamps the fractional
remainder to `0..1`, and linearly interpolates that shape with the next shape,
also clamped.

Additive mode reads `nb_shape + 1` pointers: base shape 0 plus vectors
`1..nb_shape`. Each vector is multiplied by `max(0, weight)` and added to the
base. Positive weights have no upper clamp, and the base is not subtracted from
a vector first.

Positions, three-component normals, and all nine NBT components use the same
componentwise equations. Blended normals are not renormalized.

Each per-shape position array has `nb_vertex_index` unsigned indices; each
normal/NBT array has `nb_normal_index`. An entry is a big-endian `u16` when the
ShapeSet attribute descriptor says `GX_INDEX16`, and one byte otherwise. The
mapped attribute is XYZ position, XYZ normal, or NBT, stored as `F32`, `U8`,
`S8`, `U16`, or `S16` with the descriptor's fractional scale.

Display-list position and normal indices address the generated output arrays,
not the source attribute buffers. Runtime assertions cap output positions at
2,000 vectors and output normal storage at 2,000 vectors; NBT uses three output
vectors per index.

The game checks no mode, channel range, or index range. HSDLib reads all index
maps as signed 16-bit values; the game does not.

Sources: [serialized and runtime ShapeSet layouts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.h#L75-L110),
[flags, allocation, and callback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L137-L282),
[index decoding and attribute requirements](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L483-L704),
and [blend equations and runtime limits](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L835-L958).

## Mesh Attribute Structure

| Offset | Size | Format   | Description                                        |
| ------ | ---: | -------- | -------------------------------------------------- |
| `0x00` |    4 | unsigned | `CP_ID`; see enum table                            |
| `0x04` |    4 | unsigned | Component type; see enum table                     |
| `0x08` |    4 | unsigned | Component count; meaning depends on attribute      |
| `0x0C` |    4 | unsigned | Data type; see enum table                          |
| `0x10` |    1 | unsigned | Divisor / floating-point exponent for integer data |
| `0x11` |    1 | padding  | Alignment byte                                     |
| `0x12` |    2 | unsigned | Stride                                             |
| `0x14` |    4 | pointer  | Data offset                                        |

A non-float vector component decodes as:

```text
value / pow(2.0, Exponent)
```

### `CP_ID` Values

| Enum   | Description                          |
| ------ | ------------------------------------ |
| `0x0`  | Position/Normal Influence Matrix ID  |
| `0x1`  | UV[0] Influence Matrix ID            |
| `0x2`  | UV[1] Influence Matrix ID            |
| `0x3`  | UV[2] Influence Matrix ID            |
| `0x4`  | UV[3] Influence Matrix ID            |
| `0x5`  | UV[4] Influence Matrix ID            |
| `0x6`  | UV[5] Influence Matrix ID            |
| `0x7`  | UV[6] Influence Matrix ID            |
| `0x8`  | UV[7] Influence Matrix ID            |
| `0x9`  | Vertex ID / value                    |
| `0xA`  | Normal ID / value                    |
| `0xB`  | Color[0] ID / value                  |
| `0xC`  | Color[1] ID / value                  |
| `0xD`  | UV[0] ID / value                     |
| `0xE`  | UV[1] ID / value                     |
| `0xF`  | UV[2] ID / value                     |
| `0x10` | UV[3] ID / value                     |
| `0x11` | UV[4] ID / value                     |
| `0x12` | UV[5] ID / value                     |
| `0x13` | UV[6] ID / value                     |
| `0x14` | UV[7] ID / value                     |
| `0x15` | Vertex Influence Matrix Array offset |
| `0x16` | Normal Influence Matrix Array offset |
| `0x17` | UV Influence Matrix Array offset     |
| `0x18` | Light Influence Matrix Array offset  |
| `0x19` | NBT ID / value                       |
| `0xFF` | NULL / terminator                    |

### Component Type Values

| Enum  | Description                                 |
| ----- | ------------------------------------------- |
| `0x0` | None                                        |
| `0x1` | Direct; value is stored instead of an index |
| `0x2` | 8-bit index                                 |
| `0x3` | 16-bit index                                |

### Component Count Values

The same numeric value means something different for each attribute.

#### Position

| Enum  | Description  |
| ----- | ------------ |
| `0x0` | XY position  |
| `0x1` | XYZ position |

#### Normal

| Enum  | Description                   |
| ----- | ----------------------------- |
| `0x0` | Normal                        |
| `0x1` | Normal + bi-normal + tangent  |
| `0x2` | Normal, bi-normal, or tangent |

#### Color

| Enum  | Description |
| ----- | ----------- |
| `0x0` | RGB color   |
| `0x1` | RGBA color  |

#### Texture Coordinate

| Enum  | Description    |
| ----- | -------------- |
| `0x0` | S coordinate   |
| `0x1` | ST coordinates |

### Data Type Values

| Enum  | Description     |
| ----- | --------------- |
| `0x0` | Unsigned 8-bit  |
| `0x1` | Signed 8-bit    |
| `0x2` | Unsigned 16-bit |
| `0x3` | Signed 16-bit   |
| `0x4` | Float           |

### Color Format Values

| Enum  | Description                         |
| ----- | ----------------------------------- |
| `0x0` | RGB565 — R5 G6 B5                   |
| `0x1` | RGB8 — R8 G8 B8                     |
| `0x2` | RGBX8 — R8 G8 B8 + discarded 8 bits |
| `0x3` | RGBA4 — R4 G4 B4 A4                 |
| `0x4` | RGBA6 — R6 G6 B6 A6                 |
| `0x5` | RGBA8 — R8 G8 B8 A8                 |

## Envelope Position/Normal Influence Matrix Array

| Offset | Size | Format  | Description             |
| ------ | ---: | ------- | ----------------------- |
| `0x00` |    4 | pointer | Weight Structure array  |
| `0x##` |    4 | end     | `0x00000000` terminator |

This null-terminated pointer array is the `0x14` union member only for a PObj of
type `POBJ_ENVELOPE`. Each entry is one envelope: a list of weights that
produces one matrix in the palette. When the display list supplies the
position/normal matrix attribute (`CP_ID == 0`), the facepoint value divided by
`3` is the palette index.

## Weight Structure

| Offset | Size | Format  | Description           |
| ------ | ---: | ------- | --------------------- |
| `0x00` |    4 | pointer | Bone structure offset |
| `0x04` |    4 | float   | Weight                |

### Envelope matrices

For each weight, the game combines the referenced joint's current world matrix
with that joint's inverse-bind matrix from `HSD_Joint` `0x38`.

- **Weighted path.** The envelope matrix is
  `sum(weight × (current matrix × inverse bind))` in list order. Weights are
  not normalized. The model-node correction below is then applied.
- **Single-weight fast path.** Generic HSD takes it when
  `weight >= 1.0 - FLT_EPSILON`; Melee's fighter override requires
  `weight >= 1.0`. For a skeleton-root model node it uses the referenced
  joint's current matrix directly. Otherwise it composes current matrix ×
  inverse bind × the model-node correction.
- **Model-node correction** (`_HSD_mkEnvelopeModelNodeMtx`). It is the inverse
  envelope matrix when the model node is itself the nearest skeleton. It is
  inverse skeleton-current × model-current for a skeleton-root ancestor. It is
  inverse (skeleton-current × skeleton-envelope) × model-current for any other
  skeleton ancestor.
- **Palette.** `HSD_Index2PosNrmMtx` maps the ten palette entries to GX matrix
  selectors `0, 3, ... 27`.
- **Normals.** The draw path left-multiplies each result by the view matrix and
  loads `HSD_MtxInverseTranspose` of it into the matching GX normal slot. When
  the determinant magnitude is below `1e-10`, that helper copies the upper-left
  matrix instead of inverting it.

Sources: [generic palette construction and normal loading](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L1123-L1196),
[fighter envelope override](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftparts.c#L224-L297),
[model-node correction](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/displayfunc.c#L252-L283),
[GX selector mapping](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/util.c#L32-L59),
and [inverse-transpose implementation and singular fallback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mtx.c#L161-L204).

---

## SSBM Mesh Layout

```text
Root
└─ Bone
   ├─ Bone
   └─ Object
      ├─ Material
      │  ├─ Colors
      │  └─ Texture
      │     ├─ Image
      │     │  └─ Image Data
      │     ├─ Palette
      │     │  └─ Palette Data
      │     ├─ LOD
      │     ├─ TEV
      │     └─ Texture
      ├─ Mesh
      │  ├─ Attributes
      │  │  └─ Vector Data
      │  ├─ Flag-selected binding union
      │  │  ├─ Rigid/skin Joint
      │  │  ├─ ShapeSet
      │  │  └─ Envelope Pointer Array
      │  │     └─ Weight Array
      │  │        └─ Weight
      │  └─ Display List
      │     └─ Sub-vector data and/or indexes
      └─ Object
```

## Animation

Fighter animation data is in the `Pl**AJ.dat` archives. The animation structures
are in [`ssbm_hal_dat_animation_tables.md`](ssbm_hal_dat_animation_tables.md).
