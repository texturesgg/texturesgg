//! Extract all textures from a .dat file as individual PNGs for visual inspection.
//!
//! Usage: extract-textures <dat_file> <output_dir>

use dat_parser::DatFile;
use dat_parser::descriptor::{mobj, tobj::TlutDesc, traversal};
use dat_parser::gx::{display_list, texture, vertex};

use std::env;
use std::fs;
use std::path::Path;

type TextureCacheKey = (u32, u16, u16, u32, Option<(Option<u32>, u32, u16)>);

fn texture_cache_key(
    data_ptr: u32,
    width: u16,
    height: u16,
    format: u32,
    tlut: Option<&TlutDesc>,
) -> TextureCacheKey {
    (
        data_ptr,
        width,
        height,
        format,
        tlut.map(|tlut| (tlut.data_ptr, tlut.format, tlut.color_count)),
    )
}

#[path = "support/png.rs"]
mod png;
mod support;
use support::report_traversal_issues;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: extract-textures <dat_file> <output_dir>");
        std::process::exit(1);
    }

    let dat_path = &args[1];
    let out_dir = Path::new(&args[2]);

    eprintln!("Parsing: {dat_path}");
    let raw = fs::read(dat_path).expect("Failed to read .dat file");
    let dat = DatFile::parse(&raw).expect("Failed to parse .dat file");

    // Create output directory
    fs::create_dir_all(out_dir).expect("Failed to create output directory");

    let mut texture_count = 0usize;
    // Track already-saved textures by every decoder input to avoid duplicates.
    let mut seen: Vec<TextureCacheKey> = Vec::new();

    for root in &dat.roots {
        if !root.name.ends_with("_joint") || root.name.contains("matanim") {
            continue;
        }

        println!("=== Root: {} @ 0x{:08X} ===\n", root.name, root.data_offset);

        let joints = report_traversal_issues(
            "JObj traversal",
            traversal::walk_joint_tree(&dat, root.data_offset, dat.data.len()),
        );

        for (joint_idx, joint) in joints.iter().enumerate() {
            let dobj_ptr = match joint.jobj.dobj_ptr {
                Some(p) => p,
                None => continue,
            };

            let dobjs = report_traversal_issues(
                "DObj traversal",
                traversal::read_dobj_list(&dat, dobj_ptr, dat.data.len()),
            );

            for (dobj_idx, d) in dobjs.iter().enumerate() {
                // --- Collect UV ranges from PObjs for this DObj ---
                let mut uv_min = [f32::MAX; 2];
                let mut uv_max = [f32::MIN; 2];
                let mut has_uvs = false;

                if let Some(pobj_ptr) = d.pobj_ptr {
                    let pobjs = report_traversal_issues(
                        "PObj traversal",
                        traversal::read_pobj_list(&dat, pobj_ptr, dat.data.len()),
                    );
                    for p in &pobjs {
                        if let Some(dl_offset) = p.display_list_offset {
                            let groups = display_list::parse_display_list(
                                &dat,
                                dl_offset,
                                p.display_list_size,
                                &p.attributes,
                            );
                            let mesh = vertex::decode_primitives(&dat, &p.attributes, &groups);
                            for v in &mesh.vertices {
                                uv_min[0] = uv_min[0].min(v.tex_coords[0][0]);
                                uv_min[1] = uv_min[1].min(v.tex_coords[0][1]);
                                uv_max[0] = uv_max[0].max(v.tex_coords[0][0]);
                                uv_max[1] = uv_max[1].max(v.tex_coords[0][1]);
                                has_uvs = true;
                            }
                        }
                    }
                }

                // --- Extract textures from MObj -> TObj chain ---
                let mobj_ptr = match d.mobj_ptr {
                    Some(p) => p,
                    None => continue,
                };

                let m = match mobj::MObj::parse(&dat, mobj_ptr) {
                    Ok(m) => m,
                    Err(error) => {
                        eprintln!("  ERROR: Invalid MObj at 0x{mobj_ptr:08X}: {error}");
                        continue;
                    }
                };

                let tobj_ptr = match m.tobj_ptr {
                    Some(p) => p,
                    None => continue,
                };

                let tobjs = report_traversal_issues(
                    "TObj traversal",
                    traversal::read_tobj_list(&dat, tobj_ptr, dat.data.len()),
                );

                for (tobj_idx, tobj) in tobjs.iter().enumerate() {
                    let img = match &tobj.image {
                        Some(img) => img,
                        None => {
                            println!(
                                "  [joint={} dobj={} tobj={}] No image descriptor",
                                joint_idx, dobj_idx, tobj_idx
                            );
                            continue;
                        }
                    };

                    let data_ptr = match img.data_ptr {
                        Some(p) => p,
                        None => {
                            println!(
                                "  [joint={} dobj={} tobj={}] No data pointer",
                                joint_idx, dobj_idx, tobj_idx
                            );
                            continue;
                        }
                    };

                    let cache_key = texture_cache_key(
                        data_ptr,
                        img.width,
                        img.height,
                        img.format,
                        tobj.tlut.as_ref(),
                    );
                    let is_duplicate = seen.contains(&cache_key);

                    // Print info for every texture reference regardless
                    println!(
                        "  tex {:>3} | joint={:<3} dobj={:<2} tobj={:<2} | {}x{} {:>6} | data=0x{:08X} tlut={}{}\n",
                        texture_count,
                        joint_idx,
                        dobj_idx,
                        tobj_idx,
                        img.width,
                        img.height,
                        img.format_name(),
                        data_ptr,
                        match tobj.tlut.as_ref().and_then(|tlut| tlut.data_ptr) {
                            Some(p) => format!("0x{:08X}", p),
                            None => "none".to_string(),
                        },
                        if is_duplicate {
                            " (duplicate, skipping)"
                        } else {
                            ""
                        },
                    );

                    // Print TObj details
                    println!(
                        "         texgen_src={} map_id={} wrap_s={} wrap_t={} flags=0x{:X}",
                        tobj.tex_gen_src, tobj.tex_map_id, tobj.wrap_s, tobj.wrap_t, tobj.flags,
                    );
                    if tobj.scale != [1.0, 1.0, 1.0]
                        || tobj.rotation != [0.0, 0.0, 0.0]
                        || tobj.translation != [0.0, 0.0, 0.0]
                    {
                        println!(
                            "         UV xform: scale={:?} rot={:?} trans={:?}",
                            tobj.scale, tobj.rotation, tobj.translation,
                        );
                    }

                    if has_uvs {
                        println!(
                            "         UV range: u=[{:.4}, {:.4}] v=[{:.4}, {:.4}]",
                            uv_min[0], uv_max[0], uv_min[1], uv_max[1],
                        );
                    }

                    if is_duplicate {
                        texture_count += 1;
                        continue;
                    }

                    // Decode and save
                    let rgba = match texture::decode_texture(
                        &dat,
                        data_ptr,
                        img.width,
                        img.height,
                        img.format,
                        tobj.tlut.as_ref(),
                    ) {
                        Some(rgba) => rgba,
                        None => {
                            eprintln!(
                                "  WARN: Failed to decode texture {} (format={}, {}x{})",
                                texture_count,
                                img.format_name(),
                                img.width,
                                img.height,
                            );
                            texture_count += 1;
                            continue;
                        }
                    };

                    let png_data = png::encode_png(img.width as u32, img.height as u32, &rgba);

                    let filename = format!(
                        "tex_{:03}_j{}_d{}_{}_{}x{}.png",
                        texture_count,
                        joint_idx,
                        dobj_idx,
                        img.format_name(),
                        img.width,
                        img.height,
                    );
                    let out_path = out_dir.join(&filename);
                    fs::write(&out_path, &png_data).unwrap_or_else(|e| {
                        eprintln!("  ERROR: Failed to write {}: {e}", out_path.display());
                    });
                    println!("         -> saved {filename} ({} bytes)", png_data.len());

                    seen.push(cache_key);
                    texture_count += 1;
                }
            }
        }
    }

    println!(
        "\nDone. Extracted {} unique textures to {}",
        seen.len(),
        out_dir.display()
    );
    if texture_count > seen.len() {
        println!(
            "  ({} total references, {} were duplicates)",
            texture_count,
            texture_count - seen.len(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{TlutDesc, texture_cache_key};

    #[test]
    fn texture_cache_key_distinguishes_all_decoder_inputs() {
        let palette = TlutDesc {
            data_ptr: Some(0x200),
            format: 0,
            color_count: 16,
        };
        let base = texture_cache_key(0x100, 8, 8, 8, Some(&palette));

        assert_ne!(
            base,
            texture_cache_key(0x100, 16, 8, 8, Some(&palette)),
            "width must be part of the cache identity",
        );
        assert_ne!(
            base,
            texture_cache_key(0x100, 8, 4, 8, Some(&palette)),
            "height must be part of the cache identity",
        );
        assert_ne!(
            base,
            texture_cache_key(0x100, 8, 8, 9, Some(&palette)),
            "image format must be part of the cache identity",
        );

        let mut changed_palette = palette.clone();
        changed_palette.data_ptr = Some(0x204);
        assert_ne!(
            base,
            texture_cache_key(0x100, 8, 8, 8, Some(&changed_palette)),
            "palette data must be part of the cache identity",
        );
        changed_palette.data_ptr = palette.data_ptr;
        changed_palette.format = 1;
        assert_ne!(
            base,
            texture_cache_key(0x100, 8, 8, 8, Some(&changed_palette)),
            "palette format must be part of the cache identity",
        );
        changed_palette.format = palette.format;
        changed_palette.color_count = 32;
        assert_ne!(
            base,
            texture_cache_key(0x100, 8, 8, 8, Some(&changed_palette)),
            "palette color count must be part of the cache identity",
        );
    }
}
