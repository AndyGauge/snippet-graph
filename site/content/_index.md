---
title: "Write code the way a person does"
tagline: "build a program one small piece at a time"
lead: "Asking a model for a whole file at once skips the loop a person follows: sketch the shape, get it compiling, fill in one piece, check, repeat. snippet-graph puts that loop back."
description: "snippet-graph builds Rust code one function at a time, with the compiler and tests checking each step, and hands back a best-effort result with a plain list of what is still wrong."
---

## The problem with one shot

When a model writes a whole module in one reply, three things go wrong. A mismatch between two pieces shows up late, after both are written. One bad line invalidates the file, so the whole file is redone. And every piece is written with far more context than it needs, which makes each one worse.

## The idea

Treat the program as a **graph of small snippets**. Each node is one function, type or constant, with a signature, a body and an edge to everything it uses. Then work the way a person does:

1. Write a **skeleton**: every signature, with every body `todo!()`. It must compile, so the compiler checks the whole design before any logic exists.
2. Write the **tests first**, from the spec, and require each to fail on the skeleton.
3. **Fill the bodies in dependency order**, in parallel where nothing depends on anything else. Each function sees the signatures and contracts of what it uses, not their bodies.
4. When tests fail, **blame the one function** most likely at fault and retry only that, with the failure in its prompt.
5. Keep **every attempt**, so any step can be undone or compared.
6. Hand back a **best-effort result** and a list that says, for each function, whether it works, how many attempts it took, and where a person should start.

{{< island name="GraphPlayground" label="Step through the pipeline on a real graph" >}}

## What you get

```text
$ sg spec.md ./my-crate --oracle checks.rs
```

A crate that builds, a store with every attempt, a JSON summary, and next to the crate a git repository with one commit per stored state and a `HANDOFF.md`. The report is the point: it is written for the next reader, a person or a stronger model, to finish the job from.

<ul class="cards">
  <li><a href="how-it-works/">How it works</a><p>The pipeline, stage by stage, with the limits it runs under.</p></li>
  <li><a href="history/">History</a><p>Every attempt is stored. Undo, swap, compare, and race variants.</p></li>
  <li><a href="failures/">Failures</a><p>Blame, the failure ledger, free rollbacks and the attempt budget.</p></li>
  <li><a href="review/">Review</a><p>Catching a function that does too much before any code exists.</p></li>
  <li><a href="handoff/">Handoff</a><p>What the report says, and why it is the product.</p></li>
  <li><a href="results/">Results</a><p>What was measured, with its limits, and what went wrong on the way.</p></li>
  <li><a href="faq/">FAQ</a><p>Including why this is not yet a crate on its own.</p></li>
</ul>

## Status

It works on small, well-specified pieces and has built a nine-module business-rules crate whose three acceptance figures come out right. It is Rust only, it needs a compiler, and most numbers here come from single runs on one machine. The [results page](results/) says what is not proven.
