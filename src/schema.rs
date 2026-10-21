use crate::assemble::assemble;
use crate::fill::Generator;
use crate::model::{Graph, NodeId};
use crate::skeleton;
use crate::verify::Report;
use std::collections::BTreeMap;
use std::path::Path;

pub const REPAIR_ROUNDS: usize = 6;
const PARSE_ATTEMPTS: usize = 3;
const LIB: &str = "src/lib.rs";
const MANIFEST_MARK: &str = "Cargo.toml";
const LIB_MARK: &str = "src/lib.rs";
const REMOVE_MARK: &str = "remove";

pub struct Schema {
    pub dependencies: toml::Table,
    pub graph: Graph,
    pub notes: Vec<String>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Sections {
    pub dependencies: Option<String>,
    pub code: String,
    pub remove: Vec<String>,
}

fn marker(line: &str) -> Option<&str> {
    let inner = line.trim().strip_prefix("// ===")?.strip_suffix("===")?;
    Some(inner.trim())
}

/// Split a reply into its `// === Cargo.toml ===` and `// === src/lib.rs ===` sections.
/// A reply with no markers is all code.
pub fn split_sections(reply: &str) -> Sections {
    let mut current: Option<&str> = None;
    let mut deps = String::new();
    let mut code = String::new();
    let mut remove = Vec::new();
    let mut marked = false;
    for line in reply.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        match marker(line) {
            Some(MANIFEST_MARK) => {
                current = Some(MANIFEST_MARK);
                marked = true;
            }
            Some(LIB_MARK) => {
                current = Some(LIB_MARK);
                marked = true;
            }
            Some(REMOVE_MARK) => {
                current = Some(REMOVE_MARK);
                marked = true;
            }
            _ => match current {
                Some(MANIFEST_MARK) => {
                    deps.push_str(line);
                    deps.push('\n');
                }
                Some(REMOVE_MARK) => {
                    let name = line.trim().trim_matches('`');
                    if !name.is_empty() {
                        remove.push(name.to_string());
                    }
                }
                Some(_) => {
                    code.push_str(line);
                    code.push('\n');
                }
                None if !marked => {
                    code.push_str(line);
                    code.push('\n');
                }
                None => {}
            },
        }
    }
    Sections {
        dependencies: (!deps.trim().is_empty()).then(|| deps.trim().to_string()),
        code: code.trim().to_string(),
        remove,
    }
}

/// The `[dependencies]` table of a manifest section, which may be a whole manifest or just
/// that table.
pub fn parse_dependencies(section: &str) -> Result<toml::Table, String> {
    let value: toml::Value = section
        .parse()
        .map_err(|e| format!("the Cargo.toml section is not valid TOML ({e})"))?;
    Ok(value
        .get("dependencies")
        .and_then(|d| d.as_table())
        .cloned()
        .unwrap_or_default())
}

/// Overlay `dependencies` onto the crate's manifest.
pub fn apply_dependencies(crate_dir: &Path, dependencies: &toml::Table) -> Result<(), String> {
    if dependencies.is_empty() {
        return Ok(());
    }
    let path = crate_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut manifest: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let table = manifest
        .entry("dependencies")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .ok_or("the manifest's [dependencies] is not a table")?;
    for (name, spec) in dependencies {
        table.insert(name.clone(), spec.clone());
    }
    let out = toml::to_string(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| e.to_string())
}

fn source(graph: &Graph) -> String {
    assemble(graph)
        .files
        .into_iter()
        .find(|(path, _)| path == Path::new(LIB))
        .map(|(_, text)| text)
        .unwrap_or_default()
}

/// Render the schema the way the pipeline reads it: a manifest section, then the code.
pub fn render(schema: &Schema) -> String {
    let mut manifest = toml::Table::new();
    manifest.insert(
        "dependencies".into(),
        toml::Value::Table(schema.dependencies.clone()),
    );
    format!(
        "// === {MANIFEST_MARK} ===\n{}\n// === {LIB_MARK} ===\n{}",
        toml::to_string(&manifest).unwrap_or_default().trim_end(),
        source(&schema.graph)
    )
}

const RULES: &str = "Output only Rust source, no prose. Every function body is exactly `{ todo!() }`. \
Emit two sections: `// === Cargo.toml ===` holding a `[dependencies]` table with every external crate \
the code uses (only features that exist in the version you pick), then `// === src/lib.rs ===` with the code. \
Use one file, no `mod` blocks, no macros, and no associated types or consts inside impl blocks.";

pub fn first_prompt(spec: &str, guide: &str) -> String {
    let guide = if guide.trim().is_empty() {
        String::new()
    } else {
        format!("Design rules:\n{guide}\n\n")
    };
    format!(
        "Design the Rust type system and public function signatures for this work: every struct, enum, trait, \
         const, `use` item and function or method signature, with a `///` doc comment on each stating its \
         contract for callers who cannot see the body.\n{RULES}\n\n{guide}Spec:\n{spec}\n"
    )
}

pub fn fix_all_prompt(spec: &str, guide: &str, previous: &str, errors: &[String]) -> String {
    format!(
        "{}\nYour previous attempt:\n```rust\n{previous}\n```\n\nIt failed with:\n{}\n\nFix it and output the complete corrected schema.\n",
        first_prompt(spec, guide),
        errors.join("\n")
    )
}

pub fn repair_prompt(spec: &str, current: &str, failing: &BTreeMap<NodeId, Vec<String>>) -> String {
    let mut p = format!(
        "This Rust schema does not compile. Spec it was written for:\n{spec}\n\nThe schema now:\n```rust\n{current}\n```\n\n\
         These items fail:\n"
    );
    for (id, errors) in failing {
        p.push_str(&format!("\n## `{}`\n{}\n", id.0, errors.join("\n")));
    }
    p.push_str(&format!(
        "\nReply with the corrected failing items only, not the whole file. Keep each item's name. If the \
         fix lives in another item (a missing derive on a type, a missing `use`), restate that item too. \
         If a crate or feature is missing, add a `// === Cargo.toml ===` section with the `[dependencies]` it needs \
         before a `// === src/lib.rs ===` section.\n{RULES}\n"
    ));
    p
}

fn next_index(graph: &Graph) -> usize {
    graph
        .nodes
        .values()
        .map(|n| n.placement.index + 1)
        .max()
        .unwrap_or(0)
}

/// Swap the reply's items into the graph by name, keeping each replaced item where it was.
/// Items the graph did not have are appended. Returns the names that changed.
fn splice(graph: &mut Graph, reply: Graph) -> Vec<NodeId> {
    let mut at = next_index(graph);
    let mut changed = Vec::new();
    let mut incoming = reply;
    for node in incoming.nodes.values_mut() {
        match graph.nodes.get(&node.id) {
            Some(old) => node.placement.index = old.placement.index,
            None => {
                node.placement.index = at;
                at += 1;
            }
        }
        if graph.nodes.get(&node.id) != Some(node) {
            changed.push(node.id.clone());
        }
    }
    for id in incoming.nodes.keys() {
        graph.nodes.remove(id);
    }
    skeleton::merge(graph, incoming);
    changed
}

fn merge_dependencies(
    crate_dir: &Path,
    into: &mut toml::Table,
    section: Option<&str>,
) -> Result<(), String> {
    let Some(section) = section else {
        return Ok(());
    };
    let table = parse_dependencies(section)?;
    apply_dependencies(crate_dir, &table)?;
    into.extend(table);
    Ok(())
}

/// Build a compiling schema. The model writes the whole skeleton once; after that every
/// compile error is traced to the item it sits in and only those items are rewritten, so a
/// mistake in one type never costs the items that were already right.
pub fn build(
    crate_dir: &Path,
    spec: &str,
    guide: &str,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> std::io::Result<Report>,
    rounds: usize,
) -> Result<Schema, String> {
    let mut dependencies = toml::Table::new();
    let notes = Vec::new();

    let mut errors: Vec<String> = Vec::new();
    let mut previous = String::new();
    let mut graph = None;
    for attempt in 1..=PARSE_ATTEMPTS {
        eprintln!("schema draft {attempt}");
        let prompt = if errors.is_empty() {
            first_prompt(spec, guide)
        } else {
            fix_all_prompt(spec, guide, &previous, &errors)
        };
        let sections = split_sections(&generator.generate(&prompt)?);
        merge_dependencies(
            crate_dir,
            &mut dependencies,
            sections.dependencies.as_deref(),
        )?;
        match skeleton::parse(Path::new(LIB), &sections.code) {
            Ok(g) if !g.nodes.is_empty() => {
                graph = Some(g);
                break;
            }
            Ok(_) => errors = vec!["the code section had no items".to_string()],
            Err(e) => errors = vec![e.to_string()],
        }
        previous = sections.code;
    }
    let graph = graph.ok_or_else(|| {
        format!(
            "no parseable schema after {PARSE_ATTEMPTS} drafts: {}",
            errors.join(" ")
        )
    })?;

    converge(
        crate_dir,
        spec,
        guide,
        generator,
        check,
        rounds,
        graph,
        dependencies,
        notes,
    )
}

#[allow(clippy::too_many_arguments)]
/// Check the graph and repair the failing items until it compiles or the rounds run out.
fn converge(
    crate_dir: &Path,
    spec: &str,
    guide: &str,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> std::io::Result<Report>,
    rounds: usize,
    mut graph: Graph,
    mut dependencies: toml::Table,
    mut notes: Vec<String>,
) -> Result<Schema, String> {
    for round in 0..=rounds {
        let report = check(&graph).map_err(|e| e.to_string())?;
        if report.ok {
            return Ok(Schema {
                dependencies,
                graph,
                notes,
            });
        }
        let all: Vec<String> = report
            .by_node
            .values()
            .flatten()
            .chain(&report.unattributed)
            .cloned()
            .collect();
        if round == rounds {
            return Err(format!(
                "the schema still does not compile after {rounds} repair round(s):\n{}",
                all.join("\n")
            ));
        }
        if report.by_node.is_empty() {
            eprintln!(
                "schema repair {}: whole file, {} unplaced error(s)",
                round + 1,
                report.unattributed.len()
            );
            let prompt = fix_all_prompt(spec, guide, &source(&graph), &report.unattributed);
            let sections = split_sections(&generator.generate(&prompt)?);
            merge_dependencies(
                crate_dir,
                &mut dependencies,
                sections.dependencies.as_deref(),
            )?;
            match skeleton::parse(Path::new(LIB), &sections.code) {
                Ok(g) if !g.nodes.is_empty() => graph = g,
                _ => notes.push(format!(
                    "round {}: a whole-file rewrite did not parse, kept the schema as it was",
                    round + 1
                )),
            }
            continue;
        }
        let names: Vec<&str> = report.by_node.keys().map(|id| id.0.as_str()).collect();
        eprintln!("schema repair {}: {}", round + 1, names.join(", "));
        let prompt = repair_prompt(spec, &source(&graph), &report.by_node);
        let sections = split_sections(&generator.generate(&prompt)?);
        merge_dependencies(
            crate_dir,
            &mut dependencies,
            sections.dependencies.as_deref(),
        )?;
        match skeleton::parse(Path::new(LIB), &sections.code) {
            Ok(reply) => {
                let changed = splice(&mut graph, reply);
                notes.push(format!(
                    "round {}: {} failed; rewrote {}",
                    round + 1,
                    names.join(", "),
                    if changed.is_empty() {
                        "nothing".to_string()
                    } else {
                        changed
                            .iter()
                            .map(|id| id.0.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ));
            }
            Err(e) => notes.push(format!(
                "round {}: the reply did not parse ({e})",
                round + 1
            )),
        }
    }
    unreachable!("the last round returns")
}

/// What a change did to a schema, by item name.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub added: Vec<NodeId>,
    pub changed: Vec<NodeId>,
    pub removed: Vec<NodeId>,
}

impl Diff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }

    /// The items that need new tests and bodies: everything added or changed.
    pub fn to_fill(&self) -> Vec<NodeId> {
        self.added.iter().chain(&self.changed).cloned().collect()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for (mark, ids) in [("+", &self.added), ("~", &self.changed), ("-", &self.removed)] {
            for id in ids {
                out.push_str(&format!("{mark} {}\n", id.0));
            }
        }
        out
    }
}

pub fn diff(before: &Graph, after: &Graph) -> Diff {
    let mut out = Diff::default();
    for (id, node) in &after.nodes {
        match before.nodes.get(id) {
            None => out.added.push(id.clone()),
            Some(old) if old.render() != node.render() => out.changed.push(id.clone()),
            Some(_) => {}
        }
    }
    for id in before.nodes.keys() {
        if !after.nodes.contains_key(id) {
            out.removed.push(id.clone());
        }
    }
    out
}

pub struct Upgrade {
    pub schema: Schema,
    pub diff: Diff,
}

/// Read a schema in the form `render` writes: a manifest section, then the code.
pub fn parse_schema(text: &str) -> Result<(toml::Table, Graph), String> {
    let sections = split_sections(text);
    let dependencies = match sections.dependencies.as_deref() {
        Some(section) => parse_dependencies(section)?,
        None => toml::Table::new(),
    };
    let graph = skeleton::parse(Path::new(LIB), &sections.code).map_err(|e| e.to_string())?;
    Ok((dependencies, graph))
}

pub fn upgrade_prompt(spec: &str, guide: &str, current: &str, change: &str, error: &str) -> String {
    let guide = if guide.trim().is_empty() {
        String::new()
    } else {
        format!("Design rules:\n{guide}\n\n")
    };
    let mut p = format!(
        "This Rust schema already exists and compiles. Change it as requested.\n\n{guide}Spec it was written for:\n{spec}\n\n\
         The schema now:\n```rust\n{current}\n```\n\nThe change:\n{change}\n\n\
         Reply with only the items that are new or must change, each restated whole under `// === src/lib.rs ===`, \
         keeping each item's name. Items you do not restate stay exactly as they are. To delete items, list their \
         names one per line under `// === remove ===`. If the dependencies change, add a `// === Cargo.toml ===` \
         section with the complete `[dependencies]` table. Output only Rust source, no prose. Every function body \
         is exactly `{{ todo!() }}`. One file, no `mod` blocks, no macros, no associated types or consts in impl blocks.\n"
    );
    if !error.is_empty() {
        p.push_str(&format!("\nYour previous reply failed with:\n{error}\nReply again.\n"));
    }
    p
}

/// Apply a change request to an existing schema. The model restates only the items that move,
/// they are merged by name, and the same per-item repair as `build` brings the result back to
/// compiling. The diff says which items need fresh tests and bodies.
#[allow(clippy::too_many_arguments)]
pub fn upgrade(
    crate_dir: &Path,
    spec: &str,
    guide: &str,
    current: &str,
    change: &str,
    generator: &mut dyn Generator,
    check: &mut dyn FnMut(&Graph) -> std::io::Result<Report>,
    rounds: usize,
) -> Result<Upgrade, String> {
    let (mut dependencies, before) = parse_schema(current)?;
    apply_dependencies(crate_dir, &dependencies)?;
    let mut graph = before.clone();
    let mut notes = Vec::new();

    let mut error = String::new();
    let mut applied = false;
    for attempt in 1..=PARSE_ATTEMPTS {
        eprintln!("schema upgrade draft {attempt}");
        let reply = generator.generate(&upgrade_prompt(spec, guide, &source(&before), change, &error))?;
        let sections = split_sections(&reply);
        let incoming = if sections.code.is_empty() {
            Ok(Graph::default())
        } else {
            skeleton::parse(Path::new(LIB), &sections.code).map_err(|e| e.to_string())
        };
        let incoming = match incoming {
            Ok(g) => g,
            Err(e) => {
                error = e;
                continue;
            }
        };
        if incoming.nodes.is_empty() && sections.remove.is_empty() && sections.dependencies.is_none() {
            error = "the reply changed nothing".to_string();
            continue;
        }
        merge_dependencies(crate_dir, &mut dependencies, sections.dependencies.as_deref())?;
        for name in &sections.remove {
            if graph.nodes.remove(&NodeId(name.clone())).is_none() {
                notes.push(format!("asked to remove `{name}`, which the schema does not have"));
            }
        }
        splice(&mut graph, incoming);
        applied = true;
        break;
    }
    if !applied {
        return Err(format!("no usable upgrade after {PARSE_ATTEMPTS} replies: {error}"));
    }

    let spec = format!("{spec}\n\nChange being made to the schema:\n{change}");
    let schema = converge(
        crate_dir, &spec, guide, generator, check, rounds, graph, dependencies, notes,
    )?;
    let diff = diff(&before, &schema.graph);
    Ok(Upgrade { schema, diff })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Script(Vec<String>, Vec<String>);

    impl Generator for Script {
        fn generate(&mut self, prompt: &str) -> Result<String, String> {
            self.1.push(prompt.to_string());
            if self.0.is_empty() {
                return Err("script ran out".into());
            }
            Ok(self.0.remove(0))
        }
    }

    fn crate_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sg-schema-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();
        dir
    }

    fn rejecting(word: &'static str) -> impl FnMut(&Graph) -> std::io::Result<Report> {
        move |graph| {
            let mut report = Report {
                ok: true,
                ..Report::default()
            };
            for node in graph.nodes.values() {
                if node.render().contains(word) {
                    report.ok = false;
                    report
                        .by_node
                        .entry(node.id.clone())
                        .or_default()
                        .push(format!("{word} is not allowed"));
                }
            }
            Ok(report)
        }
    }

    const DRAFT: &str = "// === Cargo.toml ===\n[dependencies]\nserde = \"1\"\n\n// === src/lib.rs ===\npub struct Good;\n\npub struct Bad(BAD);\n\npub fn keep() -> Good { todo!() }\n";

    #[test]
    fn sections_split_and_a_bare_reply_is_all_code() {
        let s = split_sections("```\n// === Cargo.toml ===\n[dependencies]\nrand = \"0.8\"\n// === src/lib.rs ===\npub fn f() {}\n```");
        assert_eq!(
            s.dependencies.as_deref(),
            Some("[dependencies]\nrand = \"0.8\"")
        );
        assert_eq!(s.code, "pub fn f() {}");
        let bare = split_sections("pub fn g() {}");
        assert_eq!(bare.dependencies, None);
        assert_eq!(bare.code, "pub fn g() {}");
    }

    #[test]
    fn fence_lines_around_or_inside_sections_are_dropped() {
        let s = split_sections("// === Cargo.toml ===\n```toml\n[dependencies]\nrand = \"0.8\"\n```\n\n// === src/lib.rs ===\n```rust\npub fn f() {}\n```\n");
        assert_eq!(
            s.dependencies.as_deref(),
            Some("[dependencies]\nrand = \"0.8\"")
        );
        assert_eq!(s.code, "pub fn f() {}");
    }

    #[test]
    fn only_the_failing_item_is_rewritten_and_the_rest_is_kept() {
        let dir = crate_dir();
        let mut gen = Script(
            vec![DRAFT.into(), "pub struct Bad(u32);\n".into()],
            Vec::new(),
        );
        let schema = build(&dir, "spec", "", &mut gen, &mut rejecting("BAD"), 3).unwrap();
        let text = render(&schema);
        assert!(text.contains("pub struct Bad(u32);"), "{text}");
        assert!(text.contains("pub struct Good;"));
        assert!(text.contains("pub fn keep() -> Good"));
        assert!(text.contains("serde = \"1\""));
        assert_eq!(gen.1.len(), 2);
        assert!(gen.1[1].contains("## `Bad`"));
        assert!(gen.1[1].contains("not the whole file"));
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("serde"));
    }

    #[test]
    fn a_repair_may_restate_another_item_and_add_a_new_one() {
        let dir = crate_dir();
        let mut gen = Script(
            vec![
                DRAFT.into(),
                "pub struct Bad(u32);\n#[derive(Debug)]\npub struct Good;\npub struct Extra;\n"
                    .into(),
            ],
            Vec::new(),
        );
        let schema = build(&dir, "spec", "", &mut gen, &mut rejecting("BAD"), 3).unwrap();
        let text = render(&schema);
        assert!(text.contains("#[derive(Debug)]"));
        assert!(text.contains("pub struct Extra;"));
        assert_eq!(schema.graph.nodes.len(), 4);
    }

    #[test]
    fn a_schema_that_never_compiles_fails_with_the_errors() {
        let dir = crate_dir();
        let mut gen = Script(
            vec![
                DRAFT.into(),
                "pub struct Bad(BAD);\n".into(),
                "pub struct Bad(BAD);\n".into(),
            ],
            Vec::new(),
        );
        let err = build(&dir, "spec", "", &mut gen, &mut rejecting("BAD"), 2)
            .err()
            .unwrap();
        assert!(err.contains("still does not compile"), "{err}");
        assert!(err.contains("BAD is not allowed"));
    }

    #[test]
    fn an_unparseable_draft_is_asked_for_again_with_the_parse_error() {
        let dir = crate_dir();
        let mut gen = Script(
            vec!["pub struct {".into(), "pub struct Fine;\n".into()],
            Vec::new(),
        );
        let schema = build(&dir, "spec", "", &mut gen, &mut rejecting("BAD"), 1).unwrap();
        assert!(render(&schema).contains("pub struct Fine;"));
        assert!(gen.1[1].contains("It failed with"));
    }

    #[test]
    fn errors_outside_every_item_ask_for_the_whole_file_again() {
        let dir = crate_dir();
        let mut calls = 0;
        let mut check = move |_: &Graph| {
            calls += 1;
            Ok(if calls == 1 {
                Report {
                    ok: false,
                    unattributed: vec!["no matching package named `nope`".into()],
                    ..Report::default()
                }
            } else {
                Report {
                    ok: true,
                    ..Report::default()
                }
            })
        };
        let mut gen = Script(vec![DRAFT.into(), DRAFT.into()], Vec::new());
        build(&dir, "spec", "", &mut gen, &mut check, 2).unwrap();
        assert!(gen.1[1].contains("no matching package named `nope`"));
        assert!(gen.1[1].contains("complete corrected schema"));
    }

    const BASE: &str = "// === Cargo.toml ===\n[dependencies]\nserde = \"1\"\n\n// === src/lib.rs ===\npub struct A;\n\npub struct B;\n\npub fn keep() -> A { todo!() }\n";

    #[test]
    fn an_upgrade_restates_only_what_moves_and_reports_the_diff() {
        let dir = crate_dir();
        let mut gen = Script(
            vec!["pub struct B(u32);\npub fn extra() -> B { todo!() }\n".into()],
            Vec::new(),
        );
        let up = upgrade(&dir, "spec", "", BASE, "give B a number", &mut gen, &mut rejecting("BAD"), 2).unwrap();
        let text = render(&up.schema);
        assert!(text.contains("pub struct B(u32);"), "{text}");
        assert!(text.contains("pub struct A;"));
        assert!(text.contains("pub fn keep() -> A"));
        assert!(text.contains("serde = \"1\""));
        assert_eq!(up.diff.added, vec![NodeId("extra".into())]);
        assert_eq!(up.diff.changed, vec![NodeId("B".into())]);
        assert!(up.diff.removed.is_empty());
        assert_eq!(up.diff.to_fill().len(), 2);
        assert!(gen.1[0].contains("give B a number"));
        assert!(gen.1[0].contains("pub fn keep() -> A"));
    }

    #[test]
    fn an_upgrade_can_remove_items() {
        let dir = crate_dir();
        let mut gen = Script(vec!["// === remove ===\nkeep\nmissing\n".into()], Vec::new());
        let up = upgrade(&dir, "spec", "", BASE, "drop keep", &mut gen, &mut rejecting("BAD"), 2).unwrap();
        assert!(!render(&up.schema).contains("keep"));
        assert_eq!(up.diff.removed, vec![NodeId("keep".into())]);
        assert!(up.schema.notes.iter().any(|n| n.contains("`missing`")));
    }

    #[test]
    fn an_upgrade_that_breaks_an_item_is_repaired_like_a_build() {
        let dir = crate_dir();
        let mut gen = Script(
            vec!["pub struct B(BAD);\n".into(), "pub struct B(u8);\n".into()],
            Vec::new(),
        );
        let up = upgrade(&dir, "spec", "", BASE, "change", &mut gen, &mut rejecting("BAD"), 2).unwrap();
        assert!(render(&up.schema).contains("pub struct B(u8);"));
        assert!(gen.1[1].contains("## `B`"));
        assert_eq!(up.diff.changed, vec![NodeId("B".into())]);
    }

    #[test]
    fn a_reply_that_changes_nothing_is_asked_again_then_fails() {
        let dir = crate_dir();
        let mut gen = Script(vec!["".into(), "".into(), "".into()], Vec::new());
        let err = upgrade(&dir, "spec", "", BASE, "x", &mut gen, &mut rejecting("BAD"), 1)
            .err()
            .unwrap();
        assert!(err.contains("changed nothing"), "{err}");
        assert!(gen.1[1].contains("failed with"));
    }

    #[test]
    fn dependencies_in_an_upgrade_reply_are_added_to_the_old_ones() {
        let dir = crate_dir();
        let mut gen = Script(
            vec!["// === Cargo.toml ===\n[dependencies]\nrand = \"0.8\"\n// === src/lib.rs ===\npub struct C;\n".into()],
            Vec::new(),
        );
        let up = upgrade(&dir, "spec", "", BASE, "add C", &mut gen, &mut rejecting("BAD"), 1).unwrap();
        assert!(render(&up.schema).contains("rand = \"0.8\""));
        assert!(std::fs::read_to_string(dir.join("Cargo.toml")).unwrap().contains("rand"));
    }
}
