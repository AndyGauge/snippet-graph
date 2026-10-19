---
title: "Use it"
lead: "Install the command, point it at a provider, give it a spec."
description: "How to install sg, choose a model provider, and run it on a spec."
---

## Install

```text
cargo install snippet-graph
cargo install sg-provider-openai
```

Until the crates are published, build from a checkout with `cargo build --release`; the binaries land in `target/release`. You need a Rust toolchain on the machine, because `sg` runs `cargo` to compile and test what it writes.

## Run

```text
$ sg spec.md ./my-crate --oracle checks.rs
```

`spec.md` says what the crate should do. `./my-crate` is where the skeleton and then the bodies are written. `--oracle` is an optional file of tests you trust, which the generated code must pass.

## Choose a provider

`sg` finds a model through a provider, an executable named `sg-provider-NAME` on your `PATH` (or next to `sg`). Pick one with `--provider NAME`; the default is `openai`.

```text
$ export SG_OPENAI_BASE_URL=http://localhost:8001/v1
$ sg spec.md ./my-crate --provider openai --width 8
```

`--width` is how many requests run at once. A provider announces how many it can serve when it starts, and `sg` never asks for more than that.

## Write a provider

The protocol is one JSON object per line. The provider starts by printing `{"hello": true, "name": "NAME", "channels": N}`. After that each request line carries an `id`, a `prompt`, a `system` text, a `slot`, a `max_tokens` and an `effort`; the reply echoes the `id` with either `text` and token counts or an `error`. Replies may arrive in any order, which is how one provider serves many channels. The `serve` function in <code>src/plugin.rs</code> does the framing for a Rust provider; see {{< node "snippet-graph::plugin::serve" >}}.
