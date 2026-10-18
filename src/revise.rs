use crate::model::{Graph, NodeId};

/// Replace a node's signature, reset the bodies of the node and every
/// transitive dependent to unfilled, and return the invalidated ids. Test
/// nodes are not reset.
pub fn apply_signature(graph: &mut Graph, id: &NodeId, new_signature: &str) -> Vec<NodeId> {
    let _ = (graph, id, new_signature);
    todo!()
}
