use crate::fill::Generator;
use crate::model::{Graph, Node, NodeId, TestReport};

/// Prompt asking for small `#[test]` functions for one subject node, from the
/// spec and the signatures (never bodies) of the subject and its dependencies.
/// Tests must use unique temp directories and assert only what the spec states.
pub fn build_prompt(spec: &str, graph: &Graph, subject: &NodeId, errors: &[String]) -> String {
    let _ = (spec, graph, subject, errors);
    todo!()
}

/// Parse a model response into Test nodes with ids `test::<subject>::<name>`,
/// placed in a tests module after the subject's file, with edges added by the
/// caller. Err when no `#[test]` function is found.
pub fn parse_tests(graph: &Graph, subject: &NodeId, response: &str) -> Result<Vec<Node>, String> {
    let _ = (graph, subject, response);
    todo!()
}

/// Generate tests for every unfilled non-test node. After each node's tests are
/// added, `check` runs them against the skeleton: they must compile and must
/// fail (red). Tests that do not compile or that pass against `todo!()` are
/// regenerated with the reason in the prompt, up to `max_retries`. Returns the
/// subjects that never got an acceptable test set.
pub fn generate_all(
    graph: &mut Graph,
    spec: &str,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> TestReport,
    max_retries: usize,
) -> Vec<(NodeId, String)> {
    let _ = (graph, spec, generator, check, max_retries);
    todo!()
}
