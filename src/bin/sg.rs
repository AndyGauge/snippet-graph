use inference_providers::config::Config;
use inference_providers::{InferenceRequest, ModelTier, Registry};
use snippet_graph::assemble::assemble;
use snippet_graph::fill::{fill_all, Generator};
use snippet_graph::skeleton;
use snippet_graph::verify;
use snippet_graph::{Graph, NodeId};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SKELETON_ATTEMPTS: usize = 4;
const FILL_RETRIES: usize = 3;
const MAX_TOKENS: u32 = 16000;

struct RegistryGenerator {
    registry: Registry,
    system: String,
}

impl Generator for RegistryGenerator {
    fn generate(&mut self, prompt: &str) -> Result<String, String> {
        let req = InferenceRequest::new(
            self.system.clone(),
            prompt.into(),
            MAX_TOKENS,
            ModelTier::Heavy,
            1,
        );
        self.registry
            .complete(&req)
            .map(|r| r.text)
            .map_err(|e| e.to_string())
    }
}

fn strip_fences(text: &str) -> String {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t.to_string();
    };
    let rest = rest.split_once('\n').map_or("", |(_, body)| body);
    rest.rsplit_once("```")
        .map_or(rest, |(body, _)| body)
        .trim()
        .to_string()
}

fn skeleton_prompt(spec: &str, errors: &[String], previous: &str) -> String {
    let mut p = format!(
        "Write the Rust skeleton for this spec as a single lib.rs: all types, consts, use items, and function/method signatures, with every function body exactly `{{ todo!() }}`. No logic. Output only Rust source.\n\nSpec:\n{spec}\n"
    );
    if !errors.is_empty() {
        p.push_str(&format!(
            "\nYour previous skeleton:\n{previous}\n\nIt failed to compile:\n{}\nFix it.\n",
            errors.join("\n")
        ));
    }
    p
}

fn check_graph(crate_dir: &Path, graph: &Graph) -> std::io::Result<verify::Report> {
    verify::check(crate_dir, &assemble(graph))
}

fn make_skeleton(
    gen: &mut RegistryGenerator,
    crate_dir: &Path,
    spec: &str,
) -> Result<Graph, String> {
    let mut errors = Vec::new();
    let mut previous = String::new();
    for attempt in 1..=SKELETON_ATTEMPTS {
        eprintln!("skeleton attempt {attempt}");
        previous = strip_fences(&gen.generate(&skeleton_prompt(spec, &errors, &previous))?);
        let graph = match skeleton::parse(Path::new("src/lib.rs"), &previous) {
            Ok(g) => g,
            Err(e) => {
                errors = vec![e.to_string()];
                continue;
            }
        };
        let report = check_graph(crate_dir, &graph).map_err(|e| e.to_string())?;
        if report.ok {
            return Ok(graph);
        }
        errors = report
            .by_node
            .into_values()
            .flatten()
            .chain(report.unattributed)
            .collect();
    }
    Err(format!(
        "no compiling skeleton after {SKELETON_ATTEMPTS} attempts"
    ))
}

fn run(spec_path: &Path, crate_dir: PathBuf) -> Result<(), String> {
    let spec = std::fs::read_to_string(spec_path).map_err(|e| e.to_string())?;
    let mut gen = RegistryGenerator {
        registry: Registry::from_config(&Config::load()),
        system: "You are a precise Rust engineer. Output only what is asked, no commentary.".into(),
    };
    let mut graph = make_skeleton(&mut gen, &crate_dir, &spec)?;
    eprintln!("skeleton ok: {} nodes", graph.nodes.len());

    let mut check = |g: &Graph| -> BTreeMap<NodeId, Vec<String>> {
        check_graph(&crate_dir, g)
            .map(|r| r.by_node)
            .unwrap_or_default()
    };
    let outcome = fill_all(&mut graph, &mut gen, &mut check, FILL_RETRIES);
    eprintln!(
        "filled {}, failed {}",
        outcome.filled.len(),
        outcome.failed.len()
    );
    for (id, why) in &outcome.failed {
        eprintln!("  failed {}: {why}", id.0);
    }
    check_graph(&crate_dir, &graph).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [spec, dir] = args.as_slice() else {
        eprintln!("usage: sg <spec.md> <crate_dir>");
        std::process::exit(2);
    };
    if let Err(e) = run(Path::new(spec), PathBuf::from(dir)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
