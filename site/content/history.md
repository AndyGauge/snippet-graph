---
title: "History"
lead: "Every attempt at every function is kept, so any step can be undone, compared or raced."
description: "Content-addressed versions, views, undo, swap, export to git, and A/B variants with benchmarks."
---

## Versions and views

A **version** is one implementation of one node. Its id is a hash of the node's kind, signature, body and impl header, so the same text is the same version wherever it came from. A **view** is a choice of one version for every node, plus the edges between them. A crate is rendered from a view. A view's parent is the view it came from, so a run is a chain of views, from the first skeleton to the final state.

Results (which tests passed, whether clippy was clean, benchmark timings) are stored per view and not per version, because the same version can behave differently beside different neighbours.

The store is a directory of JSON files, `versions/`, `views/` and `results/`, plus an append-only order log. Writes are atomic and every read checks that the content still matches its hash.

## Commands

| Command | What it does |
|---|---|
| `sg schema <spec.md> <crate> [--guide FILE] [--out FILE]` | Write only the schema: types and signatures with `todo!()` bodies, plus the dependencies they need. Compile errors are traced to the item they sit in and only those items are rewritten |
| `sg history <store> [--node ID]` | The chain of views, or every version of one node |
| `sg diff <store> <a> <b>` | Which nodes differ between two views |
| `sg undo <store> <view>` | The view this one came from |
| `sg swap <store> <view> <node> <version>` | A new view with one node set to another version |
| `sg export <store> <view> <dir> [--commit]` | Write a view out as a crate, optionally as a git commit |
| `sg edit <store> <view> <crate> --spec FILE <nodes...>` | Rewrite chosen nodes of an existing view against its tests |
| `sg ab <store> <view> <node> ...` | Race variants of one function |

## Racing variants

`sg ab` asks for several implementations of one function in parallel, keeps those that pass the tests, benchmarks them against the current one (median, spread, and a noise threshold so a wobble is not called a win), and builds a new view with the winner after checking the combination still passes.

On a text-statistics spec, `word_counts` went from 516 ms to 174 ms, about three times faster. The winner counted with borrowed string keys instead of cloning every token, pre-sized the map, and used an unstable sort where the key is total. All candidates and the winning view are stored.

## Why bother

Because a repair loop that only remembers the latest attempt cannot tell that the model just gave the same answer again, cannot go back to a version that worked, and cannot show a person what was tried. With the whole history, those become cheap: [a repeat is a free rollback, and a sweep returns a failing function to its last good version](../failures/).
