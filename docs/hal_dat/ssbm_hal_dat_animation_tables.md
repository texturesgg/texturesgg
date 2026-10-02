# Super Smash Bros. Melee — HAL DAT animation tables

This file is a reference for HSD animation data and for how Super Smash Bros.
Melee NTSC 1.02 (GALE01 revision 2) evaluates it. Layouts and behavior come from
the doldecomp/melee decompilation, pinned to commit
[`90f83f6665648a73122146d981eec48f173b1ca3`](https://github.com/doldecomp/melee/tree/90f83f6665648a73122146d981eec48f173b1ca3),
which rebuilds the NTSC 1.02 `main.dol` byte for byte. Addresses of the form
`0x80……` are in that executable. The model structures that animation drives
(JObj, DObj, MObj, TObj, PObj, RObj, splines, ShapeSets) are in
[`ssbm_hal_dat_tables.md`](ssbm_hal_dat_tables.md).

Pointers are 32-bit, big-endian, relocated, and relative to the data section
unless stated otherwise. Packed FObj numeric payloads are a separate
little-endian byte stream.

## Keep these formats separate

Melee uses related but distinct animation representations:

1. **Generic HSD animation**: `HSD_AObjDesc` owns a linked list of
   `HSD_FObjDesc` tracks and is attached through AnimJoint, MatAnim, TexAnim, or
   ShapeAnim structures.
2. **Fighter Figa animation**: `Pl**AJ.dat` holds mini-archives of compact
   `FigaTree`/`FigaTrack` descriptors. The game loads their packed streams into
   runtime FObjs.
3. **Runtime objects**: `HSD_AObj` and `HSD_FObj` hold interpreter state. They
   are not serialized.

Both serialized forms share one packed FObj stream encoding and one
interpreter.

# Fighter compact animation

## FigaTree

**Size: `0x14`.**

| Offset | Size | Format  | Description                                                        |
| ------ | ---: | ------- | ------------------------------------------------------------------ |
| `0x00` |    4 | signed  | Type; bit 0 selects classical scale                                |
| `0x04` |    4 | bitmask | AObj flags passed to the runtime animation object                  |
| `0x08` |    4 | float   | End frame (`frames`)                                               |
| `0x0C` |    4 | pointer | Signed per-part track-count byte list, terminated by `-1` / `0xFF` |
| `0x10` |    4 | pointer | Contiguous `FigaTrack` descriptor array                            |

Source: [`lbanim.h`, `FigaTree`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.h#L10-L25).

`type & 1` selects `JOBJ_CLASSICAL_SCALE` when the tree is attached
([`lbanim.c`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L87-L135)).
The meaning of the other type bits is unknown. The game's fighter animations use
only type `0` and `1`, and their flags field is always zero.

### Track-count list and fighter parts

Each signed byte gives the number of consecutive FigaTracks one fighter part
consumes. `-1` terminates the list. A count of zero still consumes its part. The
byte's ordinal is a position in Melee's fighter-part traversal. It is not a JObj
index or a pointer.

A FigaTree does not store its source fighter kind, its destination model, the
part attachment starts from, or any remap table. The game combines the list with
data loaded elsewhere:

- `Fighter_LoadCommonData` loads the [part and auxiliary tables](#common-part-tables)
  from `PlCo.dat`.
- `ftParts_SetupParts` walks the costume JObj tree depth-first and assigns each
  physical JObj to the next fighter-part slot. It leaves the auxiliary slots
  empty and does not descend through an `INSTANCE` JObj's child pointer.
- Action selection copies the animation record's packed flags word into the
  fighter. That word carries the source fighter kind and the auxiliary-part
  enable mask. See [the animation table](#fighter-animation-table).
- Same-kind attachment walks the count list and the fighter parts in parallel.
  It skips a part that is not eligible but always advances the FigaTrack cursor
  by that part's count.
- Cross-kind attachment remaps each source part through the source kind's
  `joint_to_part` table and the destination kind's `part_to_joint` table.
- Partial attachment starts from a part the caller supplies and covers that
  part's stored subtree depth.

The list has one entry per part of the source kind's table: `parts_num`, minus
the auxiliary slots, plus the auxiliary slots the record's mask enables. For an
animation whose source kind is the playing fighter and whose mask is zero, the
entries are the costume's JObjs in depth-first order. Other animations can have
more or fewer entries than the destination model has joints.

For cross-kind attachment, destination part flag `b3` selects between two
compact-track loaders. The `b3`-false loader loads only the tracks that precede
the part's first translation track (types `5..7`). The outer cursor still
consumes the part's full count. The decompiled C leaves the result undefined
when the part's first track is a translation track.

Sources:

- [`fighter.c`, common-data load](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/fighter.c#L184-L202)
- [`fighter.c`, action flags copied into fighter state](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/fighter.c#L1248-L1256)
- [`ftparts.c`, costume-tree setup](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftparts.c#L392-L455)
- [`ftparts.c`, cross-kind remap](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftparts.c#L697-L716)
- [`ftanim.c`, full same-kind attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L598-L640)
- [`ftanim.c`, partial attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L642-L775)
- [`ftanim.c`, full cross-kind attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L864-L916)
- [`lbanim.c`, ordinary and prefix-truncating compact-track loaders](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L7-L65)

## FigaTrack

**Size: `0x0C`.**

| Offset | Size | Format        | Description                                      |
| ------ | ---: | ------------- | ------------------------------------------------ |
| `0x00` |    2 | unsigned      | Packed stream length in bytes                    |
| `0x02` |    2 | unsigned bits | Start frame; the loader copies it to an `s16`    |
| `0x04` |    1 | enum          | Context-specific object/channel type             |
| `0x05` |    1 | packed format | Value numeric format and fractional-bit exponent |
| `0x06` |    1 | packed format | Slope numeric format and fractional-bit exponent |
| `0x07` |    1 | padding       | Unused alignment byte                            |
| `0x08` |    4 | pointer       | Packed FObj command/data stream                  |

Sources:

- [`lbanim.h`, `FigaTrack`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.h#L10-L18)
- [`lbanim.c`, compact-track loader](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L7-L65)

The loader copies the `u16` start frame into runtime `HSD_FObj.startframe`,
which is `s16`. A value with the high bit set is therefore a negative start
frame. The game's fighter animations use negative start frames (`-180..-1`) on
a few BRANCH tracks and on no other channel.

`0x07` is alignment padding. It is not always zero in the game's files and has
no meaning.

Fighter tracks are interpreted by the JObj callback, so their types are
[JObj channels](#jobj-channels). The compact loaders attach no RObj animation
list.

# Generic HSD animation descriptors

## HSD_AObjDesc

**Size: `0x10`.**

| Offset | Size | Format                    | Description                  |
| ------ | ---: | ------------------------- | ---------------------------- |
| `0x00` |    4 | bitmask                   | Serialized AObj flags        |
| `0x04` |    4 | float                     | End frame                    |
| `0x08` |    4 | pointer                   | First `HSD_FObjDesc`         |
| `0x0C` |    4 | unsigned/pointer identity | Object ID used by the loader |

Source: [`aobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.h#L30-L61).

`HSD_AObjLoadDesc` uses `obj_id` to attach an auxiliary object to the AObj. It
looks the value up in the HSD object table as a descriptor identity and takes a
reference to the existing object. When the table has no entry, it loads the
value as an `HSD_Joint` pointer. The auxiliary object is separate from the
object the AObj animates. Only [PATH](#path-spline-motion) reads it.

### AObj flag values

|     Value | Name              | Meaning                              |
| --------: | ----------------- | ------------------------------------ |
| `1 << 26` | `AOBJ_REWINDED`   | Runtime state                        |
| `1 << 27` | `AOBJ_FIRST_PLAY` | Runtime state                        |
| `1 << 28` | `AOBJ_NO_UPDATE`  | Descriptor behavior retained on load |
| `1 << 29` | `AOBJ_LOOP`       | Descriptor behavior retained on load |
| `1 << 30` | `AOBJ_NO_ANIM`    | Runtime state                        |

`HSD_AObjSetFlags` masks loaded flags to `NO_UPDATE | LOOP`. The remaining bits
are runtime state. See [`aobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.h#L11-L15)
and [`aobj.c`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L38-L57).

## HSD_FObjDesc

**Size: `0x14`.**

| Offset | Size | Format        | Description                          |
| ------ | ---: | ------------- | ------------------------------------ |
| `0x00` |    4 | pointer       | Next FObj descriptor                 |
| `0x04` |    4 | unsigned      | Packed data length in bytes          |
| `0x08` |    4 | float         | Start frame                          |
| `0x0C` |    1 | enum          | Context-specific object/channel type |
| `0x0D` |    1 | packed format | Value format/exponent                |
| `0x0E` |    1 | packed format | Slope format/exponent                |
| `0x0F` |    1 | padding       | Dummy/alignment byte                 |
| `0x10` |    4 | pointer       | Packed FObj command/data stream      |

Source: [`fobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.h#L32-L63).

The [loader](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L456-L468)
narrows the `f32` start frame into runtime `HSD_FObj.startframe` (`s16`),
truncating toward zero.

# Packed FObj stream

A stream is the byte range the track descriptor names: a pointer and a length.
It has no terminator byte, clock, or loop marker.

## Numeric format byte

The value and slope format bytes come from the track descriptor. The high three
bits select storage and the low five bits select a denominator exponent. An
integer payload decodes to `numerator / 2^exponent`. Exponent `31` gives the
`s32` denominator `-2147483648`. Exponent `31` is not observed in the game's
files.

| High bits | Name               | Payload                 |
| --------: | ------------------ | ----------------------- |
|    `0x00` | `HSD_A_FRAC_FLOAT` | 32-bit float            |
|    `0x20` | `HSD_A_FRAC_S16`   | signed 16-bit integer   |
|    `0x40` | `HSD_A_FRAC_U16`   | unsigned 16-bit integer |
|    `0x60` | `HSD_A_FRAC_S8`    | signed 8-bit integer    |
|    `0x80` | `HSD_A_FRAC_U8`    | unsigned 8-bit integer  |

Float is selected only when the **entire** format byte is `0x00`, not merely
when its high bits are zero. For any other unlisted category the game returns
`0.0` and consumes no bytes.

Float and 16-bit payloads are stored least-significant byte first, unlike the
big-endian DAT descriptors around them. Source:
[`fobj.c`, `parseFloat`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L114-L153).

## Operation nibble

The low nibble of an operation byte selects the interpolation command:

|   Value | HAL name        | Meaning                                   |
| ------: | --------------- | ----------------------------------------- |
|     `0` | `HSD_A_OP_NONE` | No payload; dispatch returns state 0      |
|     `1` | `HSD_A_OP_CON`  | Constant/step interpolation               |
|     `2` | `HSD_A_OP_LIN`  | Linear interpolation                      |
|     `3` | `HSD_A_OP_SPL0` | Hermite point with a zero incoming slope  |
|     `4` | `HSD_A_OP_SPL`  | Hermite point with an explicit slope      |
|     `5` | `HSD_A_OP_SLP`  | Slope/tangent-only update                 |
|     `6` | `HSD_A_OP_KEY`  | Queued one-shot key update                |
| `7..15` | —               | No handler; takes the same path as `NONE` |

Source: [`fobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.h#L10-L28).

## Packed operation count

For the first command byte:

- bits `0..3`: operation;
- bits `4..6`: initial packet count minus one, encoding counts 1 through 8;
- bit `7`: more count chunks follow.

Each continuation byte contributes seven count bits, the first of them starting
at bit 3 of the count. Its bit 7 flags a further continuation byte. The chunks
add to the initial count.

A wait duration is an unsigned base-128 varint: bit 7 is the continuation flag
and the first byte holds bits `0..6`.

The runtime stores the decoded packet count and wait in `u16` fields and
truncates larger values.

Source: [`fobj.c`, `parsePackInfo` and `parseWait`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L155-L203).

## Packet payloads

One operation header applies to its decoded packet count. Payload widths come
from the track's value and slope format bytes. The command stream does not
repeat those formats.

| Operation | Payload per packet | Wait behavior                    | State effect                                                     |
| --------- | ------------------ | -------------------------------- | ---------------------------------------------------------------- |
| `NONE`    | None               | None                             | Returns interpreter state 0 for that call                        |
| `CON`     | Value              | Value packet enters wait loading | Shifts `p1 -> p0`; clears the new slope                          |
| `LIN`     | Value              | Value packet enters wait loading | Same load state as CON; evaluation computes a segment slope      |
| `SPL0`    | Value              | Value packet enters wait loading | Shifts value and slope; sets the new incoming slope to zero      |
| `SPL`     | Value, then slope  | Value packet enters wait loading | Shifts value/slope and loads both new fields                     |
| `SLP`     | Slope only         | No wait of its own               | Shifts `d1 -> d0`, loads `d1 = slope`, and remains data-loading  |
| `KEY`     | Value              | Value packet enters wait loading | Queues a one-shot key value; a later key or termination emits it |

Value packets and wait varints alternate. SLP packets carry no wait and set a
tangent for the value packet that follows:

```text
value group = pack-header (value-payload [wait-varint]){packet-count}
slope group = SLP-pack-header slope-payload{packet-count}
```

The final wait is absent only when the stream ends immediately after that
payload. Reaching the stream length where a wait would start ends the stream
instead. A wait may exceed the animation's end frame.

When a new command group begins, the interpreter copies the **previous**
operation into `op_intrp` before reading the new header. The operation that
supplied the current point therefore controls evaluation toward the following
point. A command boundary is not a time boundary.

Sources:

- [`fobj.c`, operation payload loaders](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L219-L296)
- [`fobj.c`, packet dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L298-L334)

## Interpreter states

The low nibble of runtime `HSD_FObj.flags` stores the persistent state. Values
that loader helpers return within one call are not always stored:

|     State | Meaning and transition                                                                          |
| --------: | ----------------------------------------------------------------------------------------------- |
|       `0` | Inactive; interpreter returns                                                                   |
|       `1` | Initial data load; value op stores state `3`, SLP remains `1`                                   |
|       `2` | Subsequent data load; value op stores state `4`, SLP remains `2`                                |
|       `3` | Emit an already-launched KEY if necessary, then load a wait and store `2`                       |
|       `4` | If the wait elapsed, subtract it and store `3`; otherwise evaluate, store `5`, and return       |
|       `5` | Restore stored `4` on the next interpreter call                                                 |
| local `6` | End of stream: restore the last crossed duration, launch a pending KEY, update once, and return |

`HSD_FObjInterpretAnim` (`0x8036b030`) does not store local `6` (end of stream)
or the local `0` an undefined or `NONE` operation returns. Such a call ends, and
the next call resumes from the stored load state and remaining packet count.
Only an explicit FObj or AObj stop stores state `0`.

Because the end-of-stream state is not stored, a stream that has ended can emit
on every later call. Packed bytes `06 02 01 01 04` (KEY `2`, wait `1`, CON `4`)
emit nothing at tick `0`, `4` at tick `1`, and `4` twice at tick `2`: the
pending KEY launch leaves flag bit `0x80` set, so the tick updates once in the
stored wait-load state and again at end of stream.

Sources:

- [`fobj.c`, state access/reset](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L38-L72)
- [`fobj.c`, interpreter transitions](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L390-L444)

## Evaluation and timing

`HSD_FObjReqAnim` sets `time = s16(startframe) + requested_start`, clears the
value and slope state, and points the decoder at the stream head. Each
interpreter call adds its `rate` to `time`. While `time` is negative the track
produces no update. Waits are in animation-time units. The stream stores no
sample rate; the caller chooses the rate.

For a segment duration `T = fterm` and elapsed segment time `t`:

- `CON`: `p0` while `t < T`, then `p1`;
- `LIN`: on the first update, `d0 = (p1 - p0) / T`, then
  `p(t) = p0 + d0 * t`; if `T == 0`, it snaps to `p1`;
- `SPL0`, `SPL`, and `SLP`: cubic Hermite evaluation, or `p1` when `T == 0`;
- `KEY`: emits a queued `p0` once when its launch flag is set; otherwise it does
  not update the target.

When one call crosses a whole interval and then reaches the end of the stream,
the end-of-stream state restores the most recently subtracted duration before
its final update. A requested start beyond the last interval therefore
extrapolates LIN or Hermite state. It does not clamp to the final point. AObj
end and loop handling is a separate layer.

For LIN, `FObjUpdateAnim` computes the slope with a single-precision subtract
and divide and stores it. Each update then uses one fused multiply-add
(`fmadds` at `0x8036af98`) for `p0 + d0 * t`. The cached slope is rounded; the
product is not.

The Hermite call is equivalent to the standard form with `u = t / T`:

```text
p(t) = (2u^3 - 3u^2 + 1) p0
     + (-2u^3 + 3u^2) p1
     + (u^3 - 2u^2 + u) T d0
     + (u^3 - u^2) T d1
```

The game's operation order differs from that form and rounds differently.
`FObjUpdateAnim` (`0x8036afdc`) computes `1 / T` with a double-precision divide
and rounds it to single precision. `splGetHelmite` (`0x80378a34`) builds the
powers from `t` and that rounded reciprocal, reuses the end-tangent
subtraction, and accumulates the result with three fused multiply-adds.

Sources:

- [`fobj.c`, request/reset and evaluation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L55-L77)
- [`fobj.c`, interpolation update](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L336-L388)
- [`fobj.c`, interpreter state machine](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L390-L444)
- [`spline.c`, `splGetHelmite`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/spline.c#L7-L29)

## AObj scheduling around the stream

The AObj owns the clock. A runtime AObj holds the current frame, a rewind frame,
the end frame, and a mutable `framerate` that defaults to `1.0`.

`HSD_AObjReqAnim` sets the current frame, requests every FObj at that frame,
clears `AOBJ_NO_ANIM`, and sets `AOBJ_FIRST_PLAY`. The first interpretation
after a request uses rate `0`. Each later one first advances the current frame
by `framerate`.

FObjs are interpreted head-to-tail. Each FObj delivers its updates to the
receiver callback before the next FObj runs.

When the current frame reaches the end frame:

- A looping AObj with `rewind < end` stops its FObjs, wraps the current frame to
  `fmod(current - rewind, end - rewind) + rewind`, and requests every FObj at
  the wrapped frame.
- A looping AObj with `rewind >= end` clamps the current frame to `end`. It does
  not stop or re-request the FObjs.
- Both looping cases set `AOBJ_REWINDED` and interpret the FObjs with rate `0`.
- A non-looping AObj interprets the end tick at the normal rate, then stops
  every FObj and sets `AOBJ_NO_ANIM`.

Stopping an FObj gives a pending KEY one final interpretation, then stores
state `0`.

`AOBJ_NO_UPDATE` still advances and parses the streams but passes a null
receiver callback during ordinary interpretation.

The [`fmod`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L172-L180)
used for the wrap (`0x80364340`) is not an IEEE remainder. It divides in single
precision, truncates the quotient to a signed 64-bit integer with saturation,
converts it back to `f32`, and computes the residual with one fused
negate-multiply-subtract. The caller then adds the rewind frame. The result is
not clamped into the loop interval and can be slightly negative or negative
zero. For offset `1.0` and span `0.1`, the quotient rounds to `10` and the
residual is `-2^-26`.

Sources:

- [`aobj.c`, request and first-play rate](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L91-L132)
- [`aobj.c`, loop/end handling](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L134-L169)
- [`aobj.c`, runtime setters](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L500-L548)
- [`fobj.c`, request and stop](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L50-L111)
- [`fobj.c`, KEY flush on stop](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L79-L104)

# Context-specific channel types

A numeric FObj type has meaning only in the object receiving it. The same
number names different channels for a JObj, an RObj, an MObj, a TObj, and a
ShapeSet. There is no global animation-channel enum.

## JObj channels

|    Value | Name                   | Meaning                                    |
| -------: | ---------------------- | ------------------------------------------ |
|      `0` | —                      | None                                       |
|      `1` | `HSD_A_J_ROTX`         | Rotation X                                 |
|      `2` | `HSD_A_J_ROTY`         | Rotation Y                                 |
|      `3` | `HSD_A_J_ROTZ`         | Rotation Z                                 |
|      `4` | `HSD_A_J_PATH`         | Path                                       |
|      `5` | `HSD_A_J_TRAX`         | Translation X                              |
|      `6` | `HSD_A_J_TRAY`         | Translation Y                              |
|      `7` | `HSD_A_J_TRAZ`         | Translation Z                              |
|      `8` | `HSD_A_J_SCAX`         | Scale X                                    |
|      `9` | `HSD_A_J_SCAY`         | Scale Y                                    |
|     `10` | `HSD_A_J_SCAZ`         | Scale Z                                    |
|     `11` | `HSD_A_J_NODE`         | Thresholded visibility on the current JObj |
|     `12` | `HSD_A_J_BRANCH`       | Thresholded recursive subtree visibility   |
| `20..29` | `HSD_A_J_SETBYTE0..9`  | User byte channels 0 through 9             |
| `30..39` | `HSD_A_J_SETFLOAT0..9` | User float channels 0 through 9            |

Source: [`jobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.h#L24-L57).
Values 13 through 19 are unassigned.

The game's fighter animations use types `1..3` and `5..12` only.

### PATH spline motion

PATH is a scalar position along a curve. It is not a translation component.
`JObjUpdateFunc` clamps each sample to `[0, 1]`, reads the receiving AObj's
auxiliary object (`aobj->hsd_obj`) as a JObj, and asserts that the JObj and its
union spline pointer are non-null. It calls `splArcLengthPoint` and writes all
three translation components from the resulting point. The spline evaluator
converts normalized arc distance to a parameter, then evaluates a linear,
Bézier, B-spline, or cardinal curve for spline types `0..3`. The callback does
not check `JOBJ_SPLINE`.

The write happens at PATH's position in FObj list order. A later TRAX/Y/Z FObj
replaces one component of the PATH result. PATH replaces any translation
component interpreted before it.

The auxiliary object comes from [`HSD_AObjDesc.obj_id`](#hsd_aobjdesc). It can
lie outside the animated model tree. The staff roll (`GmStRoll.dat`) uses PATH
this way: one AObj's `obj_id` names a childless spline JObj that the model tree
does not contain.

The fighter compact loaders never assign an auxiliary object:
`HSD_AObjAlloc` zeroes `hsd_obj` and the loaders fill only flags, timing, and
FObjs. A PATH track attached that way would fail the callback's assertions. No
fighter animation in the game uses PATH.

Sources:

- [`JObjUpdateFunc`, PATH case](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L355-L381)
- [`HSD_AObjLoadDesc`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L182-L220)
- [`HSD_AObjAlloc`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L245-L253)
- [JObj descriptor-identity registration](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L633-L668)
- [fighter compact AObj loaders](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L87-L135)
- [staff-roll resource/model selection and frame request](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/gm/gmstaffroll.c#L1032-L1089)
- [`HSD_Spline` and evaluators](ssbm_hal_dat_tables.md#spline-hsd_spline)

### NODE and BRANCH visibility controls

`JObjUpdateFunc` treats both channels as thresholded scalars. They are not
pointers or serialized booleans.

- NODE clears `JOBJ_HIDDEN` on the receiving JObj when the sample is greater
  than `0.5`. Otherwise it sets `JOBJ_HIDDEN` on that JObj only.
- BRANCH applies the same threshold but clears or sets `JOBJ_HIDDEN` on the
  receiving JObj and all its descendants. The recursive helpers do not descend
  through an `INSTANCE` JObj's child pointer.

A hidden JObj skips drawing only its own DObjs (`HSD_JObjDispDObj`). Its
children still draw unless they are hidden too.

CON packets make these channels look boolean, and every value payload on these
channels in the game's fighter animations is `0` or `1`. SPL and SLP packets
also occur on them, so a sample can lie between the two. Visibility changes
only when the sample crosses the strict `> 0.5` threshold.

Sources:

- [`JObjUpdateFunc`, NODE/BRANCH cases](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L355-L437)
- [recursive set/clear helpers](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L1010-L1047)

### JObj track order

Both generic AnimJoint loading and the fighter compact loaders move the first
BRANCH (type `12`) FObj to the head of the receiving AObj's FObj list. Nothing
else moves: NODE tracks, later BRANCH tracks, and every other track keep their
descriptor order.

FObjs are interpreted head-to-tail, and JObj animation runs parent before
children. A later NODE update on the same JObj or on a descendant therefore
overrides an earlier recursive BRANCH update.

Sources:

- [`TYPE_JOBJ = 12`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.h#L28-L32)
- [`JObjSortAnim`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L283-L316)
- [fighter-path duplicate sorter](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L67-L128)
- [head-to-tail FObj interpretation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L446-L454)
- [parent-before-child JObj animation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L535-L559)

## RObj active-state channel

RObj FObjs have their own type namespace. Only type `1` (`TYPE_ROBJ`) is
consumed; every other type is ignored. A sample **greater than or equal to**
`0.5` sets RObj flag bit 31. Any other sample, including NaN, clears it. This
differs from JObj type `1` (rotation X) and from the strict `> 0.5` test of
NODE and BRANCH.

The channel changes bit 31 only. It does not animate the RObj's target, limit
value, or expression pointer. JObj-reference lookup and expression evaluation
require the bit. The general `resolveLimits` pass ignores it, while LIMIT
lookups through `HSD_RObjGetByType` require it.

JObj animation interprets the receiving JObj's AObj, then every RObj AObj, then
DObj animation. RObj elements, and each element's FObjs, are interpreted
head-to-tail. With several type-`1` tracks on one RObj, the last one
interpreted wins.

A type-`1` track attached to an RObj is not observed in the game's files.

Sources:

- [`TYPE_ROBJ = 1`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.h#L26-L32)
- [threshold and active-bit update](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L80-L120)
- [active lookup and JObj-reference consumption](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L52-L77)
- [JObj-reference position consumption](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L213-L243)
- [`resolveLimits` and expression update paths](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L448-L575)
- [active-gated LIMIT lookup in IK](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L1323-L1355)
- [JObj/RObj animation order](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L535-L540)
- [head-to-tail FObj interpretation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/fobj.c#L446-L453)

## Material channels

**Receiver: runtime `HSD_MObj`.**

|  Value | Meaning                        | Update                                                 |
| -----: | ------------------------------ | ------------------------------------------------------ |
| `1..3` | Ambient R, G, B                | Store `(u8)(255.0 * sample)` in the selected component |
| `4..6` | Diffuse R, G, B                | Store `(u8)(255.0 * sample)` in the selected component |
| `7..9` | Specular R, G, B               | Store `(u8)(255.0 * sample)` in the selected component |
|   `10` | Alpha/transparency control     | Store `1.0 - sample` in material alpha                 |
|   `11` | Pixel-engine alpha reference 0 | If PE exists, store `(u8)(255.0 * sample)`             |
|   `12` | Pixel-engine alpha reference 1 | If PE exists, store `(u8)(255.0 * sample)`             |
|   `13` | Pixel-engine destination alpha | If PE exists, store `(u8)(255.0 * sample)`             |

Type 10 is inverted: the sample is transparency, not alpha. The three PE
channels do nothing when the MObj has no runtime PE descriptor. The callback
does not range-check a sample before the `u8` cast.

Sources: [channel values](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.h#L19-L31)
and [MObj update callback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L87-L141).

## Texture channels

**Receiver: runtime `HSD_TObj`.**

|    Value | Meaning                          | Update                                                                                   |
| -------: | -------------------------------- | ---------------------------------------------------------------------------------------- |
|      `1` | Texture image selection (`TIMG`) | Convert sample to an integer index and replace the image if that table entry is non-null |
|   `2..3` | Translation U/V                  | Assign X/Y translation and mark the texture matrix dirty                                 |
|   `4..5` | Scale U/V                        | Assign X/Y scale and mark the texture matrix dirty                                       |
|   `6..8` | Rotation X/Y/Z                   | Assign rotation component and mark the texture matrix dirty                              |
|      `9` | Texture-stage blend              | Assign `blending = sample`                                                               |
|     `10` | TLUT selection (`TCLT`)          | If a TLUT table exists, assign `tlut_no = (u8)sample`                                    |
|     `11` | LOD bias                         | Assign the referenced LOD descriptor's bias                                              |
| `12..15` | Konst R/G/B/A                    | Store `(u8)(255.0 * sample)` in the selected component                                   |
| `16..19` | TEV0 R/G/B/A                     | Store `(u8)(255.0 * sample)` in the selected component                                   |
| `20..23` | TEV1 R/G/B/A                     | Store `(u8)(255.0 * sample)` in the selected component                                   |
|     `24` | Texture-stage blend (`TS_BLEND`) | Also assign `blending = sample`                                                          |

The callback does not bounds-check a TIMG index and does not null-check the LOD
or TEV pointers that types `11..23` write through. Types `12..23` change
register bytes only. They do not set the TEV descriptor's bits that enable a
custom color or alpha register; those bits are separate serialized state.

Sources: [channel values](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.h#L20-L43)
and [TObj update callback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L136-L227).

# Animation attachment structures

Every generic animation graph is a tree or list that mirrors the model. The
game pairs animation nodes with model objects by position, except where a
section below says otherwise.

## HSD_AnimJoint

**Size: `0x14`.**

| Offset | Size | Format  | Description          |
| ------ | ---: | ------- | -------------------- |
| `0x00` |    4 | pointer | Child AnimJoint      |
| `0x04` |    4 | pointer | Next AnimJoint       |
| `0x08` |    4 | pointer | AObj descriptor      |
| `0x0C` |    4 | pointer | RObj animation joint |
| `0x10` |    4 | bitmask | Flags                |

Source: [`aobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.h#L51-L64).

`HSD_JObjAddAnimAll` walks the model JObj tree and the AnimJoint tree side by
side. A JObj's children pair with the AnimJoint's children in sibling order.
The walk skips a branch that has no AnimJoint and does not enter the children
of a `JOBJ_INSTANCE` JObj.

At each paired JObj the game attaches the JObj's AObj, then the RObj animation
list, then the DObj animations. A null AObj descriptor removes the JObj's
existing AObj.

Sources:

- [generic positional AnimJoint attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L302-L349)
- [null AObj descriptor loading](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/aobj.c#L182-L220)

## HSD_RObjAnimJoint

**Size: `0x08`.**

| Offset | Size | Format  | Description                                                |
| ------ | ---: | ------- | ---------------------------------------------------------- |
| `0x00` |    4 | pointer | Next RObj animation element                                |
| `0x04` |    4 | pointer | AObj descriptor whose FObjs use the RObj channel namespace |

This is a list, not a tree. `HSD_RObjAddAnimAll` advances the JObj's RObj list
and this list together while both have an element. Extra animation elements are
ignored, and RObjs past the end of the animation list are left unchanged. On a
matched element the old AObj is removed before the new descriptor loads, so a
null AObj descriptor clears that RObj's animation.

Sources:

- [`robj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.h#L85-L88)
- [positional RObj animation attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/robj.c#L177-L201)
- [generic JObj attachment call](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L302-L317)

## Material animation graph

### Serialized layouts

`HSD_MatAnimJoint` (`0x0C`):

| Offset | Size | Format  | Description        |
| ------ | ---: | ------- | ------------------ |
| `0x00` |    4 | pointer | Child MatAnimJoint |
| `0x04` |    4 | pointer | Next MatAnimJoint  |
| `0x08` |    4 | pointer | First MatAnim      |

`HSD_MatAnim` (`0x10`):

| Offset | Size | Format  | Description              |
| ------ | ---: | ------- | ------------------------ |
| `0x00` |    4 | pointer | Next MatAnim             |
| `0x04` |    4 | pointer | Material AObj descriptor |
| `0x08` |    4 | pointer | TexAnim list             |
| `0x0C` |    4 | pointer | RenderAnim               |

`HSD_RenderAnim` (`0x08`):

| Offset | Size | Format  | Description     |
| ------ | ---: | ------- | --------------- |
| `0x00` |    4 | pointer | ChanAnim list   |
| `0x04` |    4 | pointer | TevRegAnim list |

`HSD_ChanAnim` and `HSD_TevRegAnim` (`0x08` each):

| Offset | Size | Format  | Description     |
| ------ | ---: | ------- | --------------- |
| `0x00` |    4 | pointer | Next            |
| `0x04` |    4 | pointer | AObj descriptor |

Source: [`mobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.h#L112-L142).

The game declares `renderanim`, `chananim`, and `reganim` but has no code that
reads them. How they attach and what their channel types mean is unknown. A
non-null RenderAnim pointer is not observed in the game's files.

### Attachment and evaluation

MatAnimJoint nodes pair with the JObj tree by child/next position. `INSTANCE`
stops child recursion, as it does for the other generic animation trees.

At each JObj, the DObj list and the MatAnim list advance together. When the
MatAnim list is shorter, later DObjs receive a null MatAnim, which leaves their
MObj animation unchanged. MatAnim entries past the end of the DObj list are not
attached.

For a non-null MatAnim, `HSD_MObjAddAnim` replaces the MObj's AObj from the
material descriptor and offers the whole TexAnim list to every TObj. TexAnim is
**not** paired by list position. Each TObj takes the first TexAnim whose
`GXTexMapID` equals its own. A match replaces the TObj's AObj, installs the
image table, rebuilds the runtime TLUT table, and resets the `u8` `tlut_no`
field to `0xFF`. A TObj with no matching TexAnim keeps its existing animation.

Material evaluation interprets the MObj's AObj first, then each TObj's AObj in
list order.

Sources: [JObj/tree attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L302-L349),
[DObj/MatAnim positional attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/dobj.c#L83-L117),
[MObj attachment/evaluation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L56-L68),
[`HSD_MObjAnim`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/mobj.c#L144-L150),
and [TexAnim ID lookup/attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.c#L45-L103).

## Texture animation

`HSD_TexAnim` (`0x18`):

| Offset | Size | Format   | Description                    |
| ------ | ---: | -------- | ------------------------------ |
| `0x00` |    4 | pointer  | Next TexAnim                   |
| `0x04` |    4 | enum     | Texture-map ID (`GXTexMapID`)  |
| `0x08` |    4 | pointer  | AObj descriptor                |
| `0x0C` |    4 | pointer  | Image-descriptor pointer table |
| `0x10` |    4 | pointer  | TLUT-descriptor pointer table  |
| `0x14` |    2 | unsigned | Image count                    |
| `0x16` |    2 | unsigned | TLUT count                     |

Source: [`tobj.h`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/tobj.h#L253-L261).

The tables are what the `TIMG` and `TCLT` [texture channels](#texture-channels)
index.

## Shape animation graph

| Structure            |   Size | Serialized fields                      |
| -------------------- | -----: | -------------------------------------- |
| `HSD_ShapeAnimJoint` | `0x0C` | child, next, first `HSD_ShapeAnimDObj` |
| `HSD_ShapeAnimDObj`  | `0x08` | next, first `HSD_ShapeAnim`            |
| `HSD_ShapeAnim`      | `0x08` | next, `HSD_AObjDesc`                   |

Sources: [ShapeAnimJoint and ShapeAnim layouts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.h#L102-L110)
and [the ShapeAnimDObj wrapper](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/dobj.h#L39-L42).

The graph has three levels: joint, DObj, PObj.
[HSDLib](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/Animation/HSD_ShapeAnimJoint.cs#L3-L13)
types `ShapeAnimJoint + 0x08` directly as a ShapeAnim list and reads every
ShapeSet index array as signed 16-bit. Both disagree with the game, which keeps
the DObj level and reads unsigned `u8` or big-endian `u16` indices as each
ShapeSet attribute descriptor selects.

### Positional attachment

`HSD_JObjAddAnimAll` pairs model JObjs with ShapeAnimJoint nodes by
child/sibling position and does not recurse below `JOBJ_INSTANCE`. For each
JObj, `HSD_DObjAddAnimAll` advances the model DObj, MatAnim, and ShapeAnimDObj
lists while the **model DObj list** has an element. A missing animation-side
entry is passed as null. Extra animation-side entries are ignored. Each paired
ShapeAnimDObj supplies a ShapeAnim list to `HSD_PObjAddAnimAll`, which advances
while the **model PObj list** has an element.

Absence and null differ:

- If a DObj's first ShapeAnim pointer is null, `HSD_PObjAddAnimAll` returns and
  leaves every PObj's existing animation unchanged.
- Once a non-null list starts, a missing later ShapeAnim entry leaves that PObj
  unchanged, and a ShapeAnim entry past the end of the PObj list is ignored.
- A ShapeAnim node that is present with a null AObj descriptor removes the
  PObj's AObj.

Once a non-null ShapeAnim list starts, the game asserts that every PObj it
visits is of type `POBJ_SHAPEANIM` with a ShapeSet, including PObjs past the end
of the animation list.

Sources: [JObj attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L302-L350),
[DObj attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/dobj.c#L83-L118),
and [PObj attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L77-L111).

### Shape receiver channels

Shape FObjs use the packed stream and interpreter described above. The receiver
is the PObj's ShapeSet, and the ShapeSet's mode decides how a type is read:

|                Type | Mode     | Behavior                                                |
| ------------------: | -------- | ------------------------------------------------------- |
|                 any | Average  | Overwrites the one blend scalar; the type is not read   |
| `2..(nb_shape + 1)` | Additive | Writes `weight[type - 2]` (`HSD_A_S_W0 = 2`)            |
|               other | Additive | Indexes outside the weight array; the game has no check |

Average mode has one scalar, so with several tracks the last update wins. The
game's average-mode tracks all use type `1`, which HSDLib names
[`HSD_A_S_BLEND`](https://github.com/Ploaj/HSDLib/blob/0e87df7ff6a93fb53f097d03455f9fcd8fff2288/HSDRaw/Common/Animation/HSD_FOBJ.cs#L126-L131).
The decompilation gives it no name.

Additive weights start at zero. A channel with no update keeps its previous
value. Negative weights clamp to zero when drawing, and positive weights have
no upper clamp.

Every ShapeSet in the game's files has flags `0x0005`: average mode with bit 2
set. The meaning of bit 2 is unknown. Additive mode and NBT normals are not
observed in the game's files.

Sources: [ShapeSet update callback](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L137-L163)
and [`HSD_A_S_W0`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/forward.h#L115-L130).

### Average blend evaluation

The ShapeSet layout is in the
[ShapeSet table](ssbm_hal_dat_tables.md#shapeset-structure-hsd_shapesetdesc).
This section covers how the animated scalar selects vertices.

Each of the `nb_shape` shapes has a position index map and, when the set has
normals, a normal index map. A map entry is an unsigned `u8` or a big-endian
`u16`, as the attribute descriptor selects. It addresses the shared component
buffer at `index * stride`.

`U8`, `S8`, `U16`, and `S16` components divide by the signed value
`1 << frac`. `F32` components are copied and `frac` is not applied.

For blend scalar `b`:

```text
s = min(max(0, trunc(b)), nb_shape - 1)
t = min(max(0, b - s), 1)
out = (v[s + 1] - v[s]) * t + v[s]      with s + 1 clamped to the last shape
```

Positions, XYZ normals, and all nine NBT components use the same equation per
component. Normals are not renormalized. One NBT entry writes nine consecutive
floats into the normal buffer.

Sources: [XYZ and NBT component/map decoding](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L569-L704)
and [average XYZ/NBT interpolation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/pobj.c#L833-L907).

# Fighter animation

## Fighter animation table

`ftData + 0x0C` in a fighter file (`Pl**.dat`) points to a table of `0x18`-byte
animation records. Each record selects one mini-archive inside `Pl**AJ.dat`. An
animation index is a position in this table. It is a separate namespace from
action-state IDs and FObj channel types.

| Offset | Size | Format          | Description                                                     |
| ------ | ---: | --------------- | --------------------------------------------------------------- |
| `0x00` |    4 | pointer         | Symbol name of the animation                                    |
| `0x04` |    4 | signed          | Mini-archive offset within `Pl**AJ.dat`                         |
| `0x08` |    4 | signed          | Mini-archive size in bytes                                      |
| `0x0C` |    4 | pointer         | Command/script data                                             |
| `0x10` |    4 | packed bitfield | Runtime animation flags, auxiliary mask, and source FighterKind |
| `0x14` |    4 | runtime field   | Zero in the file; the game stores the loaded address here       |

Sources: [`ft/types.h`, fighter data pointers and record](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/types.h#L613-L628),
[`Fighter_WaitAnimData`](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/types.h#L889-L896),
the [packed fighter animation state](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/types.h#L1205-L1229),
the [action-state copy](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/fighter.c#L1248-L1256),
and the [AJ relocation loop](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftdata.c#L1573-L1600).

Each mini-archive is a complete HAL DAT archive with its own header, relocation
table, and root table. Its public root is the record's symbol and points to a
[FigaTree](#figatree). Symbols follow the form
`PlyCaptain5K_Share_ACTION_Wait1_figatree`: the text between `_ACTION_` and
`_figatree` is the action name. `ftData_80085A14` loads a mini-archive into a
`0x8000`-byte buffer, which bounds its size.

A record with size zero has no animation.

Action selection copies record `0x10` into the fighter's packed `x594` word:

```text
bits 0..5    source_fighter_kind = word & 0x3F
bits 6..8    partial-attachment part selector
bits 9..21   auxiliary_part_mask = (word >> 9) & 0x1FFF
bits 22..31  other runtime animation state
```

The source kind and the auxiliary mask decide how the animation's
[track-count list maps to parts](#track-count-list-and-fighter-parts):

- A record whose source kind is the fighter's own kind attaches directly over
  the fighter's parts.
- A record with another source kind attaches through the cross-kind remap.
  Animations shared between fighters (`PlyTaro` symbols) name `FTKIND_NONE` as
  their source kind.
- A nonzero auxiliary mask enables auxiliary part slots. Kirby's copy-ability
  animations use it.

Nana has no record of her own for some motions, including Wait: the record's
size is zero. Outside demo player slots, `ftData_80085FD4` plays Popo's record
for the same index. The flags word still comes from Nana's record and names
Nana, so Popo's tree attaches over Nana's parts without a remap.

## Record count

The table's length is not stored in the fighter file. The executable holds it:
`ftData_Table_Unk0` (`0x803C0FC8`, 33 eight-byte entries) gives each fighter
kind's record count. Captain Falcon has 318 records.

Source: [per-fighter record counts](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftdata.c#L174-L180).

## Common part tables

`PlCo.dat` (public root `ftLoadCommonData`) holds the part tables every fighter
shares. `Fighter_LoadCommonData` reads two per-kind pointer arrays from the
root: the part tables at `0x10` and the auxiliary tables at `0x14`. There is one
part table for each of the 33 fighter kinds and one for `FTKIND_NONE`.

`FighterPartsTable` (`0x0C`):

| Offset | Size | Format   | Description                                                    |
| ------ | ---: | -------- | -------------------------------------------------------------- |
| `0x00` |    4 | pointer  | `joint_to_part`: one byte per slot, the logical part or `0xFF` |
| `0x04` |    4 | pointer  | `part_to_joint`: one byte per logical part, the slot           |
| `0x08` |    4 | unsigned | `parts_num`, the number of slots                               |

Auxiliary table (`0x08`); a kind with no auxiliary slots has a null pointer:

| Offset | Size | Format   | Description |
| ------ | ---: | -------- | ----------- |
| `0x00` |    4 | pointer  | Entry array |
| `0x04` |    4 | unsigned | Entry count |

Auxiliary entry (`0x04`):

| Offset | Size | Format   | Description             |
| ------ | ---: | -------- | ----------------------- |
| `0x00` |    1 | unsigned | Slot                    |
| `0x01` |    1 | unsigned | Relative slot           |
| `0x02` |    1 | unsigned | Insertion type (`0..3`) |
| `0x03` |    1 | unsigned | Descriptor ordinal      |

An auxiliary slot holds no costume JObj. The game inserts a joint there at run
time when an animation's auxiliary mask enables it. Four part tables have
auxiliary slots: one has 13 and three have one each.

Source: [`fighter.c`, common-data load](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/fighter.c#L184-L202).

## Primary and part animations

The animation an action selects is the fighter's primary animation.
`ftAnim_80070A10` attaches it only to parts whose flag `b5` is clear, and
same-kind attachment skips parts whose flag `b1` is clear.

Fighter data also holds per-part animations as generic AnimJoint graphs, such
as Falco's eye animations in `PlFc.dat`. A part animation replaces the primary
controller on its joints and runs on its own clock.

The animation does not drive every transform. The game sets the root joint's
scale, facing rotation, and position. `ftCommon_8007F6A4` sets the scale of the
part that `ftData.x8->x10` names to `1 / model_scaling`.

Sources: [joint traversal](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/jobj.c#L547-L568),
[compact attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lbanim.c#L87-L109),
[primary receiver gate](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L1249-L1252),
[part animation selection](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L1169-L1277),
[descriptor reset](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L806-L861),
and [reciprocal scale](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftcommon.c#L1424-L1431).

## Blending between animations

When an action changes with a blend duration, the game blends the incoming
animation's pose into the displayed pose over several ticks.
`ftAnim_8006E9B4` advances the blend, `lb_8000C490` blends one joint, and
`lbCopyJObjSRT` copies one joint.

- **Order within the transition frame.** Fighter proc 1 advances the old
  animation before proc 3 handles the input that changes the action.
  `ChangeMotionState` then attaches the new animation and evaluates it at once.
  The blend therefore starts from the old animation advanced one more tick, not
  from the last displayed pose.
- **Eligible parts.** A part blends when its flags satisfy
  `b1 && !b0 && !b5`. An eligible part with `b4` set copies the incoming pose
  directly.
- **Weight.** Each tick first advances the blend progress, then uses
  `rate / (rate + remaining)` as the incoming weight. It is not elapsed time
  over duration. Each tick's result is the starting pose of the next tick.
- **Scale and translation.** Each component is a weighted sum of the two poses,
  computed with a fused multiply-add.
- **Rotation.** When every Euler axis of the two poses is within `0.0001`
  (inclusive), the incoming Euler value is copied. Otherwise both rotations
  are converted to quaternions as needed, the nearer of the two antipodal
  quaternions is chosen by squared distance, and the result is a quaternion
  interpolation. A joint blended this way holds a quaternion rotation.
- **Completion.** When the blend completes, each joint copies the incoming
  pose, including whether its rotation is Euler or quaternion. A joint that
  blended as a quaternion can return to Euler storage.

Sources: [progress and part gates](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L315-L378),
[blend/copy](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/lb/lb_00B0.c#L473-L570),
[quaternion conversion/interpolation](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/sysdolphin/baselib/quatlib.c#L136-L215),
[transition attachment](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/ftanim.c#L389-L429),
and [fighter transition dispatch](https://github.com/doldecomp/melee/blob/90f83f6665648a73122146d981eec48f173b1ca3/src/melee/ft/fighter.c#L1270-L1301).
