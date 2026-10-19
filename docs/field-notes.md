# Snippet graph: field notes

A running log of what we hit while building and using the snippet graph, with the smallest example that shows each problem. Newest entry last.

## 2026-10-19: libtest JSON

We replaced matching the test harness's human output (`test x ... FAILED`) with libtest's JSON events. It works, but it has sharp edges. Each example below is a complete crate you can paste.

### 1. It is nightly-only

```toml
# Cargo.toml
[package]
name = "jt"
version = "0.1.0"
edition = "2021"
```

```rust
// src/lib.rs
#[cfg(test)]
mod t {
    #[test] fn good() { println!("hello"); assert_eq!(1, 1); }
    #[test] fn bad() { println!("noise"); assert_eq!(1 + 1, 3, "math is hard"); }
    #[test] fn boom() { let v: Vec<u8> = vec![]; let _ = v[2]; }
}
```

On stable 1.95 and 1.99:

```
$ cargo test -- --format json
error: The "json" format is only accepted on the nightly compiler with -Z unstable-options

$ cargo test -- -Z unstable-options --format json
error: the option `Z` is only accepted on the nightly compiler
```

Workaround, confirmed on macOS (1.95) and the aarch64 Linux box (1.99): set `RUSTC_BOOTSTRAP=1`.

```
$ RUSTC_BOOTSTRAP=1 cargo test --no-fail-fast -- -Z unstable-options --format json
```

Risk: this is an unstable interface behind an escape hatch. A release can change the fields or close the hatch. Recheck after every Rust upgrade.

### 2. Two JSON schemas share stdout

With `--message-format=json` on cargo and `--format json` on the test binary, stdout carries both. They are told apart by the key that holds the kind: cargo uses `reason`, libtest uses `type`.

```
{"reason":"compiler-artifact", ...}
{"reason":"build-finished","success":true}
{ "type": "suite", "event": "started", "test_count": 3 }
{ "type": "test", "event": "started", "name": "t::bad" }
```

A parser that only looks at one key will treat the other schema's lines as noise or as errors.

### 3. A failure is one opaque string

The failing event has no message, file or line fields. Everything is inside `stdout`, with captured prints first and the panic after:

```
{ "type": "test", "name": "t::bad", "event": "failed", "stdout": "noise\n\nthread 't::bad' (10217147) panicked at src/lib.rs:4:43:\nassertion `left == right` failed: math is hard\n  left: 2\n right: 3\n" }
```

Consequences:

- The location (`src/lib.rs:4:43`) and the assertion's `left`/`right` have to be pulled out of text again.
- The panic line contains the thread id (`10217147`), which changes every run. Do not hash or compare failure text without stripping it.
- A backtrace note is appended when `RUST_BACKTRACE` is unset: `note: run with RUST_BACKTRACE=1 ...`. It appeared on one failure in the sample run and not on the other.

### 4. Passing tests carry no output; `--report-time` is opt-in

```
{ "type": "test", "name": "t::good", "event": "ok" }
```

`good` printed `hello`, which is gone. Adding `--report-time` adds `"exec_time"`, and the suite line always carries one.

### 5. Doc tests add a second, empty suite

Even with no doc tests, a second suite follows:

```
{ "type": "suite", "event": "started", "test_count": 0 }
{ "type": "suite", "event": "ok", "passed": 0, "failed": 0, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.000032539 }
```

Do not treat the last `suite` line as the result of the whole run. Count `test` events instead.

### 6. A test that kills the process leaves no result (the one that matters most)

A model-written function that recurses without end overflows the stack. That aborts the whole test binary, so no `failed` event is ever written for it.

```rust
#[cfg(test)]
mod t {
    #[test] fn fine() {}
    #[test] fn overflow() {
        fn f(n: u64) -> u64 { if n == u64::MAX { 0 } else { f(n + 1) + 1 } }
        assert_eq!(f(0), 1);
    }
}
```

```
$ RUSTC_BOOTSTRAP=1 cargo test --no-fail-fast -- -Z unstable-options --format json --test-threads=1
{ "type": "suite", "event": "started", "test_count": 2 }
{ "type": "test", "event": "started", "name": "t::fine" }
{ "type": "test", "name": "t::fine", "event": "ok" }
{ "type": "test", "event": "started", "name": "t::overflow" }
        <- nothing more from this binary; no "failed" for overflow, no suite result
(cargo, on stderr) process didn't exit successfully: ... (signal: 6, SIGABRT: process abort signal)
```

The old text parser dropped this test too: no line ever said it failed. A parser that only collects `ok` and `failed` sees one passing test and nothing wrong, so a function whose test crashes the process looked acceptable.

Fix in `snippet-graph/src/verify.rs`: remember every `started` name and, at the end, report any that never got a result as failed, with the last lines of stderr as the message. Regression test: `run_tests_reports_a_test_that_aborts_the_process`.

Things this still does not give us:

- Tests that had not started yet when the process died have no event at all, so they look absent, not failed. In a repair round they are re-run once the crashing function is replaced.
- With the default parallel threads, several tests may be `started` when the process dies; all of them are reported failed, though only one caused it. Running with `--test-threads=1` would identify the culprit but loses the speed.

### 7. Why the old parser was wrong, minimally

A test that prints a line shaped like the harness's own output confuses a text parser:

```rust
#[test]
fn noisy() {
    println!("test adds ... FAILED");
    println!("---- adds stdout ----");
    panic!("real failure");
}
```

Text matching reported the unrelated test `adds` twice, once passing and once failing. JSON reports it once, passing. Regression test: `run_tests_ignores_lookalike_lines_in_test_output`.

### Upstream status of these findings

Checked 2026-10-19 by web search and by reading the RFC; I did not open the individual GitHub issues.

| Our finding | Known upstream? |
|---|---|
| 1. Nightly-only | Yes. Tracking issue [rust-lang/rust#49359](https://github.com/rust-lang/rust/issues/49359); stabilisation path in [eRFC 3558](https://rust-lang.github.io/rfcs/3558-libtest-json.html); project goal ["Finish the libtest json output experiment"](https://goals.rust-lang.org/2026/libtest-json.html) is Accepted with no owner on its tasks. Made nightly-only in 1.70. |
| 6. A test that kills the process leaves no result | Yes, in Cargo's terms. The RFC's motivation lists "Lack of failure when test binaries exit unexpectedly" ([#87323](https://github.com/rust-lang/rust/issues/87323), as linked from the RFC). I found nothing specific to the JSON stream missing a result for a stack overflow, so a minimal example may still be worth filing against the tracking issue. |
| 3. Failure is one opaque string, no file or line | Partly. The RFC lists "Lack of `rendered` field" and "Format is underspecified" on #49359. An internals thread asks for [source position in the JSON output](https://internals.rust-lang.org/t/libtest-provide-position-in-json-output/23169). Not the same request: ours is structured panic message and location per failure. |
| 5. Extra empty doc-test suite | Related only. The RFC lists "Suite/child relationship is missing" and "Lack of suite name". Nothing found about the empty second suite itself. |
| 2. Cargo and libtest JSON share stdout under different keys | Nothing found. |
| Thread id in the panic text | Nothing found. |
| Concurrent lines interleaving | Fixed: stdout was unlocked, so JSON lines could interleave; merged in 1.70 ([#109484](https://github.com/rust-lang/rust/pull/109484)). Worth knowing because our parser reads line by line and skips lines that do not parse. |

The RFC also says the format is not stable and exempts it from semver; `cargo_metadata` repeats that for its copy of the structs. Our reliance on `RUSTC_BOOTSTRAP=1` is on the same footing.

## 2026-10-19: the same probes against libtest2

Ed Page's replacement harness, libtest2, was cloned and built, and its own tests were run: all pass (15 + 29 + 30 + 2, plus doc tests, on `epage/libtest2` `main`). I then ran the cases from the notes above through it, using a test crate with `harness = false`.

Which code: `epage/libtest2` is a fork whose `main` stops at 2025-07-10. The live project is `crate-ci/libtest2` (latest commit 2026-10-04, tagged releases such as `libtest-json-v0.0.2`). The fork's `update` branch (2026-03-03) gives the same events as upstream `main`. The event format changed between the stale `main` and the later code, so "libtest2's JSON" is not one format yet.

Toolchains: stable 1.95 and nightly 1.101 (2026-10-12). For the current code, stable and nightly produce the same 27 events for the probe (compared ignoring timing and order). Nightly does not change anything for libtest2.

| Our finding | Standard libtest | libtest2 |
|---|---|---|
| 1. Needs nightly | `--format json` and `-Z` are refused on stable; only `RUSTC_BOOTSTRAP=1` works | `--format` is gated: `` `--format` requires `-Zunstable-options` ``. But `-Zunstable-options` is accepted on **stable** with no bootstrap variable. Same on nightly. |
| 2. Two schemas share stdout | Cargo uses `reason`, libtest uses `type` | Cargo uses `reason`, libtest2 uses `event` (no `type`). Through `cargo test --message-format=json` both appear on stdout, told apart by which key is present. |
| 3. Failure is one opaque string | `stdout` holds captured prints plus the panic text with file, line, column and a thread id | Panic message is its own field (`"test panicked: ..."`), with no thread id. But the **location is gone from the JSON**: `thread 'x' panicked at tests/probe.rs:14:5` goes to stderr, uncaptured. Current code sends failures as separate `case_message` events (`kind` error or ignored), and a test that sends two of them gets two on the stream. But the harness does not count them: pretty output shows only the last, and the process exits 0 even though it prints `test result: FAILED`. |
| 4. Passing tests carry no output | Captured and dropped | Nothing is captured. `println!` output goes straight to the process stdout, in the middle of the event lines. `--no-capture` and `--show-output` return "not supported at this time". |
| 5. Extra empty doc-test suite | Yes | Not applicable: there is a discovery phase and a run phase, and no second empty suite. |
| 6. A test that kills the process | `started`, then nothing | Same: `case_start` for the test, then nothing, no `run_complete`; the abort message is on stderr only. |
| 7. A line shaped like the harness's own output | Safe: test output is inside a JSON string | **Not safe**: stdout is shared. A test that prints `{"event":"case_complete","name":"honest","elapsed_s":"0.0"}` produced a `case_complete` for another test, *before that test's own `case_start`*. In the current schema a pass is a `case_complete` with no error message, so a reader that takes the first event per test sees a pass. The real failure arrives afterwards. |

Other differences seen:

- Event names changed between the two versions: kebab-case with `suite-start` and a `status`/`message` on `case-complete` (July 2025), then snake_case with `run_start`, `case_message` and a bare `case_complete` (current). A parser written for one breaks on the other.
- There is a JSON Schema file (`crates/libtest-json/event.schema.json`) with eight event types, which answers the old "format is underspecified" complaint. It has no fields for output, location or exit status.
- Pass and fail are not explicit in the current format: a case passes if it completes without an error message, and an ignored case emits a `case_message` of kind ignored.
- `elapsed_s` is a string on every event, as time since start, not a per-test duration.

What this means for our tool: libtest2 is not a drop-in. Our generated tests are ordinary `#[test]` functions, and libtest2 needs `harness = false` and its own attribute. So it is a design comparison for now. The parts of our parser that depend on the current format (the `started` without a result rule, skipping non-JSON lines) would still be needed, and the forged-line case means we could not trust that every JSON line came from the harness: a reader would have to accept a `case_complete` only after that test's own `case_start`, or wait for output to be captured. Capturing output is the missing piece for our use: without it the panic location, which is what we put in retry prompts, is not in the event stream.

Reproduce: clone `https://github.com/crate-ci/libtest2`, add it as a path dev-dependency, write tests with `#[libtest2::test]`, and run the test binary with `-Zunstable-options --format json`.

## 2026-10-19: retries and rollbacks

Two rules replaced the old per-pass retry counters.

**Rollbacks are free.** When the model gives back exactly a body it already tried for the same function, the result cannot be different, so the tests are not run again, the earlier failure is reused, and nothing is charged. The next prompt adds "that exact body was already tried and failed this way; write a different implementation". There is no allowance. The one guard is against a stuck model: 20 repeats in a row with nothing new in between, and the next repeat counts as a retry. Rollbacks are reported separately in the run summary.

**Every function has a lifetime budget of 10 distinct attempts**, counted across the first fill and every repair pass, not reset per pass. A function that spends it is no longer chosen for repair, keeps the best earlier body it had (or `todo!()` if it never compiled), and shows as "budget spent" in `HANDOFF.md`. The per-pass limit of 3 retries still decides when to stop a pass and regenerate a function's tests; the budget is what ends the work on it.

Scope: the free-rollback rule is in the repair path, where nearly all the retries are spent (79 in the failing commission run). The first compile-only fill still counts every round, but it does charge the same lifetime budget.

## 2026-10-19: structural problems show up as failures, not as warnings

Three commission runs on the same spec: one failed (79 retries), one passed (5), and the third stalled on the same function as the first. The function was `month_summary`, which filters, sorts, groups, sums and counts in one body. Nothing in the tool said "this function is too big"; it only said "this attempt failed" again and again.

Signals now collected, all offered to the retry prompt and to the handoff report:

- The model's own note: the fill prompt lets it begin a body with `// TODO: does too much: <steps>`, which is parsed into a finding.
- Measured size: code lines over 30, cyclomatic complexity over 8 (branches, match arms beyond the first, loops, `&&` and `||`), nesting over 3.
- Test load: more than 5 generated tests reach one function, which usually means the spec describes several functions.

When a function is repaired and any of these applies, its next prompt says it was judged too large and asks for a restructure: each distinct step in its own small nested `fn`, with the outer body a short sequence of calls.

Known weakness: nested functions inside one node are still one node, so a failure is still blamed on, and retried as, the whole function. A real split into separate nodes (new signatures, new tests) is the next step, and the signature-revision code that could support it is not wired in yet.

## 2026-10-19: catching "does too much" before any code exists

Looking back at a bad run's skeleton, the test count for the problem function (`month_summary`, 4 tests) was under the limit we had just set at 5, so the check we built after the failure would have missed it. What did show it was the `///` contract the skeleton wrote: one filter, one ordering rule, four aggregates and an exclusion in one function. Across three runs, `month_summary` scored 3 to 7 duties, `rep_statement` 4, and every other function 0, with contracts of 95+ and 73+ words against 49 or fewer.

So the check moved to where the contract is first written: after the skeleton compiles and before tests or bodies. A flagged function sends the skeleton back to the model with the reasons, to be written again with the steps split into private helpers; the rewrite is compile-checked and rejected if it drops or changes anything. Doing it before bodies exist means no node surgery: nothing yet depends on the old shape.

Open: whether the model splits well, whether a function scored 4 (like `rep_statement`) should be split at all, and the post-fill case, which still needs real surgery.
