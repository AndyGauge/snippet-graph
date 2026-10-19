---
title: "How it works"
lead: "The pipeline, in the order it runs, with the numbers it runs under."
description: "Skeleton, tests first, fill by level, repair, prune, sweep, handoff."
---

The command is `sg <spec.md> <crate_dir>`. Options include `--oracle` (a hand-written test file to check the result against), `--prelude` (finished code to build on), `--width` (how many requests to run at once), `--store` and `--handoff` (where history and the report go), and `--no-review` (skip the skeleton review).

## 1. Skeleton

The model writes one `lib.rs` containing every type, constant, signature and a `///` contract for each function, with every body exactly `todo!()`. The file is parsed into nodes and checked with `cargo check`. If it does not compile, the errors go back to the model, up to four times.

The prompt asks for each function to be one job, and for a doc comment that says what it returns, which inputs it treats specially, and any rounding or ordering rule from the spec. Those comments are not decoration: they are what the functions that call it will see instead of its body.

## 2. Skeleton review

Before any test or body exists, each contract is checked for functions that bundle several duties. A flagged function is sent back for a rewrite that splits it into helpers. [Details on the review page](../review/).

## 3. Tests first

For each function, small tests are generated from the **spec alone**. A test has to compile and has to fail on the skeleton. One that passes against `todo!()` proves nothing and is discarded. A function whose tests keep failing gets its tests regenerated, with the instruction to assert only what the spec states, because a test can encode the model's wrong guess as easily as a body can.

Finished code from earlier modules can be passed in as a **frozen prelude**: it is shown to the model, never rewritten, never blamed.

## 4. Fill by level

Nodes are sorted into levels. A function that uses no other function is level 0; otherwise it is one deeper than the deepest thing it uses. Each level is filled in parallel, and each function's prompt carries the whole spec, its own signature and contract, and the signatures and contracts of what it uses. After every level the crate is assembled and compiled, and a failure is attributed to a node by the line it falls on.

## 5. Repair

The tests run, failures are traced to the functions most likely at fault ([blame](../failures/)), and up to four independent suspects are retried together. This repeats for up to four rounds. A function that exhausts its attempts keeps its best earlier body.

| Limit | Value |
|---|---|
| Skeleton attempts | 4 |
| Retries per function in one fill pass | 3 |
| Repair rounds | 4, each retrying up to 4 independent suspects |
| Attempts per function over the whole run | 10 (a repeated answer is free) |
| Skeleton review rounds | 2 |

## 6. Prune and sweep

Unused imports and private helpers nobody calls are removed together with their tests. Then a **sweep** moves any function that still fails back to the newest version that did pass, and keeps the change only if it does not increase the number of failing tests.

## 7. Handoff

A git repository with a commit per stored state, and a report. [See the handoff page](../handoff/).

## What each model call sees

This is the design choice that does the most work. A function is never shown a neighbour's body. It gets signatures and contracts, so it can only rely on what the contract promises. That keeps prompts small and means a function can be replaced without anyone downstream depending on how it was written. It is also the weak point: a function that depends on exactly how a sibling behaves needs that behaviour written into the sibling's contract.
