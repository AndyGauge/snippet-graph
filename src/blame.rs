use crate::model::{Graph, NodeId, TestOutcome};

#[derive(Clone, Debug, PartialEq)]
pub struct Suspect {
    pub node: NodeId,
    pub score: f64,
}

/// Rank non-test nodes by how likely they are to be the cause of the failing
/// outcomes, highest score first, ties broken by `NodeId`. A node reached by a
/// passing test is down-weighted; a node reached by few failing tests, or only
/// by failing tests, and any node in `recently_filled`, is up-weighted. Nodes
/// not reached by any failing test never appear.
pub fn rank(graph: &Graph, outcomes: &[TestOutcome], recently_filled: &[NodeId]) -> Vec<Suspect> {
    let _ = (graph, outcomes, recently_filled);
    todo!()
}
