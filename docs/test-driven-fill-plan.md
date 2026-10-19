# Snippet Graph: Test-Driven Fill Plan

Builds on `SNIPPET_GRAPH.md` and the existing `snippet-graph` crate. The first end-to-end run (planning-store spec) compiled but shipped a behavior bug the compiler could not see (`mission.txt` instead of `mission.md`). This plan adds tests as graph nodes so behavior failures are attributed to, and retried at, individual nodes.

## Goal and acceptance

Given a spec, the pipeline produces a crate where:

1. the skeleton compiles,
2. per-node tests, written from the spec before any body exists, compile against the skeleton and fail (red),
3. every body fills with failures attributed and retried per node,
4. all generated tests pass (green), clippy is clean, and an independent hand-written oracle suite passes,
5. a re-run after changing one node re-checks only that node and its dependents.

Final gate: the planning-store spec passes its oracle suite, which includes the `mission.md` case.

## Design decisions

- **Test nodes.** A test is a node (`NodeKind::Test`) with edges to the nodes it exercises. Tests live in a `#[cfg(test)] mod tests` block that the assembler renders from Test nodes.
- **Tests before bodies.** Tests are generated from the spec and skeleton signatures only, never from bodies, so they cannot encode an implementation's mistake. A red check confirms each test fails against `todo!()` bodies and compiles.
- **Blame.** A failing test implicates the nodes it reaches. Suspects are ranked: nodes reached by few tests, nodes reached by no passing test, and most recently filled come first. A passing test clears the nodes it reaches.
- **Selective re-run.** After refilling a node, run only tests reaching that node or its dependents (`order::dependents`).
- **Edges from the compiler's view.** Method-call edges are resolved by name and receiver type where possible, because `Foo::new()` currently links only to `Foo`.
- **Oracle suite is separate.** Hand-written tests the generator never sees. They measure whether generated tests were good enough.

## Tasks

Each task owns exactly the files listed, so tasks in the same wave never touch the same file. "Depends on" means the listed tasks must be merged first.

| ID | Task | Owns | Depends on | Wave |
|----|------|------|------------|------|
| T0 | Contract: add `NodeKind::Test`, `TestReport`/`TestOutcome` types, `Graph::tests_for(node)` and `Graph::subjects_of(test)`, stubs for new modules, `lib.rs` exports | `model.rs`, `lib.rs`, stub files for T4/T5/T8/T9 | none | 0 |
| T1 | Assembler renders Test nodes into `#[cfg(test)] mod tests { use super::*; ... }`, with spans, deterministic | `assemble.rs` | T0 | 1 |
| T2 | Skeleton parser recognises test fns in `mod tests` as Test nodes, derives edges from test bodies to subjects | `skeleton.rs` | T0 | 1 |
| T2b | Method-call edge resolution (`Type::method`, `x.method()` by unique name or receiver type hint) for all nodes | `edges.rs` (new, called from `skeleton.rs` by T2 via a one-line hook added in T0) | T0 | 1 |
| T3 | Verifier runs `cargo test --message-format=json`, parses per-test pass/fail and panic text, returns `TestReport`; adds clippy pass attributing warnings to nodes | `verify.rs` | T0 | 1 |
| T4 | Blame: rank suspect nodes from passing and failing tests and the graph | `blame.rs` | T0 | 1 |
| T5 | Test generation: prompt from spec + skeleton signatures, one small test set per node, parse into Test nodes; red check helper | `testgen.rs` | T0 | 1 |
| T6 | Oracle suite and benchmark specs: hand-written tests for the planning-store spec (including `mission.md`), plus a second small spec; metrics collector (retries per node, tokens, wall time) | `snippet-graph/tests/` , `snippet-graph/benches/specs/`, `metrics.rs` | T0 | 1 |
| T7 | Fill v2: after each fill, run affected tests, use blame to choose the retry node, pass failure text into the prompt, select-and-rerun by dependents | `fill.rs` | T3, T4 | 2 |
| T8 | Signature revision: a node may request a signature change; apply it, invalidate and refill dependents, bounded revisions | `revise.rs` | T0, T2 | 2 |
| T9 | Import existing crate into a graph (bodies kept), select nodes to rewrite, write back; the edit path | `import.rs` | T1, T2 | 2 |
| T10 | Parallel fill: fill each dependency level concurrently, spread across providers with `slot_hint`, merge results deterministically | `parallel.rs` | T7 | 3 |
| T11 | Driver: spec → skeleton → tests → red check → fill (parallel) → green, line-buffered progress, nonzero exit on any failed node or red/green violation, JSON run report | `src/bin/sg.rs` | T5, T7, T10, T6 | 3 |
| T12 | End-to-end eval on both benchmark specs against oracle suites; record results; docs update; file follow-ups for gaps | `SNIPPET_GRAPH.md`, results file | T11, T8, T9 | 4 |

## Dependency graph

```
T0 ──┬─ T1 ──────────────┬─ T9 ───────────────┐
     ├─ T2 ──┬────────────┤                    │
     │       └─ T8 ───────┼────────────────────┤
     ├─ T2b               │                    │
     ├─ T3 ──┐            │                    │
     ├─ T4 ──┴─ T7 ─ T10 ─┴─ T11 ──────────────┴─ T12
     ├─ T5 ───────────────────┘
     └─ T6 ───────────────────┘
```

## Waves and parallelism

- **Wave 0 (serial):** T0 only. Small, done first so every later task builds against the same types. Must compile with all stubs.
- **Wave 1 (7 parallel):** T1, T2, T2b, T3, T4, T5, T6. Fully independent files. Largest fan-out.
- **Wave 2 (3 parallel):** T7, T8, T9. T7 needs T3 and T4; T8 and T9 need the parser and assembler.
- **Wave 3 (serial pair):** T10 then T11. T10 edits nothing T7 owns, but wraps its API, so it follows T7. T11 integrates everything.
- **Wave 4:** T12, the real evaluation.

Critical path: T0 → T3 → T7 → T10 → T11 → T12. Everything else has slack, so the long poles (T3, T7) should start first within their waves and get the strongest model.

## Per-task acceptance

- **T0:** `cargo check` clean; existing 24 tests still pass; no `todo!()` left in model code.
- **T1:** round-trip test: a graph with Test nodes renders, reparses to an equal graph; spans accurate inside the test module.
- **T2:** a source with `mod tests` parses to Test nodes with edges to the functions they call.
- **T2b:** `Foo::new()` links to the `new` method; `x.go()` links when `go` is unique, and records an ambiguous call as edges to all candidates.
- **T3:** on a temp crate with one passing and one failing test, report names both and carries the panic message; a clippy warning inside a node is attributed to that node.
- **T4:** table-driven tests of rankings: single-test blame, shared-subject blame cleared by a passing test, ties broken deterministically.
- **T5:** generated tests for the planning-store skeleton compile against it and all fail on `todo!()` (red check); non-compiling tests are rejected with the compiler error fed back for a retry.
- **T6:** oracle suite passes against a known-good reference implementation and fails against the `mission.txt` variant.
- **T7:** with a fake generator that gets one body wrong, only that node is retried; unrelated tests are not re-run; failure text appears in the retry prompt.
- **T8:** a fake generator requesting a signature change triggers refills of exactly the dependents; revisions are capped.
- **T9:** import → rewrite one node → write back leaves all other bytes unchanged.
- **T10:** wall time on a fake generator with latency drops roughly with level width; output identical to serial.
- **T11:** one command runs the full pipeline; exits nonzero when any node fails; progress visible while running.
- **T12:** planning-store passes the oracle suite end to end; metrics recorded; remaining gaps listed.

## Risks and mitigations

- **Bad generated tests.** Wrong expectations make correct code look broken. Mitigation: the red check, bounded retries on the test itself, and the oracle suite as an independent measure. Track the rate of tests revised after a body failure.
- **Spec ambiguity.** If the spec doesn't determine the behavior, tests and bodies diverge. Mitigation: tests derive only from spec text, and ambiguity surfaces as disagreement to report, not to guess around.
- **Blame noise.** Integration-style tests reach many nodes. Mitigation: prompt for small tests aimed at one node, and weight blame by reach.
- **Shared state in tests** (filesystem paths). Mitigation: generated tests must use unique temp dirs; checked in the red check.
- **Parallel fill cost.** More concurrent calls raise provider rate-limit pressure. Mitigation: use the registry's slot hints and cap concurrency per level.
- **Merge conflicts in fan-out.** Mitigation: the file ownership column; T0 adds every cross-file hook up front.

## Fan-out instructions

1. Run T0 alone, merge, confirm it compiles.
2. Launch Wave 1 as seven parallel agents, one per task, each told its owned files only and the acceptance line above. T3 and T4 first in the queue.
3. When T3 and T4 land, launch T7; when T1 and T2 land, launch T8 and T9. These can start before the rest of Wave 1 finishes.
4. After T7, launch T10; after T5, T6, T7, T10, launch T11.
5. Run T12 myself, serially, with the real providers.

---

# Part 2: Versioned Nodes and A/B Performance Testing

Status when written: Part 1 is built and committed except the final evaluation, which a second full run on the planning-store spec was still producing. Part 2 changes the core data model, so it starts after that result is read and any fixes from it are in.

## Goal and acceptance

Keep every attempt at every node, make any past state restorable, and answer "if I swap this node's implementation, do the tests still pass and does performance improve?" with a measurement attributable to that one node.

1. Nothing is overwritten. Every generated body is a stored version with its origin and results.
2. A run is a view (a map from each node to one chosen version). Any past view can be rendered back to a crate, byte for byte.
3. Swapping one node to another version produces a new view; the pipeline reruns only the tests and benchmarks that reach that node or its dependents.
4. A comparison reports correctness (tests pass or fail) and performance (median of repeated runs, with a noise threshold) for baseline against variant.
5. Failed attempts are kept with their failure text and fed into later prompts for the same node.
6. Final gate: on a spec with a performance-sensitive function, N generated variants are tested and benchmarked, the winner is selected by measurement, and the composed view passes all tests and its benchmark.

## Design decisions

- **Identity versus version.** A node keeps its stable id. A version is content-addressed: the hash of signature, body, impl header, and kind. Same content, same version, stored once.
- **Origin on every version.** Run id, model, provider, prompt hash, parent version, creation time. Failed attempts are versions too, flagged by their results.
- **Views are cheap.** A view is a small map of node id to version id. Undo moves a pointer. A branch is another view. A diff lists nodes whose versions differ.
- **Results are an index, not part of the version.** Test outcomes and metrics are recorded per (view, version) so the same version can be judged in different contexts, since a version's performance depends on its neighbours.
- **Storage is a directory, not a database server.** Content-addressed JSON files for versions, views, and results, plus an append-only run log. No new native dependencies. A view can be exported as a plain crate and committed to git for human review; git is the export format, not the store.
- **Same-signature swaps are free; signature changes are not.** A swap that keeps the signature touches only that node. A different signature creates a new node version and invalidates its dependents (reusing the existing signature-revision code).
- **Benchmarks are nodes.** A benchmark node has edges to the functions it times, exactly like tests, so selective rerun and attribution work the same way.
- **Measurement.** Repeated runs in release mode, median reported, a variant counts as faster only beyond a configured noise threshold. Instruction counts are used where the machine supports them, wall time otherwise.
- **A final composed check.** Two faster nodes can interact badly. The winning combination is benchmarked as a whole before it is accepted.

## Tasks

Each task owns exactly the files listed. "Depends on" means the listed tasks must be merged first.

| ID | Task | Owns | Depends on | Wave |
|----|------|------|------------|------|
| V0 | Contract: `VersionId`, `NodeVersion`, `Origin`, `RunId`, `View`, a `Store` trait, `NodeKind::Bench`, `BenchResult` types, an in-memory `Store` for tests, stubs for new modules, exports | `version.rs`, `model.rs`, `lib.rs`, stubs | none | 0 |
| V1 | Directory store: put and get versions, views, runs; list versions of a node; history of a node; atomic writes; hash verification on read | `store.rs` | V0 | 1 |
| V2 | View operations: build from a graph, render to a graph, diff two views, swap one node, undo, list dependents to invalidate | `view.rs` | V0 | 1 |
| V3 | Results index: record test outcomes and metrics per (view, version); queries such as passing versions of a node ordered by a metric, and failures with text for a node | `results.rs` | V0 | 1 |
| V5 | Benchmark runner: run bench nodes in release mode N times, parse timings, median and spread, instruction counts when available, noise-threshold comparison | `bench.rs` | V0 | 1 |
| V6 | Bench nodes in the assembler and parser: render `Bench` nodes in a `#[cfg(test)] mod benches`, parse them back, edges to subjects | `assemble.rs`, `skeleton.rs` | V0 | 1 |
| V10 | Git export: render a view to a directory as a crate, commit it with the view and run ids in the message | `export.rs` | V0 | 1 |
| V4 | Fill loop writes versions instead of overwriting; keeps failed attempts with failure text; prior failures for the same node go into its prompt | `fill.rs`, `parallel.rs` | V1, V2, V3 | 2 |
| V7 | Benchmark generation from the spec for performance-sensitive nodes, with a check that the bench compiles and runs against a correct implementation | `benchgen.rs` | V5, V6 | 2 |
| V8 | Variants and tournament: generate N variants for a node in parallel, keep those passing the tests, benchmark them, select by measurement beyond the noise threshold, build the winning view, run the composed check | `variants.rs` | V2, V3, V5, V4 | 2 |
| V9 | Command-line: `history`, `diff`, `undo`, `swap`, `ab`, `export`, plus edit mode (import, rewrite selected nodes) wired in; the main driver records every run as a view | `src/bin/sg.rs`, `cli.rs` | V4, V7, V8, V10 | 3 |
| V11 | Evaluation: add a spec with a performance-sensitive function, run N variants through the tournament, record the history, show an undo and a swap end to end; update docs | `benches/specs/`, `SNIPPET_GRAPH.md`, results file | V9 | 4 |

## Dependency graph

```
V0 ─┬─ V1 ──┬─────────────┐
    ├─ V2 ──┼─ V4 ────────┼─ V8 ──┐
    ├─ V3 ──┘             │       │
    ├─ V5 ──┬─ V7 ────────┼───────┼─ V9 ── V11
    ├─ V6 ──┘             │       │
    └─ V10 ───────────────┴───────┘
```

V8 also needs V2, V3, and V5 directly, and V4 for the versioned fill path.

## Waves and parallelism

- **Wave 0 (serial):** V0. The new types and the `Store` trait are the contract; the in-memory store lets every later task test against it without waiting for the directory store.
- **Wave 1 (6 parallel):** V1, V2, V3, V5, V6, V10. Disjoint files.
- **Wave 2 (3 parallel):** V4, V7, V8. V4 and V7 can start as soon as their own dependencies land; V8 last.
- **Wave 3:** V9, the integration.
- **Wave 4:** V11, run by hand against the real providers.

Critical path: V0, V1, V4, V8, V9, V11 (or through V5 if the benchmark runner is the slower of the two). Start V1 and V5 first in Wave 1.

## Per-task acceptance

- **V0:** crate compiles; existing tests pass; the in-memory store round-trips a version and a view.
- **V1:** put then get returns identical content; a corrupted file is detected by its hash; concurrent writers of the same content do not conflict; history of a node lists versions in order with parents.
- **V2:** build a view from a graph and render it back to an equal graph; a diff names exactly the nodes that differ; a swap changes one entry; undo restores the previous view; dependents of a swapped node are listed.
- **V3:** record and query results for a few views; ordering by metric is stable and ties are deterministic; failures for a node include their text.
- **V5:** on a fixture with a fast and a slow function, the runner reports the slow one slower by more than the noise threshold, and a function compared against itself is reported as no difference.
- **V6:** a graph with bench nodes renders and reparses to an equal graph; edges to subjects are present.
- **V10:** exporting a view yields a crate that builds; the commit message names the view and run.
- **V4:** a wrong body followed by a right one leaves both versions in the store; the retry prompt contains the earlier failure text; nothing is lost on interruption.
- **V7:** generated benches for a sample spec compile and run against a correct implementation and report a time.
- **V8:** with fake generators returning fast, slow, and failing variants, the failing one is excluded, the fast one wins, and a variant within the noise threshold does not displace the baseline; the composed check runs once on the winning view.
- **V9:** each command works against a store created by a real run; `undo` followed by `diff` shows the expected change.
- **V11:** end-to-end on the new spec with real providers; the history shows every variant, its test result and its timing; a swap back to the baseline reproduces its earlier timing within the noise threshold.

## Risks and mitigations

- **Noisy measurements.** Wall time on a shared machine can mislead. Mitigation: median of repeated runs, a noise threshold, instruction counts where available, and re-measuring a winner before accepting it.
- **Neighbour effects.** A version's speed depends on the rest of the view. Mitigation: results are keyed by view, and the winner gets a composed check.
- **Store growth.** Many variants per node add up. Mitigation: content addressing dedupes identical bodies; add a prune command that keeps views and what they reference.
- **Benchmarks that measure the wrong thing.** A generated bench may not reflect real use. Mitigation: benches come from the spec, are checked against a correct implementation, and are human-reviewable as ordinary nodes.
- **Complexity creep in the fill loop.** Versioning touches the hot path. Mitigation: the in-memory store and the store trait keep the existing tests running unchanged while the directory store is built behind it.

## Part 3: skeleton review (S1 to S5 built, S6 and S7 open)

Motivation: across three commission runs, one function (`month_summary`) accounted for most of the retries, and the tool had already measured it as too large before it kept retrying. The problem is visible in the skeleton, before any body or test exists. Test count would not have caught it (4 tests against a limit of 5); the `///` contract the skeleton wrote for it listed one filter, an ordering rule, four aggregates and an exclusion rule.

Where it sits: Skeleton (compiles) -> **Skeleton review** -> Tests -> Fill -> Repair. Measured size after fill stays as the last safety net.

| ID | Task | Depends on |
|---|---|---|
| S1 (built) | Skeleton prompt rule: split any function whose contract has more than about three distinct duties; write the helpers into the skeleton | none |
| S2 (built) | Cheap pre-filter on each node's contract and signature: bullet and clause count, parameter count, mentions of ordering, filtering and several aggregates | none |
| S3 (built) | Review pass: a model reads the spec and each flagged contract and proposes helper signatures with one-line contracts; the proposal must compile as stubs | S2, signature revision wired into the driver |
| S4 (built, see note) | Apply a split: add the helper nodes, rewrite the parent to call them, reset only the parent's dependents, regenerate tests for the new nodes only | S3 |
| S5 (built) | Loop S2 to S4 until nothing is flagged or a round limit is reached; record what was split and why in the handoff report | S4 |
| S6 | Same trigger after fill: a function that is over the size limits and has failed is split instead of retried | S4 |
| S7 | Evaluate: run the original monolithic commission spec through S1 to S5 and compare with the hand-split spec (`commission_split`) on retries, wall time and pass rate | S5 |

How it was built: the review runs before any body or test exists, so a split is not node surgery. A model is shown the compiling skeleton, the spec and the flagged functions, and writes the whole skeleton again with the flagged functions split into private helpers. The result is parsed and compile-checked like any skeleton, and is rejected, with the reason fed back for one retry, if it does not compile, drops any item, changes any existing signature, or adds nothing. After two failed tries the earlier skeleton is kept. Up to two rounds run (`MAX_REVIEW_ROUNDS`). The pre-filter flags a contract with more than three duties (listed results plus ordering, exclusion, filtering and empty-case rules, not counting `# Arguments` bullets), or one of 60+ words with two, or more than five parameters. Thresholds were set against the contracts from three earlier runs: `month_summary` scored 3 to 7, `rep_statement` 4, every other function 0. `sg --no-review` turns it off, for S7. Code: `snippet-graph/src/review.rs`, wired in `sg.rs`; tested with a fake model, and end to end with the real parser and compiler in `tests/review_skeleton.rs`.

Not built: S6 (splitting after fill) needs real node surgery on a graph that already has bodies and tests. S7 needs the spark. Not yet seen with a real model: whether the model's splits are good, and whether `rep_statement` (flagged at score 4) gets over-split.

Acceptance: on the original spec, the review splits `month_summary` without help and the run reaches green in about the same number of retries as the hand-split spec.

Known risk: a split adds nodes and tests, so a spec that did not need one will be over-decomposed and slower. Thresholds need tuning against runs that were fine, not only the one that was not.

## Other remaining work (independent of Part 2)

- Wire signature revision into the driver; it is built and tested but unused.
- Generate tests in parallel; they are serial now and a large share of run time.
- Cheaper lint checks; the per-level lint run is the next biggest cost after model calls.
- Handle functions that never get acceptable tests more usefully than reporting them untested.
- Bring `SNIPPET_GRAPH.md` up to date with what was built.

---

# Results

## Text statistics (small, performance-sensitive spec)

- Full pipeline run: skeleton on the first try, 49 generated tests, all functions filled, 4 retries in total, all 49 generated tests and the 9 hand-written oracle tests pass. About 52 minutes. Not marked clean only because the linter flagged two unused helper functions.
- A/B test of `word_counts` with 4 variants (benchmark generated automatically):
  - baseline 516 ms (spread 38 ms)
  - three variants about 174 to 179 ms, one unchanged at 516 ms
  - winner 174 ms, about 3x faster. It counts with `&str` keys instead of cloning every token, pre-sizes the map, and uses an unstable sort (safe because the sort key is total).
  - all candidates and the winning view are stored; `history`, `diff` and `undo` show the chain from the first skeleton through every repair attempt to the winner.

## Planning store (larger spec)

Earlier runs failed on bugs in the tooling (string literals re-indented when nested, blame choosing callers over failing helpers, a lint rule the model worked around, no request timeout). Those are fixed. A rerun is in progress at the time of writing.

## Findings by feature

One sample per row unless stated; the model is non-deterministic, so treat single runs as indications. "Not yet observed" means built and unit-tested but not seen on a real model run.

| Feature | Finding | Evidence |
|---|---|---|
| Skeleton first, compiler-checked | Interface mismatches surface before any logic exists. Skeleton compiled on the first attempt in every module run. | All nine port modules |
| Tests written before bodies, must fail on the skeleton | Catches a compiled-but-wrong result that a compile-only gate passes. A spec-derived test can still encode the model's wrong intuition, so tests need their own revision step. | Earlier `mission.txt` vs `mission.md` miss |
| Spec included in the fill prompt | Without it the model ignored the stated rounding method. | Rounding module |
| Fill in dependency order, parallel within a level | Works; a function sees only signatures of what it uses, never bodies. Likely cause of trouble where functions rely on exactly how a sibling behaves. | Commission module |
| Blame and targeted repair | Retries the deepest failing function, up to 4 independent ones per round. Early version blamed types and callers; restricted to functions and methods. | Planning store |
| Test revision | Regenerates tests for a function that keeps failing. Helped on single functions; did not rescue the commission run on its own. | Commission, first run |
| Keep the best body on failed repair | Before: a repair that ran out of retries left `todo!()`, and blame could wipe correct neighbours. First commission run: 79 retries, 44 min, 13 tests failing. After the fix, same spec: 5 retries, 9.5 min, all 20 tests and the hand-written oracle pass. Single sample each. | `commission.err` runs 1 and 2 |
| Frozen prelude of finished modules | Needed so later modules build on finished code and do not regenerate it. Without it the skeleton step turned finished functions into stubs. | Totals, commission |
| Lint prune (unused imports, dead helpers) | Stops the model gaming the lint with a helper that references everything. Does not catch every lint: commission still ended with an unused import and a needless clone in generated tests. | Commission, manufacturer |
| Content-addressed versions, views, history, undo, swap | Every attempt is stored and navigable. | Textstats A/B run |
| A/B variants with benchmarks | `word_counts` went from 516 ms to 174 ms (about 3x) with all candidates stored. | Textstats |
| Request timeout and resume | Required: a closed laptop lid once hung a run forever. The 900 s timeout bounded it; a stalled server call still cost about two hours before the timeout fired. | Manufacturer, first attempt |
| Doc comments on every generated item | Built; the skeleton prompt now asks for a contract comment (returns, skipped inputs, rounding and ordering). Not yet observed whether it reduces retries. | Spark commission run pending |
| Handoff repo (one commit per view, `HANDOFF.md`) | Built for a person or stronger model to finish a best-effort result: status per function, depth, attempts, last error, and a list of where to start. Not yet observed on a real run. | Unit tests only |
| libtest JSON for test results | Unstable (nightly-only), but `RUSTC_BOOTSTRAP=1` unlocks it on stable; confirmed on this Mac and on the spark. Replaces matching human `test x ... FAILED` lines with one event per test carrying the panic output. Not yet observed on a real model run. | Probe crate on both machines |
| Structural checks (marker, complexity, test load) | Built after the spark commission run failed on `month_summary` for the second time in three runs: one body that filters, sorts, groups, sums and counts. Fill prompts now let the model write `// TODO: does too much: <steps>` as the first line; bodies are measured for code lines (limit 30), complexity (8) and nesting (3); a function reached by more than 5 tests is flagged. A flagged function's next attempt is told to put each step in a small nested `fn`. Findings appear in `HANDOFF.md` and the run summary. Not yet observed on a real run; thresholds are guesses to tune. | Unit tests only |
| Run on the spark instead of a laptop | Cross-built `aarch64-unknown-linux-gnu` binary copied to `~/sg`; the spark already has Rust and gcc. First launch failed because `cargo` is not on the path in a non-interactive ssh shell. | Spark commission run |

### Before and after

**Keep the best body on a failed repair** (commission module, same spec, same model, one run each)

| | Before | After |
|---|---|---|
| Retries | 79 (`month_summary` 27, `net_sales` 25, `rep_statement` 18, `order_balance` 9) | 5 (`net_sales` 2, `order_commission` 3) |
| Wall time | 44 min 16 s | 9 min 34 s |
| Tests | 13 of 18 failing | all 20 pass; hand-written oracle passes |
| Model tokens (in + out) | 482,596 + 126,235 | 43,901 + 27,914 |

Final code before: the four functions the repair loop had touched were all reset to the skeleton, including two trivial ones that were probably correct earlier.

```rust
pub fn net_sales(order: &OrderRecord) -> Decimal { todo!() }
pub fn order_balance(order: &OrderRecord) -> Decimal { todo!() }
pub fn rep_statement(orders: &[OrderRecord], rep_id: u32, rate: f64, year: i32, month: u32) -> RepStatement { todo!() }
pub fn month_summary(orders: &[OrderRecord], rates: &[(u32, f64)], year: i32, month: u32) -> MonthSummary { todo!() }
```

Final code after (excerpt; the full module is 4 filled bodies plus 2 that passed first time):

```rust
pub fn net_sales(order: &OrderRecord) -> Decimal {
    order.lines.iter().map(line_total).fold(Decimal::default(), |acc, val| acc + val)
}

pub fn order_balance(order: &OrderRecord) -> Decimal {
    let totals = document_totals(&order.lines, order.tax_rate, order.shipping);
    balance_due(totals.total, &order.payments)
}
```

Caveat: the fix and the run-to-run variation are not separated. Run 1 also never got `month_summary` to compile (type annotation, `&&o` pattern, `&Vec<&T>` vs `&[T]`), and that alone blocked every dependent test. A second sample of the old build would show how much of the gap is the fix.

**libtest JSON instead of parsing the text output**

A test that prints a line shaped like the harness's own output:

```rust
#[test]
fn noisy() {
    println!("test adds ... FAILED");
    println!("---- adds stdout ----");
    panic!("real failure");
}
```

| Parser | Result for the unrelated passing test `adds` |
|---|---|
| Text matching (before) | reported twice, once passing and once failing (`left: 2, right: 1` in the regression test) |
| JSON events (after) | reported once, passing; `noisy` fails with `real failure` in its output |

The regression test is `run_tests_ignores_lookalike_lines_in_test_output` in `snippet-graph/src/verify.rs`; it fails on the old implementation and passes on the new one. No timing difference was measured: parsing cost is negligible next to compiling.

**Pending a real run (spark commission run in progress)**

- Doc comments: compare retries, time and tokens against the "after" column above (same spec, no docs), and show the generated `///` lines.
- Handoff repo: show the generated `git log` and `HANDOFF.md` for that run.

### Port of the Cairn rules (cairn-core), module by module

- Passed their own tests and the hand-written oracle: rounding, totals, rma, leads, defaults, quote_to_order, receipt, manufacturer (0 to 3 retries each; manufacturer needed one hand-removed redundant import; its rerun took 54 min wall, about 6 min of real work).
- Commission passed tests and oracle after the keep-best-body fix; clippy not yet clean.
- Not yet done: combine into one crate and reproduce the mission's acceptance figures from the fixtures.

## Known limits

- The linter's dead-code rule is enforced only at the end and has no repair step; unused helpers fail the run.
- A function whose tests cannot be made to fail against the empty skeleton is left untested (seen on a trivial constructor).
- Benchmarks come from the spec and are only checked to compile and print a timing; their quality is not otherwise verified.
- A winning variant is picked by benchmarks that reach the function directly; effects on other functions are only caught by the combined check.
