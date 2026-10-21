use snippet_graph::assemble::assemble;
use snippet_graph::bench::{self, BenchConfig};
use snippet_graph::benchgen;
use snippet_graph::cli;
use snippet_graph::fill::{fill_all_versioned, Generator};
use snippet_graph::import::write_back;
use snippet_graph::plugin::{self, Plugin, PluginGenerator};
use snippet_graph::repo_graph;
use snippet_graph::review;
use snippet_graph::schema;
use snippet_graph::session::{
    self, all_tests, check_graph, new_run_id, now_ms, record_view, save_state, FILL_RETRIES, MODEL,
    TESTGEN_RETRIES,
};
use snippet_graph::skeleton;
use snippet_graph::store::DirStore;
use snippet_graph::testgen;
use snippet_graph::variants::{self, Evaluation};
use snippet_graph::verify;
use snippet_graph::version::{Results, Store};
use snippet_graph::view;
use snippet_graph::{Graph, NodeId, NodeKind};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

const SKELETON_ATTEMPTS: usize = 4;
const BENCH_RETRIES: usize = 3;
const USAGE: &str = "usage:
  sg [run] <spec.md> <crate_dir> [--oracle file.rs] [--width N] [--resume sg-state.json] [--prelude file.rs] [--store DIR] [--handoff DIR] [--no-review] [--provider NAME] [--provider-args ARGS]
  sg schema <spec.md> <crate_dir> [--from SCHEMA --change FILE] [--guide FILE] [--out FILE] [--rounds N] [--provider NAME] [--provider-args ARGS]
  sg history <store> [--node ID]
  sg diff <store> <a> <b>
  sg graph [--out FILE] [--root DIR] <crate_dir>...
  sg failures <store> [--node ID] [--open]
  sg good <store> <node>
  sg sweep <store> <view> [--dry-run]
  sg undo <store> <view>
  sg swap <store> <view> <node> <version>
  sg export <store> <view> <dir> [--commit]
  sg ab <store> <view> <node> --spec FILE --crate DIR [--n 4] [--runs 7] [--noise 5] [--width N]
  sg edit <store> <view> <crate_dir> --spec FILE <node ids...>";

fn strip_fences(text: &str) -> String {
    review::unfence(text)
}

fn skeleton_prompt(spec: &str, prelude: &str, errors: &[String], previous: &str) -> String {
    let mut p = format!(
        "Write the Rust skeleton for this spec as a single lib.rs: all types, consts, use items, and function/method signatures, with every function body exactly `{{ todo!() }}`. No logic. Private helpers that need no state must be free functions, not methods taking `&self`; never add a function whose only purpose is to call other functions. Keep each function to one job: if its contract would need more than three separate duties (filtering, ordering, several aggregates, exclusions), split the steps into small private helper functions that it calls. Put a `///` doc comment on every new type and function stating its contract for callers who cannot see the body: what it returns, which inputs it skips or treats specially, and any rounding, ordering or error behaviour from the spec. Output only Rust source.\n\nSpec:\n{spec}\n"
    );
    if !prelude.is_empty() {
        p.push_str(&format!(
            "\nThe file already begins with these items, which are finished. Do not repeat or change them; write only the new items that follow them:\n```rust\n{prelude}\n```\n"
        ));
    }
    if !errors.is_empty() {
        p.push_str(&format!(
            "\nYour previous skeleton:\n{previous}\n\nIt failed to compile:\n{}\nFix it.\n",
            errors.join("\n")
        ));
    }
    p
}

const PRELUDE_GAP: usize = 10_000;

fn with_prelude(prelude: &Graph, mut model: Graph) -> Graph {
    model.nodes.retain(|id, _| !prelude.nodes.contains_key(id));
    for node in model.nodes.values_mut() {
        node.placement.index += PRELUDE_GAP;
    }
    let mut combined = prelude.clone();
    skeleton::merge(&mut combined, model);
    combined.frozen = prelude.nodes.keys().cloned().collect();
    combined
}

fn build_skeleton(
    crate_dir: &Path,
    text: &str,
    prelude: Option<&Graph>,
) -> Result<Graph, Vec<String>> {
    let mut graph =
        skeleton::parse(Path::new("src/lib.rs"), text).map_err(|e| vec![e.to_string()])?;
    if let Some(prelude) = prelude {
        graph = with_prelude(prelude, graph);
    }
    let report = check_graph(crate_dir, &graph).map_err(|e| vec![e.to_string()])?;
    if report.ok {
        Ok(graph)
    } else {
        Err(report
            .by_node
            .into_values()
            .flatten()
            .chain(report.unattributed)
            .collect())
    }
}

fn make_skeleton(
    gen: &mut PluginGenerator,
    crate_dir: &Path,
    spec: &str,
    prelude: Option<&str>,
    review_rounds: usize,
) -> Result<(Graph, Vec<String>), String> {
    let prelude_text = prelude.unwrap_or_default();
    let prelude_graph = match prelude {
        Some(text) => {
            Some(skeleton::parse(Path::new("src/lib.rs"), text).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let mut errors = Vec::new();
    let mut previous = String::new();
    for attempt in 1..=SKELETON_ATTEMPTS {
        eprintln!("skeleton attempt {attempt}");
        previous = strip_fences(&gen.generate(&skeleton_prompt(
            spec,
            prelude_text,
            &errors,
            &previous,
        ))?);
        match build_skeleton(crate_dir, &previous, prelude_graph.as_ref()) {
            Ok(graph) => {
                let reviewed = review::review_skeleton(
                    graph,
                    previous,
                    spec,
                    gen,
                    &mut |text| build_skeleton(crate_dir, text, prelude_graph.as_ref()),
                    review_rounds,
                );
                for note in &reviewed.notes {
                    eprintln!("skeleton review: {note}");
                }
                return Ok((reviewed.graph, reviewed.notes));
            }
            Err(e) => errors = e,
        }
    }
    Err(format!(
        "no compiling skeleton after {SKELETON_ATTEMPTS} attempts"
    ))
}

struct Args {
    spec: PathBuf,
    crate_dir: PathBuf,
    oracle: Option<PathBuf>,
    resume: Option<PathBuf>,
    prelude: Option<PathBuf>,
    handoff: Option<PathBuf>,
    store: Option<PathBuf>,
    width: Option<usize>,
    provider: Option<String>,
    provider_args: Option<String>,
    review: bool,
}

struct Flags {
    positional: Vec<String>,
    values: BTreeMap<String, String>,
    switches: Vec<String>,
}

impl Flags {
    fn parse(args: Vec<String>, valued: &[&str], switches: &[&str]) -> Result<Flags, String> {
        let mut flags = Flags {
            positional: Vec::new(),
            values: BTreeMap::new(),
            switches: Vec::new(),
        };
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            if valued.contains(&a.as_str()) {
                let v = it.next().ok_or_else(|| format!("{a} needs a value"))?;
                flags.values.insert(a, v);
            } else if switches.contains(&a.as_str()) {
                flags.switches.push(a);
            } else if a.starts_with("--") {
                return Err(format!("unknown flag {a}"));
            } else {
                flags.positional.push(a);
            }
        }
        Ok(flags)
    }

    fn has(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        self.values.get(name).map(PathBuf::from)
    }

    fn number<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.values.get(name) {
            Some(v) => v.parse().map_err(|_| format!("{name}: bad number {v}")),
            None => Ok(default),
        }
    }

    fn required(&self, name: &str) -> Result<PathBuf, String> {
        self.path(name).ok_or_else(|| format!("{name} is required"))
    }
}

fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let flags = Flags::parse(
        args,
        &[
            "--oracle",
            "--resume",
            "--width",
            "--store",
            "--prelude",
            "--handoff",
            "--provider",
            "--provider-args",
        ],
        &["--no-review"],
    )?;
    let width = flags
        .values
        .get("--width")
        .map(|_| flags.number("--width", 0usize))
        .transpose()?;
    let [spec, dir] = <[String; 2]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <spec.md> <crate_dir>".to_string())?;
    Ok(Args {
        spec: spec.into(),
        crate_dir: dir.into(),
        oracle: flags.path("--oracle"),
        resume: flags.path("--resume"),
        prelude: flags.path("--prelude"),
        handoff: flags.path("--handoff"),
        store: flags.path("--store"),
        width,
        provider: flags.values.get("--provider").cloned(),
        provider_args: flags.values.get("--provider-args").cloned(),
        review: !flags.has("--no-review"),
    })
}

fn io(e: impl ToString) -> String {
    e.to_string()
}

fn open_store(dir: &str) -> Result<DirStore, String> {
    DirStore::open(Path::new(dir)).map_err(io)
}

fn generator_pool(plugin: &Arc<Plugin>, width: usize) -> Vec<Box<dyn Generator + Send>> {
    (0..width.max(1))
        .map(|i| Box::new(plugin.generator(Some(i))) as Box<dyn Generator + Send>)
        .collect()
}

/// Start the provider program: `--provider NAME` (or `SG_PROVIDER`), by default `openai`.
/// `--provider-args` (or `SG_PROVIDER_ARGS`) is passed on as
/// words.
fn open_plugin(name: Option<&str>, args: Option<&str>) -> Result<Arc<Plugin>, String> {
    let name = name
        .map(str::to_string)
        .or_else(|| std::env::var("SG_PROVIDER").ok())
        .unwrap_or_else(|| "openai".to_string());
    let args: Vec<String> = args
        .map(str::to_string)
        .or_else(|| std::env::var("SG_PROVIDER_ARGS").ok())
        .map(|a| a.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let program = plugin::find_provider(&name)?;
    let p = Plugin::spawn(&program, &args)?;
    eprintln!(
        "provider {} ({}), {} channel(s)",
        p.info.name,
        program.display(),
        p.info.channels
    );
    Ok(p)
}

fn load_state(path: &Path) -> Result<Graph, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut graph: Graph = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    for node in graph.nodes.values_mut() {
        if matches!(node.kind, NodeKind::Function | NodeKind::Method) {
            node.body = None;
        }
    }
    Ok(graph)
}

fn run(args: Args) -> Result<bool, String> {
    let started = Instant::now();
    let spec = std::fs::read_to_string(&args.spec).map_err(|e| e.to_string())?;
    let plugin = open_plugin(args.provider.as_deref(), args.provider_args.as_deref())?;
    let mut gen = plugin.generator(None);
    let width = args.width.unwrap_or(plugin.info.channels.clamp(1, 16));
    let crate_dir = args.crate_dir;
    let store_dir = args.store.unwrap_or_else(|| crate_dir.join(".sg-store"));
    let store = DirStore::open(&store_dir).map_err(io)?;
    let run_id = new_run_id();

    let prelude_text = match &args.prelude {
        Some(path) => Some(std::fs::read_to_string(path).map_err(|e| e.to_string())?),
        None => None,
    };
    let (graph, untested, review_notes) = match &args.resume {
        Some(path) => {
            let mut graph = load_state(path)?;
            graph.spec = spec.clone();
            (graph, Vec::new(), Vec::new())
        }
        None => {
            let rounds = if args.review {
                review::MAX_REVIEW_ROUNDS
            } else {
                0
            };
            let (mut graph, review_notes) =
                make_skeleton(&mut gen, &crate_dir, &spec, prelude_text.as_deref(), rounds)?;
            graph.spec = spec.clone();
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
            save_state(&crate_dir, &graph)?;
            (graph, untested, review_notes)
        }
    };
    let test_count = graph
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Test)
        .count();
    eprintln!("tests ok: {test_count} test nodes");
    let handoff_dir = args.handoff.clone().unwrap_or_else(|| {
        let mut name = crate_dir.as_os_str().to_owned();
        name.push("-handoff");
        PathBuf::from(name)
    });
    let job = session::Job {
        crate_dir: &crate_dir,
        store_dir: &store_dir,
        handoff_dir: &handoff_dir,
        spec: &spec,
        oracle: args.oracle.as_deref(),
    };
    let usage = || {
        (
            plugin.usage.input.load(Ordering::Relaxed),
            plugin.usage.output.load(Ordering::Relaxed),
        )
    };
    let finished = session::run_graph(
        &job,
        graph,
        untested,
        review_notes,
        run_id,
        store,
        generator_pool(&plugin, width),
        &mut gen,
        started,
        &usage,
    )?;
    println!("{:#}", finished.summary);
    Ok(finished.ok)
}

fn schema_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(
        args,
        &[
            "--guide",
            "--from",
            "--change",
            "--out",
            "--rounds",
            "--provider",
            "--provider-args",
        ],
        &[],
    )?;
    let [spec, dir] = <[String; 2]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <spec.md> <crate_dir>".to_string())?;
    let spec = std::fs::read_to_string(&spec).map_err(|e| format!("{spec}: {e}"))?;
    let guide = match flags.path("--guide") {
        Some(path) => {
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => String::new(),
    };
    let rounds = flags.number("--rounds", schema::REPAIR_ROUNDS)?;
    let plugin = open_plugin(
        flags.values.get("--provider").map(String::as_str),
        flags.values.get("--provider-args").map(String::as_str),
    )?;
    let mut gen = plugin.generator(None);
    let crate_dir = PathBuf::from(dir);
    let built = match (flags.path("--from"), flags.path("--change")) {
        (None, None) => schema::build(
            &crate_dir,
            &spec,
            &guide,
            &mut gen,
            &mut |g: &Graph| check_graph(&crate_dir, g),
            rounds,
        )?,
        (Some(from), Some(change)) => {
            let current =
                std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
            let change = std::fs::read_to_string(&change)
                .map_err(|e| format!("{}: {e}", change.display()))?;
            let up = schema::upgrade(
                &crate_dir,
                &spec,
                &guide,
                &current,
                &change,
                &mut gen,
                &mut |g: &Graph| check_graph(&crate_dir, g),
                rounds,
            )?;
            eprint!("{}", up.diff.render());
            if up.diff.is_empty() {
                eprintln!("schema: the change left every item as it was");
            }
            up.schema
        }
        _ => return Err("--from and --change go together".to_string()),
    };
    for note in &built.notes {
        eprintln!("schema: {note}");
    }
    eprintln!("schema ok: {} items", built.graph.nodes.len());
    let text = schema::render(&built);
    match flags.path("--out") {
        Some(path) => {
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?
        }
        None => println!("{text}"),
    }
    Ok(())
}

fn history_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &["--node"], &[])?;
    let [store] = <[String; 1]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <store>".to_string())?;
    let store = open_store(&store)?;
    let node = flags.values.get("--node").map(|n| NodeId(n.clone()));
    print!("{}", cli::history(&store, node.as_ref())?);
    Ok(())
}

fn graph_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &["--out", "--root"], &[])?;
    if flags.positional.is_empty() {
        return Err("expected at least one <crate_dir>".to_string());
    }
    let dirs: Vec<PathBuf> = flags.positional.iter().map(PathBuf::from).collect();
    let root = flags
        .values
        .get("--root")
        .map_or(PathBuf::from("."), PathBuf::from);
    let graph = repo_graph::build(&dirs, &root)?;
    let text = serde_json::to_string_pretty(&graph).map_err(|e| e.to_string())?;
    match flags.values.get("--out") {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?,
        None => println!("{text}"),
    }
    eprintln!(
        "{} nodes, {} edges in {} crate(s)",
        graph.nodes.len(),
        graph.edges.len(),
        graph.crates.len()
    );
    Ok(())
}

fn failures_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &["--node"], &["--open"])?;
    let [store] = <[String; 1]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <store>".to_string())?;
    let node = flags.values.get("--node").map(|n| NodeId(n.clone()));
    print!(
        "{}",
        cli::failures(&open_store(&store)?, node.as_ref(), flags.has("--open"))?
    );
    Ok(())
}

fn good_cmd(args: Vec<String>) -> Result<(), String> {
    let [store, node] =
        <[String; 2]>::try_from(args).map_err(|_| "expected <store> <node>".to_string())?;
    print!("{}", cli::good(&open_store(&store)?, &NodeId(node))?);
    Ok(())
}

fn sweep_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &[], &["--dry-run"])?;
    let [store, view] = <[String; 2]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <store> <view>".to_string())?;
    let mut store = open_store(&store)?;
    let (text, _) = cli::sweep(&mut store, &view, !flags.has("--dry-run"), now_ms())?;
    print!("{text}");
    Ok(())
}

fn diff_cmd(args: Vec<String>) -> Result<(), String> {
    let [store, a, b] =
        <[String; 3]>::try_from(args).map_err(|_| "expected <store> <a> <b>".to_string())?;
    print!("{}", cli::diff_views(&open_store(&store)?, &a, &b)?);
    Ok(())
}

fn undo_cmd(args: Vec<String>) -> Result<(), String> {
    let [store, view] =
        <[String; 2]>::try_from(args).map_err(|_| "expected <store> <view>".to_string())?;
    println!("{}", cli::undo(&open_store(&store)?, &view)?.0);
    Ok(())
}

fn swap_cmd(args: Vec<String>) -> Result<(), String> {
    let [store, view, node, version] = <[String; 4]>::try_from(args)
        .map_err(|_| "expected <store> <view> <node> <version>".to_string())?;
    let mut store = open_store(&store)?;
    let id = cli::swap_cmd(&mut store, &view, &NodeId(node), &version, now_ms())?;
    println!("{}", id.0);
    Ok(())
}

fn export_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &[], &["--commit"])?;
    let [store, view, dir] = <[String; 3]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <store> <view> <dir>".to_string())?;
    let commit = flags.has("--commit");
    let id = cli::export_cmd(&open_store(&store)?, &view, Path::new(&dir), commit)?;
    println!("exported view {}", id.0);
    Ok(())
}

fn ab_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(
        args,
        &[
            "--spec",
            "--crate",
            "--n",
            "--runs",
            "--noise",
            "--width",
            "--provider",
            "--provider-args",
        ],
        &[],
    )?;
    let [store_dir, view_arg, node_arg] = <[String; 3]>::try_from(flags.positional.clone())
        .map_err(|_| "expected <store> <view> <node>".to_string())?;
    let spec = std::fs::read_to_string(flags.required("--spec")?).map_err(io)?;
    let crate_dir = flags.required("--crate")?;
    let n: usize = flags.number("--n", 4)?;
    let cfg = BenchConfig {
        runs: flags.number("--runs", 7)?,
        noise_pct: flags.number("--noise", 5.0)?,
    };
    let width = flags.number("--width", n)?;

    let mut store = open_store(&store_dir)?;
    let base_id = cli::resolve_view(&store, &view_arg)?;
    let mut base = store.get_view(&base_id).map_err(io)?;
    let node = cli::parse_node(&base, &node_arg)?;
    let plugin = open_plugin(
        flags.values.get("--provider").map(String::as_str),
        flags.values.get("--provider-args").map(String::as_str),
    )?;
    let run_id = new_run_id();

    let mut graph = view::to_graph(&store, &base).map_err(io)?;
    graph.spec = spec.clone();
    if cli::benches_reaching(&graph, &node).is_empty() {
        eprintln!("no bench reaches {}; generating one", node.0);
        let mut gen = plugin.generator(None);
        let mut check = |g: &Graph| -> Result<(), String> {
            let needed = cli::benches_reaching(g, &node);
            if needed.is_empty() {
                return Err("no bench reaches the node".into());
            }
            let results = bench::run_benches(&crate_dir, &assemble(g), &[], &cfg).map_err(io)?;
            let missing: Vec<String> = needed
                .iter()
                .filter(|id| !results.iter().any(|r| &r.bench == *id))
                .map(|id| id.0.clone())
                .collect();
            if missing.is_empty() {
                Ok(())
            } else {
                Err(format!("no BENCH output from: {}", missing.join(", ")))
            }
        };
        benchgen::generate(
            &mut graph,
            &spec,
            &node,
            &mut gen,
            &mut check,
            BENCH_RETRIES,
        )?;
        base = record_view(&mut store, &graph, &run_id, Some(&base))?;
        eprintln!("bench view {}", base.id.0);
    }

    let mut evaluate = |g: &Graph| -> Evaluation {
        let assembled = assemble(g);
        let tests = verify::run_tests(&crate_dir, &assembled, &[]).unwrap_or_default();
        let passed = tests.compiled && tests.outcomes.iter().all(|o| o.passed);
        let benches = if passed {
            bench::run_benches(&crate_dir, &assembled, &[], &cfg).unwrap_or_default()
        } else {
            Vec::new()
        };
        Evaluation { tests, benches }
    };
    let mut generators = generator_pool(&plugin, width);
    let report = variants::tournament(
        &mut store,
        &base,
        &node,
        n,
        &spec,
        &mut generators,
        &mut evaluate,
        &cfg,
        &run_id,
    )
    .map_err(io)?;
    print!("{}", cli::format_report(&report));
    Ok(())
}

fn edit_cmd(args: Vec<String>) -> Result<(), String> {
    let flags = Flags::parse(args, &["--spec", "--provider", "--provider-args"], &[])?;
    let [store_dir, view_arg, crate_dir, ids @ ..] = flags.positional.as_slice() else {
        return Err("expected <store> <view> <crate_dir> <node ids...>".into());
    };
    if ids.is_empty() {
        return Err("name at least one node to edit".into());
    }
    let spec = std::fs::read_to_string(flags.required("--spec")?).map_err(io)?;
    let crate_dir = PathBuf::from(crate_dir);
    let mut store = open_store(store_dir)?;
    let base_id = cli::resolve_view(&store, view_arg)?;
    let base = store.get_view(&base_id).map_err(io)?;
    let mut graph = view::to_graph(&store, &base).map_err(io)?;
    graph.spec = spec;
    for id in ids {
        let id = cli::parse_node(&base, id)?;
        let node = graph.nodes.get_mut(&id).ok_or_else(|| id.0.clone())?;
        if !matches!(node.kind, NodeKind::Function | NodeKind::Method) {
            return Err(format!("{} has no body to rewrite", id.0));
        }
        node.body = None;
    }

    let plugin = open_plugin(
        flags.values.get("--provider").map(String::as_str),
        flags.values.get("--provider-args").map(String::as_str),
    )?;
    let mut gen = plugin.generator(None);
    let run_id = new_run_id();
    let mut run_tests = |g: &Graph, only: &[NodeId]| {
        verify::run_tests(&crate_dir, &assemble(g), only).unwrap_or_default()
    };
    let (outcome, edited) = fill_all_versioned(
        &mut graph,
        &mut gen,
        &mut run_tests,
        FILL_RETRIES,
        &mut store,
        &run_id,
        MODEL,
        Some(&base),
    )
    .map_err(io)?;
    for (id, why) in &outcome.failed {
        eprintln!("  failed {}: {why}", id.0);
    }
    let report = all_tests(&crate_dir, &graph);
    store
        .put_results(Results {
            view: edited.id.clone(),
            tests: cli::outcomes_of(&report),
            benches: Vec::new(),
            clippy_clean: None,
            recorded_ms: now_ms(),
        })
        .map_err(io)?;
    println!("{}", edited.id.0);
    if !outcome.failed.is_empty() || graph.nodes.values().any(|n| !n.is_filled()) {
        return Err("some nodes were not filled; crate directory left unchanged".into());
    }
    write_back(&crate_dir, &graph).map_err(io)
}

fn dispatch(args: Vec<String>) -> Result<bool, String> {
    let mut args = args;
    let command = args.first().cloned().unwrap_or_default();
    let known = [
        "schema", "history", "diff", "graph", "failures", "good", "sweep", "undo", "swap",
        "export", "ab", "edit", "run",
    ];
    if !known.contains(&command.as_str()) {
        return run(parse_args(args)?);
    }
    args.remove(0);
    match command.as_str() {
        "run" => return run(parse_args(args)?),
        "schema" => schema_cmd(args)?,
        "history" => history_cmd(args)?,
        "diff" => diff_cmd(args)?,
        "graph" => graph_cmd(args)?,
        "failures" => failures_cmd(args)?,
        "good" => good_cmd(args)?,
        "sweep" => sweep_cmd(args)?,
        "undo" => undo_cmd(args)?,
        "swap" => swap_cmd(args)?,
        "export" => export_cmd(args)?,
        "ab" => ab_cmd(args)?,
        _ => edit_cmd(args)?,
    }
    Ok(true)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    match dispatch(args) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
