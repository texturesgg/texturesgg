# gc-iso

Read GameCube disc images (GCM/ISO), and replace one file in place.

- `Disc::open` reads the header and file table without loading the image, then
  `Disc::read` streams one file by its path, or by its bare name when no other
  file shares it.
- `replace_file` writes a replacement into the image. A file that
  fits goes into its old slot; a larger one is appended at the end, padded to
  the disc's 32-byte file boundary, and the file table is pointed at it. Nothing
  else on the disc moves, and a slot that shares bytes with the header, the
  file table or another file is refused.

File contents are opaque bytes here: checking that a replacement suits the game
is the caller's job. Offsets and sizes from the image are bounds-checked before
they are used.

```rust
let mut disc = gc_iso::Disc::open("melee.iso")?;
assert_eq!(disc.header().game_id, "GALE01");
let costume = disc.read("PlFcNr.dat")?;

gc_iso::replace_file("melee.iso", "PlFcNr.dat", &edited)?;
```
