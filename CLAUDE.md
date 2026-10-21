# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`sg` (crate `snippet-graph`) builds Rust code one function at a time: compiling skeleton (bodies `todo!()`), tests first, fill in dependency order, blame-and-retry of the single failing function, with every attempt kept in a content-addressed store. It shells out to `cargo` to compile and test the code it generates, so a Rust toolchain must be present. Libtest JSON output is unstable, so `sg` sets `RUSTC_BOOTSTRAP=1` for that one command.

## Commands

- `make build` / `make test` / `make lint` (clippy, `-D warnings`) / `make check` (lint then test). All use `--workspace`.
- Single test: `cargo test -p snippet-graph <name>` (integration tests are in `tests/`, e.g. `cargo test --test oracles`).
- Run the CLI: `cargo run --bin sg -- spec.md ./out-crate --oracle checks.rs --width 8`. `sg --help` lists subcommands (`schema`, `history`, `diff`, `undo`, `swap`, `export`, `ab`, `edit`, `graph`, ...).
- Site: `make site-data` regenerates `site/data/graph.json` by running `sg graph` on this repo; `make site` builds Hugo + Svelte (`site/web`) into `site/public`; `make site-dev` serves it.
- Release: `make publish` (needs `PUB_NEW_CRATE_TOKEN`/`CARGO_REGISTRY_TOKEN` in `.env`, clean git tree). Publishes `snippet-graph` first, then `sg-provider-openai`.

## Architecture

Cargo workspace: the `snippet-graph` library + `sg` binary (`src/bin/sg.rs`, thin front for `src/cli.rs`), and `sg-provider-openai`.

**No model dependency in the library.** Models are reached only through the `Generator` trait (`generate(&mut self, prompt) -> Result<String, String>`). The binary's real generator (`src/plugin.rs`) spawns an external `sg-provider-NAME` process and speaks JSON lines over stdin/stdout; the protocol is documented in `src/plugin.rs`. A provider advertises its concurrency and `sg` never exceeds it. Changes to the protocol must keep `src/plugin.rs` and `sg-provider-openai` in step.

Pipeline, roughly in order (modules in `src/`):
- `model.rs` — `Graph`, `Node`, `NodeKind`, `Placement`, test reports: the core data types. The graph is the source of truth; files are a projection of it (`assemble.rs`).
- `skeleton.rs`, `schema.rs`, `review.rs` — skeleton generation, the standalone `schema` stage (per-item compile-error repair and `upgrade` from an existing schema), and skeleton review.
- `edges.rs`, `order.rs`, `structure.rs` — "uses" edges (including method-call edges), topological levels for parallel fill.
- `testgen.rs`, `fill.rs`, `parallel.rs`, `blame.rs`, `revise.rs`, `lintfix.rs`, `verify.rs` — tests-before-bodies (each must fail on the skeleton), level-by-level fill, blaming the deepest failing function, signature revision, and compile/test/clippy verification.
- `store.rs`, `version.rs`, `view.rs`, `ledger.rs`, `export.rs`, `import.rs`, `session.rs` — content-addressed versions, views mapping nodes to versions, the attempt ledger, export to a crate/git, and resumable sessions.
- `handoff.rs`, `results.rs`, `metrics.rs` — the per-function report; a function that fails 10 attempts stops being retried, its last compiling version is restored, and it goes top of the handoff list.
- `variants.rs`, `bench.rs`, `benchgen.rs` — `sg ab`: generate variants, keep those passing tests, benchmark against baseline.
- `repo_graph.rs` — `sg graph`: parses source with `syn` (not the compiler) to produce the code graph used by the site; calls through a path or `self` are exact, calls on other values are matched by method name and marked possible.

`benches/` holds specs, oracles (tests the generator never sees) and reference solutions used by `tests/oracles.rs`. `docs/` has `design.md`, `test-driven-fill-plan.md` and `field-notes.md` (a log of failures and fixes; add to it when you learn something non-obvious).

## Conventions

Keep comments for the non-obvious why. Update `CHANGELOG.md` for user-visible changes.
