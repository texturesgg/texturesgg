//! A model small enough to write out by hand, so the editor's flows run
//! without game files: one joint drawing one triangle, textured with an 8×8
//! RGBA8 image and tinted by RGB565 vertex colors (two green, one yellow),
//! with a material of its own.

const JOINT: usize = 0x000;
const DOBJ: usize = 0x040;
const MOBJ: usize = 0x050;
const MATERIAL: usize = 0x068;
const TOBJ: usize = 0x080;
const IMAGE: usize = 0x0E0;
const ATTRIBUTES: usize = 0x100;
const POBJ: usize = 0x150;
const DISPLAY_LIST: usize = 0x180;
const POSITIONS: usize = 0x1A0;
const TEX_COORDS: usize = 0x1D0;
/// Where the image's pixels start, as the editor reports a texture's offset.
pub const PIXELS: usize = 0x200;
const DATA_SIZE: usize = 0x300;

pub const SIZE: u32 = 8;
pub const GREEN: [u8; 4] = [96, 224, 96, 255];
pub const YELLOW: [u8; 4] = [248, 252, 48, 255];

/// The model's file, under the root name `Test_joint`.
pub fn model() -> Vec<u8> {
    model_named("Test_joint")
}

/// The model's file under the root name `root`. A costume's slot is read
/// from its root name, so `PlyFalco5KRe_Share_joint` makes it Falco's red.
pub fn model_named(root: &str) -> Vec<u8> {
    let mut data = vec![0u8; DATA_SIZE];
    let mut sites: Vec<u32> = Vec::new();
    let word = |data: &mut [u8], at: usize, value: u32| {
        data[at..at + 4].copy_from_slice(&value.to_be_bytes());
    };
    let mut pointer = |data: &mut [u8], at: usize, target: usize| {
        data[at..at + 4].copy_from_slice(&(target as u32).to_be_bytes());
        sites.push(at as u32);
    };
    let float = |data: &mut [u8], at: usize, value: f32| {
        data[at..at + 4].copy_from_slice(&value.to_be_bytes());
    };

    pointer(&mut data, JOINT + 0x10, DOBJ);
    for axis in 0..3 {
        float(&mut data, JOINT + 0x20 + axis * 4, 1.0);
    }

    pointer(&mut data, DOBJ + 0x08, MOBJ);
    pointer(&mut data, DOBJ + 0x0C, POBJ);

    // Unlit vertex colors (VERTEX alone), under texture 0.
    word(&mut data, MOBJ + 0x04, (1 << 1) | (1 << 4));
    pointer(&mut data, MOBJ + 0x08, TOBJ);
    pointer(&mut data, MOBJ + 0x0C, MATERIAL);
    data[MATERIAL..MATERIAL + 12]
        .copy_from_slice(&[10, 20, 30, 255, 200, 200, 200, 255, 70, 80, 90, 255]);
    float(&mut data, MATERIAL + 0x0C, 1.0);
    float(&mut data, MATERIAL + 0x10, 50.0);

    // GX_TG_TEX0, unit scale, repeating once each way, modulating.
    word(&mut data, TOBJ + 0x0C, 4);
    for axis in 0..3 {
        float(&mut data, TOBJ + 0x1C + axis * 4, 1.0);
    }
    data[TOBJ + 0x3C] = 1;
    data[TOBJ + 0x3D] = 1;
    word(&mut data, TOBJ + 0x40, (1 << 4) | (4 << 16));
    float(&mut data, TOBJ + 0x44, 1.0);
    word(&mut data, TOBJ + 0x48, 1);
    pointer(&mut data, TOBJ + 0x4C, IMAGE);

    pointer(&mut data, IMAGE, PIXELS);
    data[IMAGE + 4..IMAGE + 6].copy_from_slice(&(SIZE as u16).to_be_bytes());
    data[IMAGE + 6..IMAGE + 8].copy_from_slice(&(SIZE as u16).to_be_bytes());
    // GX_TF_RGBA8.
    word(&mut data, IMAGE + 0x08, 6);

    // (name, type, components, component type, stride, buffer)
    let attributes: [(u32, u32, u32, u32, u16, Option<usize>); 3] = [
        // Position: 8-bit indices into XYZ floats.
        (9, 2, 1, 4, 12, Some(POSITIONS)),
        // Color 0: RGB565, inline.
        (11, 1, 0, 0, 2, None),
        // Texture coordinate 0: 8-bit indices into ST floats.
        (13, 2, 1, 4, 8, Some(TEX_COORDS)),
    ];
    for (index, (name, kind, count, component, stride, buffer)) in
        attributes.into_iter().enumerate()
    {
        let at = ATTRIBUTES + index * 0x18;
        word(&mut data, at, name);
        word(&mut data, at + 0x04, kind);
        word(&mut data, at + 0x08, count);
        word(&mut data, at + 0x0C, component);
        data[at + 0x12..at + 0x14].copy_from_slice(&stride.to_be_bytes());
        if let Some(buffer) = buffer {
            pointer(&mut data, at + 0x14, buffer);
        }
    }
    word(&mut data, ATTRIBUTES + 3 * 0x18, 0xFF);

    pointer(&mut data, POBJ + 0x08, ATTRIBUTES);
    data[POBJ + 0x0E..POBJ + 0x10].copy_from_slice(&1u16.to_be_bytes());
    pointer(&mut data, POBJ + 0x10, DISPLAY_LIST);

    // GX_DRAW_TRIANGLES, three vertices: position, color, texture coordinate.
    let green = 0x670Cu16.to_be_bytes();
    let yellow = 0xFFE6u16.to_be_bytes();
    data[DISPLAY_LIST..DISPLAY_LIST + 15].copy_from_slice(&[
        0x90, 0, 3, 0, green[0], green[1], 0, 1, green[0], green[1], 1, 2, yellow[0], yellow[1], 2,
    ]);
    let positions: [[f32; 3]; 3] = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
    for (index, value) in positions.iter().flatten().enumerate() {
        float(&mut data, POSITIONS + index * 4, *value);
    }
    let tex_coords: [[f32; 2]; 3] = [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]];
    for (index, value) in tex_coords.iter().flatten().enumerate() {
        float(&mut data, TEX_COORDS + index * 4, *value);
    }
    // Opaque mid-gray. Each 4×4 block is 16 alpha-red pairs, then 16
    // green-blue pairs.
    data[PIXELS..].fill(0x80);
    for block in 0..4 {
        for texel in 0..16 {
            data[PIXELS + block * 64 + texel * 2] = 0xFF;
        }
    }

    sites.sort_unstable();
    let name = [root.as_bytes(), &[0]].concat();
    let mut file = vec![0u8; 0x20];
    let size = 0x20 + data.len() + sites.len() * 4 + 8 + name.len();
    file[0x00..0x04].copy_from_slice(&(size as u32).to_be_bytes());
    file[0x04..0x08].copy_from_slice(&(data.len() as u32).to_be_bytes());
    file[0x08..0x0C].copy_from_slice(&(sites.len() as u32).to_be_bytes());
    file[0x0C..0x10].copy_from_slice(&1u32.to_be_bytes());
    file.extend_from_slice(&data);
    file.extend(sites.iter().flat_map(|site| site.to_be_bytes()));
    // The root: the joint, named by the string table's first entry.
    file.extend_from_slice(&(JOINT as u32).to_be_bytes());
    file.extend_from_slice(&0u32.to_be_bytes());
    file.extend_from_slice(&name);
    file
}
