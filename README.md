# snippet-graph

Build Rust code the way a person does: one small piece at a time, with the compiler and the tests checking each step.

Asking a model for a whole file at once skips the loop a person follows. `sg` puts it back. It treats a program as a graph of snippets, where each node is a function, type, constant or test and each edge is a use, and works through it in order:

1. Write a **skeleton**: every signature, every body `todo!()`. It must compile.
2. Write the **tests first**, from the spec, and require each to fail on the skeleton.
3. **Fill the bodies in dependency order**, in parallel where nothing depends on anything else. A function sees the signatures and contracts of what it uses, not their bodies.
4. When a test fails, **blame one function** and retry only that one, with the failure in its prompt.
5. Keep **every attempt** in a content-addressed store, so any step can be undone or compared.
6. Hand back a best-effort result and a report that says, for each function, whether it works, how many attempts it took and where a person should start.

A function that cannot be fixed after 10 attempts stops being retried. The last version that compiled is put back, and the function goes to the top of the handoff list with its last error.

## Install

```sh
cargo install snippet-graph sg-provider-openai
```

You need a Rust toolchain on the machine, because `sg` runs `cargo` to compile and test what it writes. Test results are read from libtest's JSON output, which is unstable, so `sg` sets `RUSTC_BOOTSTRAP=1` for that one command on a stable compiler.

## Run

```sh
export SG_OPENAI_BASE_URL=http://localhost:8001/v1
sg spec.md ./my-crate --oracle checks.rs --width 8
```

`spec.md` says what the crate should do. `--oracle` is an optional file of tests you wrote and the generator never sees; the result must pass them. `sg --help` lists everything else, including `--resume`, `--store`, `--handoff` and `--no-review`.

## Providers

`sg` has no HTTP client and no model SDK. It talks to a provider, a program named `sg-provider-NAME`, over JSON lines on stdin and stdout, and picks one with `--provider NAME` (or `SG_PROVIDER`; the default is `openai`). This repository ships `sg-provider-openai`, for any OpenAI-compatible endpoint such as `llama.cpp`, vLLM, Ollama or a hosted API. A provider announces how many requests it can serve at once, and `sg` never asks for more. The protocol is documented in [`src/plugin.rs`](src/plugin.rs).

## As a library

The graph, versions, ledger, review, verification and handoff are a library. It reaches a model through one trait, `Generator::generate(&mut self, prompt) -> Result<String, String>`, so it carries no model dependency.

## The site and the code graph

[`site/`](site) is a Hugo and Svelte site that explains the design and walks this repository as a graph of snippets, read from the source by `sg graph`. Edges come from parsing with `syn`, not from the compiler: calls through a path or `self` are exact, calls on other values are matched by method name and marked as possible.

```sh
make site       # build into site/public
make site-dev   # serve with live reload
```

## Development

```sh
make check      # clippy with warnings denied, then all tests
```

The design is in [`docs/design.md`](docs/design.md), the plan for test-driven fill in [`docs/test-driven-fill-plan.md`](docs/test-driven-fill-plan.md), and a log of what went wrong and how it was fixed in [`docs/field-notes.md`](docs/field-notes.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
