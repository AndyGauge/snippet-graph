use crate::model::{Graph, Node, NodeId};
use std::collections::BTreeMap;

pub trait Generator {
    fn generate(&mut self, prompt: &str) -> Result<String, String>;
}

pub struct Outcome {
    pub filled: Vec<NodeId>,
    pub failed: Vec<(NodeId, String)>,
}

/// Prompt for one node: its own signature plus the signatures (never bodies) of
/// the nodes it uses, and any prior compiler errors for this node.
pub fn build_prompt(graph: &Graph, id: &NodeId, prior_errors: &[String]) -> String {
    let mut out = String::new();
    if let Some(node) = graph.nodes.get(id) {
        out.push_str("Implement the body of this Rust item. Respond with only the body as a single `{ ... }` block.\n\n");
        if let Some(header) = &node.impl_header {
            out.push_str(&format!("Inside `{header}`:\n"));
        }
        out.push_str(&format!("```rust\n{}\n```\n", node.signature));

        let deps: Vec<&Node> = graph
            .dependencies(id)
            .filter_map(|d| graph.nodes.get(d))
            .collect();
        if !deps.is_empty() {
            out.push_str("\nAvailable items it may use (signatures only):\n```rust\n");
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
    if !prior_errors.is_empty() {
        out.push_str("\nYour previous attempt failed with these errors; fix them:\n");
        for e in prior_errors {
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

/// Extract a single `{ ... }` block from model output, stripping markdown fences.
pub fn extract_body(response: &str) -> Option<String> {
    let text = unfenced(response);
    let chars: Vec<char> = text.chars().collect();
    let open = chars.iter().position(|&c| c == '{')?;
    let mut depth = 0usize;
    let mut i = open;
    while i < chars.len() {
        match chars[i] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(chars[open..=i].iter().collect());
                }
            }
            '"' => i = skip_string(&chars, i),
            'r' if is_raw_start(&chars, i) => i = skip_raw_string(&chars, i),
            '\'' => i = skip_char_literal(&chars, i),
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => i = skip_block_comment(&chars, i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_string(chars: &[char], start: usize) -> usize {
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '"' => return i,
            _ => {}
        }
        i += 1;
    }
    chars.len()
}

fn is_raw_start(chars: &[char], i: usize) -> bool {
    if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        return false;
    }
    let mut j = i + 1;
    while chars.get(j) == Some(&'#') {
        j += 1;
    }
    chars.get(j) == Some(&'"')
}

fn skip_raw_string(chars: &[char], start: usize) -> usize {
    let mut j = start + 1;
    let mut hashes = 0;
    while chars[j] == '#' {
        hashes += 1;
        j += 1;
    }
    let mut i = j + 1;
    while i < chars.len() {
        if chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
            return i + hashes;
        }
        i += 1;
    }
    chars.len()
}

fn skip_char_literal(chars: &[char], start: usize) -> usize {
    if chars.get(start + 1) == Some(&'\\') {
        let mut i = start + 3;
        while i < chars.len() && chars[i] != '\'' {
            i += 1;
        }
        return i;
    }
    if chars.get(start + 2) == Some(&'\'') {
        return start + 2;
    }
    start
}

fn skip_block_comment(chars: &[char], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '/' && chars[i + 1] == '*' {
            depth += 1;
            i += 2;
        } else if chars[i] == '*' && chars[i + 1] == '/' {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i - 1;
            }
        } else {
            i += 1;
        }
    }
    chars.len()
}

/// Fill unfilled nodes in dependency order, one at a time. After each fill, call
/// `check` with the current graph; if it reports errors attributed to that node,
/// retry up to `max_retries` times with the errors in the prompt, then mark the
/// node failed and continue.
pub fn fill_all(
    graph: &mut Graph,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> BTreeMap<NodeId, Vec<String>>,
    max_retries: usize,
) -> Outcome {
    let mut outcome = Outcome {
        filled: Vec::new(),
        failed: Vec::new(),
    };
    let order = match crate::order::topological(graph) {
        Ok(order) => order,
        Err(_) => graph.nodes.keys().cloned().collect(),
    };

    for id in order {
        match graph.nodes.get(&id) {
            Some(node) if !node.is_filled() => {}
            _ => continue,
        }
        let mut errors: Vec<String> = Vec::new();
        let mut succeeded = false;
        for _ in 0..=max_retries {
            let prompt = build_prompt(graph, &id, &errors);
            let attempt = generator.generate(&prompt).and_then(|r| {
                extract_body(&r).ok_or_else(|| "no `{ ... }` block in response".to_string())
            });
            match attempt {
                Err(e) => errors = vec![e],
                Ok(body) => {
                    if let Some(node) = graph.nodes.get_mut(&id) {
                        node.body = Some(body);
                    }
                    errors = check(graph).remove(&id).unwrap_or_default();
                    if errors.is_empty() {
                        succeeded = true;
                        break;
                    }
                }
            }
        }
        if succeeded {
            outcome.filled.push(id);
        } else {
            if let Some(node) = graph.nodes.get_mut(&id) {
                node.body = None;
            }
            outcome.failed.push((id, errors.join("\n")));
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NodeKind, Placement};
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

    #[test]
    fn extract_strips_fences() {
        let r = "Here:\n```rust\n{ let x = 1; x }\n```\nDone";
        assert_eq!(extract_body(r).unwrap(), "{ let x = 1; x }");
    }

    #[test]
    fn extract_handles_nested_and_literals() {
        let r = "fn f() { if a { \"}\" } else { '}' } // }\n /* } */ r#\"}\"# }\ntrailing }";
        let b = extract_body(r).unwrap();
        assert!(b.ends_with("r#\"}\"# }"), "{b}");
    }

    #[test]
    fn extract_lifetime_is_not_char() {
        let b = extract_body("{ let x: &'a str = \"\"; }").unwrap();
        assert_eq!(b, "{ let x: &'a str = \"\"; }");
    }

    #[test]
    fn extract_unbalanced_is_none() {
        assert!(extract_body("{ oops").is_none());
        assert!(extract_body("no braces").is_none());
    }

    #[test]
    fn prompt_has_signatures_not_bodies() {
        let mut g = Graph::default();
        g.insert(func("a", "fn a() -> u32"));
        let mut b = func("b", "fn b() -> u32");
        b.body = Some("{ SECRET }".into());
        g.insert(b);
        g.add_use(&nid("a"), &nid("b"));
        let p = build_prompt(&g, &nid("a"), &["boom".into()]);
        assert!(p.contains("fn a() -> u32"));
        assert!(p.contains("fn b() -> u32"));
        assert!(!p.contains("SECRET"));
        assert!(p.contains("boom"));
    }

    #[test]
    fn fill_all_retries_then_fails_and_continues() {
        let mut g = Graph::default();
        g.insert(func("a", "fn a()"));
        g.insert(func("b", "fn b()"));
        let mut gen = Fake(
            VecDeque::from([
                Ok("{ bad }".to_string()),
                Ok("{ still bad }".to_string()),
                Ok("{ good }".to_string()),
            ]),
            Vec::new(),
        );
        let mut check = |g: &Graph| {
            let mut m = BTreeMap::new();
            for (id, n) in &g.nodes {
                if n.body.as_deref().is_some_and(|b| b.contains("bad")) {
                    m.insert(id.clone(), vec!["err".to_string()]);
                }
            }
            m
        };
        let out = fill_all(&mut g, &mut gen, &mut check, 1);
        assert_eq!(out.failed.len(), 1);
        assert_eq!(out.filled.len(), 1);
        let failed = &out.failed[0].0;
        assert!(g.nodes[failed].body.is_none());
        assert!(gen.1[1].contains("err"));
    }
}
