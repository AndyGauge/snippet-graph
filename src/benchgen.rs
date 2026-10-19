use crate::fill::Generator;
use crate::model::{Graph, Node, NodeId};

pub fn build_prompt(spec: &str, graph: &Graph, subject: &NodeId, errors: &[String]) -> String {
    let _ = (spec, graph, subject, errors);
    todo!()
}

/// Parse a response into Bench nodes with ids `bench::<subject>::<name>` and
/// edges to the subject, each timing a realistic workload and printing
/// `BENCH <ns>`.
pub fn parse_benches(graph: &Graph, subject: &NodeId, response: &str) -> Result<Vec<Node>, String> {
    let _ = (graph, subject, response);
    todo!()
}

/// Generate benches for `subject`; `check` must confirm they compile and run
/// against the current implementation. Retries with the reason up to `max_retries`.
pub fn generate(
    graph: &mut Graph,
    spec: &str,
    subject: &NodeId,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> Result<(), String>,
    max_retries: usize,
) -> Result<(), String> {
    let _ = (graph, spec, subject, generator, check, max_retries);
    todo!()
}
