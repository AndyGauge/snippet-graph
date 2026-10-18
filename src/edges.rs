use crate::model::Graph;

/// Add method-call edges to `graph.uses` on top of the identifier-name edges
/// already present: `Type::method(..)` links to the `Type::method` node, and
/// `x.method(..)` links to every Method node named `method` (all candidates
/// when ambiguous). Test nodes get edges to what their bodies call. Must be
/// idempotent and must never remove existing edges or add self-edges.
pub fn resolve(graph: &mut Graph) {
    let _ = graph;
}
