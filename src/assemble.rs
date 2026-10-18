use crate::model::{Graph, Node, NodeId, NodeKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Rendered source for one node: where it landed, as 1-based inclusive line numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: PathBuf,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct Assembled {
    pub files: BTreeMap<PathBuf, String>,
    pub spans: BTreeMap<NodeId, Span>,
}

/// Render the graph to files. Nodes are ordered by `Placement::index` within a
/// file; Method nodes sharing a file and `impl_header` are grouped into one impl
/// block positioned at the lowest index among them. Output is deterministic.
pub fn assemble(graph: &Graph) -> Assembled {
    let mut per_file: BTreeMap<&Path, Vec<Item>> = BTreeMap::new();
    let mut impls: BTreeMap<(&Path, &str), Vec<&Node>> = BTreeMap::new();

    for node in graph.nodes.values() {
        let file = node.placement.file.as_path();
        match (&node.kind, &node.impl_header) {
            (NodeKind::Method, Some(header)) => {
                impls.entry((file, header.as_str())).or_default().push(node)
            }
            _ => per_file.entry(file).or_default().push(Item::Single(node)),
        }
    }
    for ((file, header), mut methods) in impls {
        methods.sort_by(|a, b| (a.placement.index, &a.id).cmp(&(b.placement.index, &b.id)));
        per_file
            .entry(file)
            .or_default()
            .push(Item::Impl(header, methods));
    }

    let mut files = BTreeMap::new();
    let mut spans = BTreeMap::new();
    for (file, mut items) in per_file {
        items.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        let mut out = String::new();
        let mut line = 1usize;
        for (n, item) in items.iter().enumerate() {
            if n > 0 {
                out.push('\n');
                line += 1;
            }
            match item {
                Item::Single(node) => {
                    let text = node.render();
                    let count = text.lines().count().max(1);
                    record(&mut spans, node, file, line, line + count - 1);
                    out.push_str(&text);
                    out.push('\n');
                    line += count;
                }
                Item::Impl(header, methods) => {
                    out.push_str(header);
                    out.push_str(" {\n");
                    line += 1;
                    for (m, node) in methods.iter().enumerate() {
                        if m > 0 {
                            out.push('\n');
                            line += 1;
                        }
                        let text = indent(&node.render());
                        let count = text.lines().count().max(1);
                        record(&mut spans, node, file, line, line + count - 1);
                        out.push_str(&text);
                        out.push('\n');
                        line += count;
                    }
                    out.push_str("}\n");
                    line += 1;
                }
            }
        }
        files.insert(file.to_path_buf(), out);
    }
    Assembled { files, spans }
}

enum Item<'a> {
    Single(&'a Node),
    Impl(&'a str, Vec<&'a Node>),
}

impl Item<'_> {
    fn sort_key(&self) -> (usize, &NodeId) {
        match self {
            Item::Single(n) => (n.placement.index, &n.id),
            Item::Impl(_, methods) => (methods[0].placement.index, &methods[0].id),
        }
    }
}

fn record(spans: &mut BTreeMap<NodeId, Span>, node: &Node, file: &Path, start: usize, end: usize) {
    spans.insert(
        node.id.clone(),
        Span {
            file: file.to_path_buf(),
            start_line: start,
            end_line: end,
        },
    );
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Placement;

    fn node(
        id: &str,
        kind: NodeKind,
        sig: &str,
        body: Option<&str>,
        hdr: Option<&str>,
        file: &str,
        index: usize,
    ) -> Node {
        Node {
            id: NodeId(id.into()),
            kind,
            signature: sig.into(),
            body: body.map(Into::into),
            impl_header: hdr.map(Into::into),
            placement: Placement {
                file: file.into(),
                index,
            },
        }
    }

    fn sample() -> Graph {
        let mut g = Graph::default();
        g.insert(node(
            "T",
            NodeKind::Type,
            "struct Foo;",
            None,
            None,
            "a.rs",
            0,
        ));
        g.insert(node(
            "m2",
            NodeKind::Method,
            "fn b(&self)",
            Some("{\n    1;\n}"),
            Some("impl Foo"),
            "a.rs",
            3,
        ));
        g.insert(node(
            "f",
            NodeKind::Function,
            "fn f()",
            None,
            None,
            "a.rs",
            2,
        ));
        g.insert(node(
            "m1",
            NodeKind::Method,
            "fn a(&self)",
            None,
            Some("impl Foo"),
            "a.rs",
            1,
        ));
        g.insert(node(
            "u",
            NodeKind::Use,
            "use std::fmt;",
            None,
            None,
            "b.rs",
            0,
        ));
        g
    }

    #[test]
    fn orders_and_groups() {
        let a = assemble(&sample());
        let expected = "struct Foo;\n\nimpl Foo {\n    fn a(&self) { todo!() }\n\n    fn b(&self) {\n        1;\n    }\n}\n\nfn f() { todo!() }\n";
        assert_eq!(a.files[Path::new("a.rs")], expected);
        assert_eq!(a.files[Path::new("b.rs")], "use std::fmt;\n");
    }

    #[test]
    fn spans_match_rendered_lines() {
        let g = sample();
        let a = assemble(&g);
        let text = &a.files[Path::new("a.rs")];
        let lines: Vec<&str> = text.lines().collect();
        let span = |id: &str| &a.spans[&NodeId(id.into())];
        assert_eq!((span("T").start_line, span("T").end_line), (1, 1));
        assert_eq!((span("m1").start_line, span("m1").end_line), (4, 4));
        assert_eq!((span("m2").start_line, span("m2").end_line), (6, 8));
        assert_eq!((span("f").start_line, span("f").end_line), (11, 11));
        assert_eq!(lines[3], "    fn a(&self) { todo!() }");
        assert_eq!(lines[5], "    fn b(&self) {");
        assert_eq!(lines[7], "    }");
        assert_eq!(lines[10], "fn f() { todo!() }");
        assert_eq!(span("u").file, PathBuf::from("b.rs"));
    }

    #[test]
    fn deterministic() {
        let g = sample();
        assert_eq!(assemble(&g).files, assemble(&g).files);
    }
}
