use crate::fill::Generator;
use crate::model::{Graph, Node, NodeId, NodeKind, Placement, TestReport};
use proc_macro2::{TokenStream, TokenTree};
use std::collections::BTreeSet;

const TEST_INDEX_BASE: usize = 1_000_000;

/// Prompt asking for small `#[test]` functions for one subject node, from the
/// spec and the signatures (never bodies) of the subject and its dependencies.
/// Tests must use unique temp directories and assert only what the spec states.
pub fn build_prompt(spec: &str, graph: &Graph, subject: &NodeId, errors: &[String]) -> String {
    let mut out = String::new();
    out.push_str("Write Rust unit tests for one function, derived only from the specification below.\n");
    out.push_str("Rules:\n");
    out.push_str("- Respond with only `#[test] fn name() { ... }` functions; each test must be self-contained (no helper items).\n");
    out.push_str("- Keep tests small and aimed at the single function under test.\n");
    out.push_str("- Assert only what the specification states; do not guess unspecified behavior.\n");
    out.push_str("- Do not rely on any other function being correct beyond its signature.\n");
    out.push_str("- Anything touching the filesystem must use a unique directory under `std::env::temp_dir()` (include the test name and process id), never a shared path.\n");
    out.push_str("- The tests run inside `mod tests` with `use super::*;`.\n");
    out.push_str("\nSpecification:\n");
    out.push_str(spec);
    out.push('\n');

    if let Some(node) = graph.nodes.get(subject) {
        out.push_str("\nFunction under test (signature only):\n```rust\n");
        if let Some(header) = &node.impl_header {
            out.push_str(&format!("// in {header}\n"));
        }
        out.push_str(&node.signature);
        out.push_str("\n```\n");

        let deps: Vec<&Node> = graph
            .dependencies(subject)
            .filter_map(|d| graph.nodes.get(d))
            .filter(|n| n.kind != NodeKind::Test)
            .collect();
        if !deps.is_empty() {
            out.push_str("\nRelated items (signatures only):\n```rust\n");
            for dep in deps {
                if let Some(header) = &dep.impl_header {
                    out.push_str(&format!("// in {header}\n"));
                }
                out.push_str(&dep.signature);
                out.push('\n');
            }
            out.push_str("```\n");
        }
    }
    if !errors.is_empty() {
        out.push_str("\nYour previous attempt was rejected; fix these problems:\n");
        for e in errors {
            out.push_str(&format!("- {e}\n"));
        }
    }
    out
}

fn unfenced(response: &str) -> &str {
    let Some(start) = response.find("```") else {
        return response;
    };
    let after = &response[start + 3..];
    let content_start = after.find('\n').map_or(0, |i| i + 1);
    let content = &after[content_start..];
    match content.find("```") {
        Some(end) => &content[..end],
        None => content,
    }
}

fn is_test_fn(f: &syn::ItemFn) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("test"))
}

fn collect_tests<'a>(items: &'a [syn::Item], out: &mut Vec<&'a syn::ItemFn>) {
    for item in items {
        match item {
            syn::Item::Fn(f) if is_test_fn(f) => out.push(f),
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    collect_tests(inner, out);
                }
            }
            _ => {}
        }
    }
}

fn byte_offset(src: &str, line: usize, column: usize) -> Option<usize> {
    let mut start = 0;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            let off = l.char_indices().nth(column).map_or(l.len(), |(b, _)| b);
            return Some(start + off);
        }
        start += l.len();
    }
    None
}

fn block_text(src: &str, block: &syn::Block) -> String {
    let open = block.brace_token.span.open().start();
    let close = block.brace_token.span.close().end();
    let sliced = byte_offset(src, open.line, open.column)
        .zip(byte_offset(src, close.line, close.column))
        .filter(|(a, b)| a < b)
        .map(|(a, b)| src[a..b].to_string());
    sliced.unwrap_or_else(|| {
        let ts = quote::quote!(#block);
        ts.to_string()
    })
}

/// Parse a model response into Test nodes with ids `test::<subject>::<name>`,
/// placed in a tests module after the subject's file, with edges added by the
/// caller. Err when no `#[test]` function is found.
pub fn parse_tests(graph: &Graph, subject: &NodeId, response: &str) -> Result<Vec<Node>, String> {
    let subject_node = graph
        .nodes
        .get(subject)
        .ok_or_else(|| format!("unknown subject {}", subject.0))?;
    let src = unfenced(response);
    let file = syn::parse_file(src).map_err(|e| format!("tests do not parse: {e}"))?;
    let mut fns = Vec::new();
    collect_tests(&file.items, &mut fns);
    if fns.is_empty() {
        return Err("no `#[test]` function found in response".into());
    }
    let existing = graph
        .nodes
        .values()
        .filter(|n| n.placement.file == subject_node.placement.file)
        .count();
    let mut seen = BTreeSet::new();
    let mut nodes = Vec::new();
    for f in fns {
        let name = f.sig.ident.to_string();
        if !seen.insert(name.clone()) {
            return Err(format!("duplicate test name `{name}`"));
        }
        nodes.push(Node {
            id: NodeId(format!("test::{}::{}", subject.0, name)),
            kind: NodeKind::Test,
            signature: format!("#[test] fn {name}()"),
            body: Some(block_text(src, &f.block)),
            impl_header: None,
            placement: Placement {
                file: subject_node.placement.file.clone(),
                index: TEST_INDEX_BASE + existing + nodes.len(),
            },
        });
    }
    Ok(nodes)
}

fn idents(ts: TokenStream, out: &mut BTreeSet<String>) {
    for tt in ts {
        match tt {
            TokenTree::Ident(i) => {
                out.insert(i.to_string());
            }
            TokenTree::Group(g) => idents(g.stream(), out),
            _ => {}
        }
    }
}

fn link_test(graph: &mut Graph, test: &Node, subject: &NodeId) {
    let mut used = BTreeSet::new();
    if let Some(body) = &test.body {
        if let Ok(ts) = body.parse::<TokenStream>() {
            idents(ts, &mut used);
        }
    }
    let called: Vec<NodeId> = graph
        .nodes
        .values()
        .filter(|n| !matches!(n.kind, NodeKind::Test | NodeKind::Use))
        .filter(|n| {
            n.id.0
                .rsplit("::")
                .next()
                .is_some_and(|last| used.contains(last))
        })
        .map(|n| n.id.clone())
        .collect();
    graph.add_use(&test.id, subject);
    for id in called {
        graph.add_use(&test.id, &id);
    }
}

fn remove_tests(graph: &mut Graph, ids: &[NodeId]) {
    for id in ids {
        graph.nodes.remove(id);
        graph.uses.remove(id);
    }
}

fn rejection(report: &TestReport, new_ids: &[NodeId]) -> Option<String> {
    if !report.compiled {
        let mut lines: Vec<String> = Vec::new();
        for (id, errs) in &report.build_errors {
            if new_ids.contains(id) {
                lines.extend(errs.iter().cloned());
            }
        }
        for (id, errs) in &report.build_errors {
            if !new_ids.contains(id) {
                lines.extend(errs.iter().map(|e| format!("{}: {e}", id.0)));
            }
        }
        lines.extend(report.unattributed.iter().cloned());
        if lines.is_empty() {
            lines.push("tests did not compile".into());
        }
        return Some(format!("tests did not compile:\n{}", lines.join("\n")));
    }
    let mut problems = Vec::new();
    for id in new_ids {
        match report.outcomes.iter().find(|o| &o.test == id) {
            None => problems.push(format!("{} was not run", id.0)),
            Some(o) if o.passed => problems.push(format!(
                "{} passed against the unimplemented skeleton; it must fail until the function is implemented",
                id.0
            )),
            Some(_) => {}
        }
    }
    (!problems.is_empty()).then(|| problems.join("\n"))
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
    let subjects: Vec<NodeId> = graph
        .nodes
        .values()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))
        .map(|n| n.id.clone())
        .collect();
    let mut failed = Vec::new();

    for subject in subjects {
        let mut errors: Vec<String> = Vec::new();
        let mut accepted = false;
        for _ in 0..=max_retries {
            let prompt = build_prompt(spec, graph, &subject, &errors);
            let nodes = match generator
                .generate(&prompt)
                .and_then(|r| parse_tests(graph, &subject, &r))
            {
                Ok(nodes) => nodes,
                Err(e) => {
                    errors = vec![e];
                    continue;
                }
            };
            let ids: Vec<NodeId> = nodes.iter().map(|n| n.id.clone()).collect();
            for node in &nodes {
                graph.insert(node.clone());
                link_test(graph, node, &subject);
            }
            let report = check(graph);
            match rejection(&report, &ids) {
                None => {
                    accepted = true;
                    break;
                }
                Some(reason) => {
                    remove_tests(graph, &ids);
                    errors = vec![reason];
                }
            }
        }
        if !accepted {
            failed.push((subject, errors.join("\n")));
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TestOutcome;
    use std::collections::VecDeque;

    struct Fake(VecDeque<Result<String, String>>, Vec<String>);

    impl Generator for Fake {
        fn generate(&mut self, prompt: &str) -> Result<String, String> {
            self.1.push(prompt.to_string());
            self.0
                .pop_front()
                .unwrap_or_else(|| Err("exhausted".into()))
        }
    }

    fn func(name: &str, sig: &str) -> Node {
        Node {
            id: NodeId(name.into()),
            kind: NodeKind::Function,
            signature: sig.into(),
            body: None,
            impl_header: None,
            placement: Placement {
                file: "lib.rs".into(),
                index: 0,
            },
        }
    }

    fn nid(s: &str) -> NodeId {
        NodeId(s.into())
    }

    fn graph() -> Graph {
        let mut g = Graph::default();
        g.insert(func("add", "pub fn add(a: u32, b: u32) -> u32"));
        let mut h = func("helper", "fn helper() -> u32");
        h.body = Some("{ SECRET }".into());
        g.insert(h);
        g.add_use(&nid("add"), &nid("helper"));
        g
    }

    const GOOD: &str = "```rust\nmod tests {\n    use super::*;\n    #[test]\n    fn adds() {\n        assert_eq!(add(1, 2), 3);\n    }\n}\n```";

    fn report(compiled: bool, results: &[(&str, bool)]) -> TestReport {
        TestReport {
            compiled,
            outcomes: results
                .iter()
                .map(|(t, p)| TestOutcome {
                    test: nid(t),
                    passed: *p,
                    message: String::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn prompt_has_signatures_not_bodies() {
        let p = build_prompt("adds numbers", &graph(), &nid("add"), &["boom".into()]);
        assert!(p.contains("adds numbers"));
        assert!(p.contains("pub fn add(a: u32, b: u32) -> u32"));
        assert!(p.contains("fn helper() -> u32"));
        assert!(!p.contains("SECRET"));
        assert!(p.contains("boom"));
        assert!(p.contains("temp_dir"));
    }

    #[test]
    fn parse_extracts_test_nodes() {
        let nodes = parse_tests(&graph(), &nid("add"), GOOD).unwrap();
        assert_eq!(nodes.len(), 1);
        let n = &nodes[0];
        assert_eq!(n.id, nid("test::add::adds"));
        assert_eq!(n.kind, NodeKind::Test);
        assert_eq!(n.signature, "#[test] fn adds()");
        assert!(n.body.as_deref().unwrap().starts_with('{'));
        assert!(n.body.as_deref().unwrap().contains("assert_eq!(add(1, 2), 3);"));
        assert!(n.placement.index >= TEST_INDEX_BASE);
    }

    #[test]
    fn parse_rejects_missing_tests_and_bad_syntax() {
        assert!(parse_tests(&graph(), &nid("add"), "fn x() {}").is_err());
        assert!(parse_tests(&graph(), &nid("add"), "#[test] fn x( {").is_err());
    }

    #[test]
    fn generate_retries_on_compile_error_then_accepts() {
        let mut g = graph();
        g.nodes.get_mut(&nid("helper")).unwrap().body = Some("{ 1 }".into());
        g.nodes.get_mut(&nid("helper")).unwrap().kind = NodeKind::Const;
        let mut gen = Fake(
            VecDeque::from([Ok(GOOD.to_string()), Ok(GOOD.to_string())]),
            Vec::new(),
        );
        let mut calls = 0;
        let mut check = |_: &Graph| {
            calls += 1;
            if calls == 1 {
                let mut r = report(false, &[]);
                r.build_errors
                    .insert(nid("test::add::adds"), vec!["E0425 nope".into()]);
                r
            } else {
                report(true, &[("test::add::adds", false)])
            }
        };
        let failed = generate_all(&mut g, "spec", &mut gen, &mut check, 2);
        assert!(failed.is_empty());
        assert!(gen.1[1].contains("E0425 nope"));
        assert!(g.nodes.contains_key(&nid("test::add::adds")));
        assert!(g.dependencies(&nid("test::add::adds")).any(|d| d == &nid("add")));
        assert_eq!(g.tests_for(&nid("add")).len(), 1);
    }

    #[test]
    fn generate_rejects_passing_tests_and_removes_them() {
        let mut g = graph();
        let mut gen = Fake(
            VecDeque::from([Ok(GOOD.to_string()), Ok(GOOD.to_string())]),
            Vec::new(),
        );
        let mut check = |_: &Graph| report(true, &[("test::add::adds", true)]);
        let failed = generate_all(&mut g, "spec", &mut gen, &mut check, 1);
        assert_eq!(failed.len(), 2);
        assert_eq!(failed[0].0, nid("add"));
        assert!(failed[0].1.contains("passed"));
        assert!(!g.nodes.contains_key(&nid("test::add::adds")));
        assert!(!g.uses.contains_key(&nid("test::add::adds")));
    }
}
