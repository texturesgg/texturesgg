//! Parse a .dat file and print its structure for debugging/verification.

use dat_parser::DatFile;
use dat_parser::descriptor::{mobj, traversal};
use dat_parser::gx::vertex;
use std::env;
use std::fs;

mod support;
use support::report_traversal_issues;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: parse-dat <dat_file>");
        std::process::exit(1);
    }

    let path = &args[1];
    eprintln!("Parsing: {path}");

    let raw = fs::read(path).expect("Failed to read file");
    let dat = DatFile::parse(&raw).expect("Failed to parse .dat file");

    print!("{dat}");

    // Print relocation-site statistics
    println!(
        "\n  Relocation sites: {} entries",
        dat.relocation_sites.len()
    );
    if let (Some(first), Some(last)) = (dat.relocation_sites.first(), dat.relocation_sites.last()) {
        println!("    Range: 0x{first:08X} .. 0x{last:08X}");
    }

    // Walk JObj tree from the primary _joint root (skip matanim)
    for root in &dat.roots {
        if root.name.ends_with("_joint") && !root.name.contains("matanim") {
            println!(
                "\n=== JObj Tree: {} @ 0x{:08X} ===",
                root.name, root.data_offset
            );
            let joints = report_traversal_issues(
                "JObj traversal",
                traversal::walk_joint_tree(&dat, root.data_offset, dat.data.len()),
            );
            println!("  Total joints: {}", joints.len());

            let mut total_vertices = 0usize;
            let mut total_triangles = 0usize;
            let mut total_dobjs = 0usize;
            let mut total_pobjs = 0usize;
            let mut total_textures = 0usize;
            let mut sample_vertices = [None; 5];
            let mut sample_count = 0;

            // Walk DObjs for each joint
            for joint in &joints {
                if let Some(dobj_ptr) = joint.jobj.dobj_ptr {
                    let dobjs = report_traversal_issues(
                        "DObj traversal",
                        traversal::read_dobj_list(&dat, dobj_ptr, dat.data.len()),
                    );
                    total_dobjs += dobjs.len();

                    for d in &dobjs {
                        // Parse material
                        if let Some(mobj_ptr) = d.mobj_ptr {
                            match mobj::MObj::parse(&dat, mobj_ptr) {
                                Ok(m) => {
                                    if let Some(tobj_ptr) = m.tobj_ptr {
                                        let tobjs = report_traversal_issues(
                                            "TObj traversal",
                                            traversal::read_tobj_list(
                                                &dat,
                                                tobj_ptr,
                                                dat.data.len(),
                                            ),
                                        );
                                        total_textures += tobjs.len();
                                    }
                                }
                                Err(error) => {
                                    eprintln!("  Invalid MObj at 0x{mobj_ptr:08X}: {error}");
                                }
                            }
                        }

                        // Parse polygon objects
                        if let Some(pobj_ptr) = d.pobj_ptr {
                            let pobjs = report_traversal_issues(
                                "PObj traversal",
                                traversal::read_pobj_list(&dat, pobj_ptr, dat.data.len()),
                            );
                            total_pobjs += pobjs.len();

                            for p in &pobjs {
                                if p.display_list_offset.is_some() {
                                    let groups = support::read_display_list(&dat, p);
                                    let mesh =
                                        vertex::decode_primitives(&dat, &p.attributes, &groups);
                                    total_vertices += mesh.vertices.len();
                                    total_triangles += mesh.triangles.len();
                                    for v in mesh.vertices.iter().take(5 - sample_count) {
                                        sample_vertices[sample_count] =
                                            Some((v.position, v.normal, v.tex_coords[0]));
                                        sample_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            println!("\n  === Mesh Statistics ===");
            println!("  DObjs:     {total_dobjs}");
            println!("  PObjs:     {total_pobjs}");
            println!("  Vertices:  {total_vertices}");
            println!("  Triangles: {total_triangles}");
            println!("  Textures:  {total_textures}");

            if sample_count > 0 {
                println!("\n  === Sample Vertices (first 5) ===");
                for (position, normal, uv) in sample_vertices.into_iter().flatten() {
                    println!(
                        "    pos=({:.4}, {:.4}, {:.4}) nrm=({:.4}, {:.4}, {:.4}) uv=({:.4}, {:.4})",
                        position[0],
                        position[1],
                        position[2],
                        normal[0],
                        normal[1],
                        normal[2],
                        uv[0],
                        uv[1],
                    );
                }
            }
        }
    }
}
