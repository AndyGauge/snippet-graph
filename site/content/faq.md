---
title: "FAQ"
lead: "Questions that come up, answered from the code and the runs."
description: "Why snippet-graph is not published as its own crate, which models it works with, and what it needs."
---

## Why does the library not depend on a model SDK?

It reaches a model through one trait with one method, `Generator::generate(&mut self, prompt) -> Result<String, String>`, and the `sg` command reaches a *provider* through a small plugin protocol: JSON lines over a child process's stdin and stdout. A provider is any executable named `sg-provider-NAME`. So the crate carries no HTTP client and no vendor SDK, and a new backend is a new small program, not a change here. The graph, versions, failure ledger, review, handoff report and verification are all independent of which model answers.

## Which models does it work with?

Anything a provider can reach. This repository ships `sg-provider-openai`, which talks to any OpenAI-compatible endpoint (`llama.cpp`, vLLM, Ollama and hosted services). The runs on these pages used a 120-billion-parameter general model served by `llama.cpp`. A faster coding model was timed but not yet judged on correctness with the tool. See [Use it](../use-it/) for how to pick a provider.

## Is it only for Rust?

Yes. It depends on the Rust compiler to check the skeleton and each step, on `cargo test` for results, and on a Rust parser to split a crate into nodes. The idea carries over to any language with a fast compiler and a test runner. The code does not.

## How is this different from generating a file and retrying?

A whole-file retry throws away what was right to fix what was wrong, and it cannot say which part failed. Here a failure is traced to a function, only that function is retried, the rest is frozen, and each attempt is stored. The cost is more, smaller calls, plus the work of writing a spec precise enough to generate tests from.

## What does it need to run a test crate?

A Rust toolchain. For results it asks the test harness for JSON, which is not yet stable, so on a stable compiler it sets `RUSTC_BOOTSTRAP=1` for that one command. If the format changes in a future release, the parser is in one function.

## What happens to a function it cannot fix?

After 10 distinct attempts it stops being retried. If an earlier version compiled, that version is put back. Otherwise it stays `todo!()`. Either way it is at the top of the "start here" list in the handoff report with its last error.

## Where is the code?

At [github.com/andygauge/snippet-graph](https://github.com/andygauge/snippet-graph). The [Code](../code/) page walks the repository itself as a graph of snippets, the same way `sg` sees a crate it is writing.
