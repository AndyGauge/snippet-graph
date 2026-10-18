use crate::fill::{Generator, Outcome};
use crate::model::{Graph, NodeId};
use std::collections::BTreeMap;

/// Fill each dependency level (`order::levels`) concurrently, one worker per
/// generator, then check the level before moving on. Results are merged in
/// `NodeId` order so output matches a serial run.
pub fn fill_parallel(
    graph: &mut Graph,
    generators: &mut [Box<dyn Generator + Send>],
    check: &(dyn Fn(&Graph) -> BTreeMap<NodeId, Vec<String>> + Sync),
    max_retries: usize,
) -> Outcome {
    let _ = (graph, generators, check, max_retries);
    todo!()
}
