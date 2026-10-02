use dat_parser::DatFile;
use dat_parser::descriptor::{pobj::PObj, traversal};
use dat_parser::gx::display_list::{self, DisplayListLimits, PrimitiveGroup};
use dat_parser::hsd::scene::HsdSceneLimits;

/// A PObj's primitive groups, each list held to the scene's budgets. A list
/// that cannot be read is reported and gives no groups.
pub fn read_display_list(dat: &DatFile, pobj: &PObj) -> Vec<PrimitiveGroup> {
    let Some(offset) = pobj.display_list_offset else {
        return Vec::new();
    };
    let limits = HsdSceneLimits::default();
    display_list::parse_display_list(
        dat,
        offset,
        pobj.display_list_size,
        &pobj.attributes,
        DisplayListLimits {
            vertices: limits.max_vertices,
            primitive_groups: limits.max_primitive_groups,
            vertex_attribute_decodes: limits.max_vertex_attribute_decodes,
        },
    )
    .unwrap_or_else(|error| {
        eprintln!("  PObj at 0x{:08X}: {error}", pobj.offset);
        Vec::new()
    })
}

pub fn report_traversal_issues<T>(
    context: &str,
    outcome: traversal::TraversalOutcome<T>,
) -> Vec<T> {
    for issue in outcome.issues {
        eprintln!("  {context}: {issue}");
    }
    outcome.nodes
}
