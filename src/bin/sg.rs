use inference_providers::config::Config;
use inference_providers::{InferenceRequest, ModelTier, Registry};
use snippet_graph::assemble::assemble;
use snippet_graph::blame;
use snippet_graph::fill::{fill_all_tested, Generator};
use snippet_graph::metrics::RunMetrics;
use snippet_graph::parallel::fill_parallel;
use snippet_graph::skeleton;
use snippet_graph::testgen;
use snippet_graph::verify;
use snippet_graph::{Graph, NodeId, NodeKind, TestReport};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const SKELETON_ATTEMPTS: usize = 4;
const FILL_RETRIES: usize = 3;
const TESTGEN_RETRIES: usize = 3;
const REPAIR_ROUNDS: usize = 4;
const DEFAULT_WIDTH: usize = 4;
const MAX_TOKENS: u32 = 16000;

#[derive(Default)]
struct Usage {
    input: AtomicU64,
    output: AtomicU64,
    calls: AtomicU64,
}

struct RegistryGenerator {
    registry: Arc<Registry>,
    slot: Option<usize>,
    usage: Arc<Usage>,
}

const SYSTEM: &str = "You are a precise Rust engineer. Output only what is asked, no commentary.";

impl Generator for RegistryGenerator {
    fn generate(&mut self, prompt: &str) -> Result<String, String> {
        let mut req = InferenceRequest::new(
            SYSTEM.into(),
            prompt.into(),
            MAX_TOKENS,
            ModelTier::Heavy,
            1,
        );
        req.slot_hint = self.slot;
        let started = Instant::now();
        let response = self.registry.complete(&req).map_err(|e| e.to_string())?;
        self.usage
            .input
            .fetch_add(u64::from(response.input_tokens), Ordering::Relaxed);
        self.usage
            .output
            .fetch_add(u64::from(response.output_tokens), Ordering::Relaxed);
        let n = self.usage.calls.fetch_add(1, Ordering::Relaxed) + 1;
        eprintln!(
            "  model call {n}: {}s, {}+{} tokens",
            started.elapsed().as_secs(),
            response.input_tokens,
            response.output_tokens
        );
        Ok(response.text)
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
        "Write the Rust skeleton for this spec as a single lib.rs: all types, consts, use items, and function/method signatures, with every function body exactly `{{ todo!() }}`. No logic. Private helpers that need no state must be free functions, not methods taking `&self`; every private item must be called by something, since unused items fail the lint. Output only Rust source.\n\nSpec:\n{spec}\n"
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

struct Args {
    spec: PathBuf,
    crate_dir: PathBuf,
    oracle: Option<PathBuf>,
    width: usize,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args().skip(1);
    let mut positional = Vec::new();
    let mut oracle = None;
    let mut width = DEFAULT_WIDTH;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--oracle" => oracle = Some(PathBuf::from(it.next()?)),
            "--width" => width = it.next()?.parse().ok()?,
            _ => positional.push(a),
        }
    }
    let [spec, dir] = <[String; 2]>::try_from(positional).ok()?;
    Some(Args {
        spec: spec.into(),
        crate_dir: dir.into(),
        oracle,
        width,
    })
}

fn all_tests(crate_dir: &Path, graph: &Graph) -> TestReport {
    verify::run_tests(crate_dir, &assemble(graph), &[]).unwrap_or_default()
}

fn failing(report: &TestReport) -> usize {
    report.outcomes.iter().filter(|o| !o.passed).count()
}

fn run_oracle(crate_dir: &Path, graph: &Graph, oracle: &Path) -> Result<bool, String> {
    let work = std::env::temp_dir().join(format!("sg-oracle-{}", std::process::id()));
    std::fs::create_dir_all(work.join("src")).map_err(|e| e.to_string())?;
    std::fs::copy(crate_dir.join("Cargo.toml"), work.join("Cargo.toml"))
        .map_err(|e| e.to_string())?;
    let lib = assemble(graph)
        .files
        .into_values()
        .next()
        .unwrap_or_default();
    let oracle_src = std::fs::read_to_string(oracle).map_err(|e| e.to_string())?;
    std::fs::write(work.join("src/lib.rs"), format!("{lib}\n{oracle_src}"))
        .map_err(|e| e.to_string())?;
    let status = Command::new("cargo")
        .args(["test", "--quiet"])
        .current_dir(&work)
        .status()
        .map_err(|e| e.to_string())?;
    Ok(status.success())
}

fn run(args: Args) -> Result<bool, String> {
    let started = Instant::now();
    let spec = std::fs::read_to_string(&args.spec).map_err(|e| e.to_string())?;
    let registry = Arc::new(Registry::from_config(&Config::load()));
    let usage = Arc::new(Usage::default());
    let mut gen = RegistryGenerator {
        registry: registry.clone(),
        slot: None,
        usage: usage.clone(),
    };
    let crate_dir = args.crate_dir;

    let mut graph = make_skeleton(&mut gen, &crate_dir, &spec)?;
    eprintln!("skeleton ok: {} nodes", graph.nodes.len());

    let mut check_tests = |g: &Graph| all_tests(&crate_dir, g);
    let untested = testgen::generate_all(
        &mut graph,
        &spec,
        &mut gen,
        &mut check_tests,
        TESTGEN_RETRIES,
    );
    for (id, why) in &untested {
        eprintln!("  no tests for {}: {why}", id.0);
    }
    let test_count = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Test)
        .count();
    eprintln!("tests ok: {test_count} test nodes");

    let mut generators: Vec<Box<dyn Generator + Send>> = (0..args.width.max(1))
        .map(|i| {
            Box::new(RegistryGenerator {
                registry: registry.clone(),
                slot: Some(i),
                usage: usage.clone(),
            }) as Box<dyn Generator + Send>
        })
        .collect();
    let lint = |g: &Graph| -> BTreeMap<NodeId, Vec<String>> {
        verify::clippy(&crate_dir, &assemble(g))
            .map(|r| r.by_node)
            .unwrap_or_default()
    };
    let mut outcome = fill_parallel(&mut graph, &mut generators, &lint, FILL_RETRIES);
    eprintln!(
        "filled {}, failed {}",
        outcome.filled.len(),
        outcome.failed.len()
    );

    for round in 1..=REPAIR_ROUNDS {
        let report = all_tests(&crate_dir, &graph);
        if report.compiled && failing(&report) == 0 {
            break;
        }
        let suspects = blame::rank(&graph, &report.outcomes, &[]);
        eprintln!(
            "repair round {round}: {} failing tests, {} suspects",
            failing(&report),
            suspects.len()
        );
        let Some(top) = suspects.first() else { break };
        if let Some(node) = graph.nodes.get_mut(&top.node) {
            node.body = None;
        }
        let mut run_tests = |g: &Graph, only: &[NodeId]| {
            verify::run_tests(&crate_dir, &assemble(g), only).unwrap_or_default()
        };
        let mut again = fill_all_tested(&mut graph, &mut gen, &mut run_tests, FILL_RETRIES);
        if !again.failed.is_empty() {
            for (id, why) in &again.failed {
                eprintln!("  revising tests for {}", id.0);
                let mut check_tests = |g: &Graph| all_tests(&crate_dir, g);
                let _ = testgen::revise_tests(
                    &mut graph,
                    &spec,
                    id,
                    std::slice::from_ref(why),
                    &mut gen,
                    &mut check_tests,
                    TESTGEN_RETRIES,
                );
            }
            let second = fill_all_tested(&mut graph, &mut gen, &mut run_tests, FILL_RETRIES);
            again.filled.extend(second.filled);
            again.failed = second.failed;
            for (id, n) in second.retries {
                *again.retries.entry(id).or_default() += n;
            }
        }
        outcome.filled.extend(again.filled);
        outcome.failed.extend(again.failed);
        for (id, n) in again.retries {
            *outcome.retries.entry(id).or_default() += n;
        }
    }

    let final_report = all_tests(&crate_dir, &graph);
    let lint_report = verify::clippy(&crate_dir, &assemble(&graph)).map_err(|e| e.to_string())?;
    let oracle_ok = match &args.oracle {
        Some(path) => Some(run_oracle(&crate_dir, &graph, path)?),
        None => None,
    };

    let mut metrics = RunMetrics::default();
    for (id, n) in &outcome.retries {
        for _ in 0..*n {
            metrics.record_retry(id);
        }
    }
    metrics.input_tokens = usage.input.load(Ordering::Relaxed);
    metrics.output_tokens = usage.output.load(Ordering::Relaxed);
    metrics.wall_ms = started.elapsed().as_millis() as u64;

    outcome
        .failed
        .retain(|(id, _)| graph.nodes.get(id).is_some_and(|n| !n.is_filled()));
    let ok = outcome.failed.is_empty()
        && untested.is_empty()
        && final_report.compiled
        && failing(&final_report) == 0
        && lint_report.ok
        && oracle_ok.unwrap_or(true);
    let summary = serde_json::json!({
        "ok": ok,
        "nodes": graph.nodes.len(),
        "tests": test_count,
        "failed_nodes": outcome.failed.iter().map(|(id, why)| format!("{}: {why}", id.0)).collect::<Vec<_>>(),
        "untested_nodes": untested.iter().map(|(id, _)| id.0.clone()).collect::<Vec<_>>(),
        "failing_tests": failing(&final_report),
        "clippy_clean": lint_report.ok,
        "oracle_passed": oracle_ok,
        "metrics": serde_json::from_str::<serde_json::Value>(&metrics.to_json()).unwrap_or_default(),
    });
    println!("{summary:#}");
    Ok(ok)
}

fn main() {
    let Some(args) = parse_args() else {
        eprintln!("usage: sg <spec.md> <crate_dir> [--oracle file.rs] [--width N]");
        std::process::exit(2);
    };
    match run(args) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
