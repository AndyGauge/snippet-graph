//! Everything after the skeleton: tests first, parallel fill, blame and repair, lint,
//! a sweep back to last-good versions, and the handoff repository. `sg` and callers that
//! bring their own skeleton (the queue's pipeline) both run it from here.

use crate::assemble::assemble;
use crate::blame;
use crate::cli;
use crate::fill::{fill_all_versioned, Generator};
use crate::handoff;
use crate::ledger;
use crate::lintfix;
use crate::metrics::RunMetrics;
use crate::model::{Graph, NodeId, NodeKind, TestReport};
use crate::parallel::fill_parallel_versioned;
use crate::store::DirStore;
use crate::structure;
use crate::testgen;
use crate::verify;
use crate::version::{Origin, Results, RunId, Store, View};
use crate::view;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const FILL_RETRIES: usize = 3;
pub const TESTGEN_RETRIES: usize = 3;
pub const REPAIR_ROUNDS: usize = 4;
pub const LINT_PRUNE_ROUNDS: usize = 3;
pub const MAX_REPAIRS_PER_ROUND: usize = 4;
pub const NODE_ATTEMPT_BUDGET: usize = 10;
pub const MODEL: &str = "sg";

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn origin(run: &RunId) -> Origin {
    Origin {
        run: run.clone(),
        model: MODEL.into(),
        prompt_hash: String::new(),
        created_ms: now_ms(),
    }
}

pub fn record_view(
    store: &mut dyn Store,
    graph: &Graph,
    run: &RunId,
    parent: Option<&View>,
) -> Result<View, String> {
    let v = view::from_graph(store, graph, run, &origin(run), parent).map_err(io)?;
    store.put_view(v.clone()).map_err(io)?;
    Ok(v)
}

pub fn new_run_id() -> RunId {
    RunId(format!("run-{}", now_ms()))
}

pub fn check_graph(crate_dir: &Path, graph: &Graph) -> std::io::Result<verify::Report> {
    verify::check(crate_dir, &assemble(graph))
}

pub fn save_state(crate_dir: &Path, graph: &Graph) -> Result<(), String> {
    let text = serde_json::to_string(graph).map_err(|e| e.to_string())?;
    std::fs::write(crate_dir.join("sg-state.json"), text).map_err(|e| e.to_string())
}

pub fn all_tests(crate_dir: &Path, graph: &Graph) -> TestReport {
    verify::run_tests(crate_dir, &assemble(graph), &[]).unwrap_or_default()
}

pub fn failing(report: &TestReport) -> usize {
    report.outcomes.iter().filter(|o| !o.passed).count()
}

pub fn run_oracle(crate_dir: &Path, graph: &Graph, oracle: &Path) -> Result<bool, String> {
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
    let output = Command::new("cargo")
        .args(["test", "--quiet"])
        .current_dir(&work)
        .output()
        .map_err(|e| e.to_string())?;
    eprint!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    Ok(output.status.success())
}

pub fn review_section(notes: &[String]) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut out = String::from("## Skeleton review\n\n");
    for note in notes {
        out.push_str(&format!("- {note}\n"));
    }
    out.push('\n');
    out
}

fn io(e: impl ToString) -> String {
    e.to_string()
}

pub struct Job<'a> {
    pub crate_dir: &'a Path,
    pub store_dir: &'a Path,
    pub handoff_dir: &'a Path,
    pub spec: &'a str,
    pub oracle: Option<&'a Path>,
}

pub struct Finished {
    pub ok: bool,
    pub graph: Graph,
    pub summary: serde_json::Value,
}

/// Run a skeleton that already has its tests to the end: fill every function, repair what
/// the tests blame, and write the handoff. `generators` are the parallel workers, `generator`
/// the one used for repair rounds, and `usage` reports the tokens spent so far.
#[allow(clippy::too_many_arguments)]
pub fn run_graph<'a>(
    job: &Job,
    mut graph: Graph,
    untested: Vec<(NodeId, String)>,
    review_notes: Vec<String>,
    run_id: RunId,
    mut store: DirStore,
    mut generators: Vec<Box<dyn Generator + Send + 'a>>,
    generator: &mut dyn Generator,
    started: Instant,
    usage: &dyn Fn() -> (u64, u64),
) -> Result<Finished, String> {
    let crate_dir = job.crate_dir.to_path_buf();
    let store_dir = job.store_dir.to_path_buf();
    let test_count = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Test)
        .count();
    eprintln!("tests ok: {test_count} test nodes");

    let first = record_view(&mut store, &graph, &run_id, None)?;
    let lint = |g: &Graph| -> BTreeMap<NodeId, Vec<String>> {
        let mut assembled = assemble(g);
        for text in assembled.files.values_mut() {
            text.insert_str(0, "#![allow(dead_code)] ");
        }
        verify::clippy(&crate_dir, &assembled)
            .map(|r| r.by_node)
            .unwrap_or_default()
    };
    graph.budget = NODE_ATTEMPT_BUDGET;
    let (mut outcome, mut current) = fill_parallel_versioned(
        &mut graph,
        &mut generators,
        &lint,
        FILL_RETRIES,
        &mut store,
        &run_id,
        MODEL,
        Some(&first),
    )
    .map_err(io)?;
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
        let targets = blame::repair_targets(&graph, &suspects, MAX_REPAIRS_PER_ROUND);
        if targets.is_empty() {
            break;
        }
        eprintln!(
            "  retrying {}",
            targets
                .iter()
                .map(|t| t.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let saved: Vec<(NodeId, String)> = graph
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.kind, NodeKind::Function | NodeKind::Method))
            .filter_map(|(id, n)| n.body.clone().map(|b| (id.clone(), b)))
            .collect();
        for target in &targets {
            let mut findings = graph
                .nodes
                .get(target)
                .and_then(|n| n.body.as_deref())
                .map(|b| structure::evaluate_body(target, b))
                .unwrap_or_default();
            findings.extend(structure::test_load(&graph, target));
            match structure::refactor_reason(&findings) {
                Some(reason) => {
                    eprintln!("  {} judged too large: {reason}", target.0);
                    graph.refactor.insert(target.clone(), reason);
                }
                None => {
                    graph.refactor.remove(target);
                }
            }
            if let Some(node) = graph.nodes.get_mut(target) {
                node.body = None;
            }
        }
        let mut run_tests = |g: &Graph, only: &[NodeId]| {
            verify::run_tests(&crate_dir, &assemble(g), only).unwrap_or_default()
        };
        let (mut again, view) = fill_all_versioned(
            &mut graph,
            &mut *generator,
            &mut run_tests,
            FILL_RETRIES,
            &mut store,
            &run_id,
            MODEL,
            Some(&current),
        )
        .map_err(io)?;
        current = view;
        if !again.failed.is_empty() {
            for (id, why) in &again.failed {
                eprintln!("  revising tests for {}", id.0);
                let mut check_tests = |g: &Graph| all_tests(&crate_dir, g);
                let _ = testgen::revise_tests(
                    &mut graph,
                    job.spec,
                    id,
                    std::slice::from_ref(why),
                    &mut *generator,
                    &mut check_tests,
                    TESTGEN_RETRIES,
                );
            }
            let (second, view) = fill_all_versioned(
                &mut graph,
                &mut *generator,
                &mut run_tests,
                FILL_RETRIES,
                &mut store,
                &run_id,
                MODEL,
                Some(&current),
            )
            .map_err(io)?;
            current = view;
            again.filled.extend(second.filled);
            again.failed = second.failed;
            for (id, n) in second.retries {
                *again.retries.entry(id).or_default() += n;
            }
            for (id, n) in second.rollbacks {
                *again.rollbacks.entry(id).or_default() += n;
            }
        }
        for (id, body) in saved {
            if let Some(node) = graph.nodes.get_mut(&id) {
                if node.body.is_none() {
                    node.body = Some(body);
                }
            }
        }
        outcome.filled.extend(again.filled);
        outcome.failed.extend(again.failed);
        for (id, n) in again.retries {
            *outcome.retries.entry(id).or_default() += n;
        }
        for (id, n) in again.rollbacks {
            *outcome.rollbacks.entry(id).or_default() += n;
        }
    }

    for _ in 0..LINT_PRUNE_ROUNDS {
        let lint = verify::clippy(&crate_dir, &assemble(&graph)).map_err(|e| e.to_string())?;
        if lint.ok {
            break;
        }
        let removed = lintfix::prune(&mut graph, &lint.by_node);
        if removed.is_empty() {
            break;
        }
        eprintln!("  pruned {} unused nodes", removed.len());
        current = record_view(&mut store, &graph, &run_id, Some(&current))?;
    }

    let mut swept = 0;
    let swaps: Vec<_> = ledger::sweep_plan(&store, &current)
        .map_err(io)?
        .into_iter()
        .filter(|(node, _)| !graph.frozen.contains(node))
        .collect();
    if !swaps.is_empty() {
        let before = all_tests(&crate_dir, &graph);
        let mut candidate = graph.clone();
        for (node, version) in &swaps {
            let body = store.get_version(version).map_err(io)?.body;
            if let Some(n) = candidate.nodes.get_mut(node) {
                n.body = body;
            }
        }
        let after = all_tests(&crate_dir, &candidate);
        let better = after.compiled && (!before.compiled || failing(&after) <= failing(&before));
        eprintln!(
            "sweep: {} nodes back to their last good version: {} failing tests before, {} after{}",
            swaps.len(),
            failing(&before),
            failing(&after),
            if better {
                ""
            } else {
                " (kept the current code)"
            }
        );
        if better {
            graph = candidate;
            current = record_view(&mut store, &graph, &run_id, Some(&current))?;
            swept = swaps.len();
        }
    }

    let final_report = all_tests(&crate_dir, &graph);
    let lint_report = verify::clippy(&crate_dir, &assemble(&graph)).map_err(|e| e.to_string())?;
    store
        .put_results(Results {
            view: current.id.clone(),
            tests: cli::outcomes_of(&final_report),
            benches: Vec::new(),
            clippy_clean: Some(lint_report.ok),
            recorded_ms: now_ms(),
        })
        .map_err(io)?;
    let oracle_ok = match &job.oracle {
        Some(path) => Some(run_oracle(&crate_dir, &graph, path)?),
        None => None,
    };

    let ledger_all = ledger::ledger(&store).map_err(io)?;
    let failures: BTreeMap<NodeId, String> = outcome.failed.iter().cloned().collect();
    let reports = handoff::node_reports(&graph, &final_report, &failures, &store);
    let handoff_dir = job.handoff_dir.to_path_buf();
    handoff::write_repo(
        &store,
        &current.id,
        Some(&crate_dir.join("Cargo.toml")),
        &handoff_dir,
        &format!(
            "{}{}{}",
            handoff::render(&graph, &reports),
            ledger::summary_markdown(&ledger_all),
            review_section(&review_notes)
        ),
    )?;

    let mut metrics = RunMetrics::default();
    for (id, n) in &outcome.retries {
        for _ in 0..*n {
            metrics.record_retry(id);
        }
    }
    let (input, output) = usage();
    metrics.input_tokens = input;
    metrics.output_tokens = output;
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
        "view": current.id.0,
        "store": store_dir.display().to_string(),
        "handoff": handoff_dir.display().to_string(),
        "structural": reports.iter().filter(|r| !r.structure.is_empty()).map(|r| format!("{}: {}", r.node.0, r.structure.join("; "))).collect::<Vec<_>>(),
        "start_here": handoff::start_here(&graph, &reports).iter().map(|n| n.0.clone()).collect::<Vec<_>>(),
        "nodes": graph.nodes.len(),
        "tests": test_count,
        "failed_nodes": outcome.failed.iter().map(|(id, why)| format!("{}: {why}", id.0)).collect::<Vec<_>>(),
        "untested_nodes": untested.iter().map(|(id, _)| id.0.clone()).collect::<Vec<_>>(),
        "failing_tests": failing(&final_report),
        "skeleton_review": review_notes,
        "swept_to_last_good": swept,
        "failures_seen": ledger_all.len(),
        "failures_open": ledger_all.iter().filter(|f| f.is_open()).count(),
        "rollbacks": outcome.rollbacks.iter().map(|(id, n)| (id.0.clone(), *n)).collect::<BTreeMap<_, _>>(),
        "clippy_clean": lint_report.ok,
        "oracle_passed": oracle_ok,
        "metrics": serde_json::from_str::<serde_json::Value>(&metrics.to_json()).unwrap_or_default(),
    });
    Ok(Finished { ok, graph, summary })
}
