use dat_parser::descriptor::traversal;

pub fn report_traversal_issues<T>(
    context: &str,
    outcome: traversal::TraversalOutcome<T>,
) -> Vec<T> {
    for issue in outcome.issues {
        eprintln!("  {context}: {issue}");
    }
    outcome.nodes
}
