---
title: "Results"
lead: "What was measured, how much to trust it, and what went wrong on the way."
description: "A nine-module port that reproduces its acceptance figures, five runs of one spec, reasoning effort, throughput, and the failures along the way."
---

Most numbers on this page come from **one run each**, on one machine, against a model that gives different answers each time. They show what can happen, not what usually happens.

## A real port

The first real task was a business-rules library ported from an Access database: nine modules (rounding, totals, defaults, quote-to-order, receipts, commission, RMA routing, qualified leads, a manufacturer request). Each was built by the tool from a short spec, with finished modules passed in as frozen code. Eight passed their own generated tests and a hand-written check with zero to three retries. The ninth did too, after a stalled server call and one redundant import removed by hand.

Merged into one crate, the **94 unit tests pass** and a hand-written test that reads the real CSV fixtures reproduces all three figures the task was judged on:

| Figure | Result |
|---|---|
| One rep, September 2026: commission and unpaid balance | $25,701.07 and $328,267.47 |
| One order's tax (net sales 12,098.00 at 0.0725) | $877.10 |
| All September 2026 commission | $183,280.48 over 136 orders, 82 with no payment |

The rounding is the hard part: Access rounds halves to even on binary floating point, so $877.105 becomes $877.10, and plain half-up rounding would give $877.11 and $25,701.08. The generated code gets it right, and the test would fail under half-up.

What the tool did not do: write the glue that loads the fixtures, the test that asserts the figures, a `FLAGS.md` listing the odd V1 behaviours it kept, or clean generated test code of 16 clippy complaints. Those were left, listed, for a finishing pass.

## Five runs of one spec

The commission module was run five times, with the tool changing in between.

{{< island name="RunsChart" data="runs" label="Retries and wall time for five runs of the same spec" >}}

Run A (79 retries) and run B (5) differ by one change: a repair that ran out of attempts used to leave the function as `todo!()`, wiping working code, and now keeps its best earlier body. But run C, with that fix, still took 45, and run D failed because of a bug in blame that only showed up there. The spread between runs is large, and one run cannot show that a change helped.

## Does thinking less help?

The server can be asked for less reasoning before each answer. On one trivial function it cut the answer from about 250 tokens to 78. On the commission spec, three runs each:

| | Passed | Wall time (min / median / max) | Median retries |
|---|---|---|---|
| Default reasoning | 2 of 3 | 394 / 581 / 913 s | 8 |
| Low reasoning | 0 of 3 | 269 / 719 / 844 s | 17 |

Fewer tokens per call, but more wrong answers and more calls. On this input it did not pay.

## Which model, how fast

{{< island name="ThroughputChart" data="throughput" label="Total tokens per second against simultaneous requests" >}}

- A single stream runs at about 54 tokens per second whatever the model, so concurrency is what raises throughput. Going from fewer slots to 8 raised the total at 8 requests by about 1.4 to 1.5 times, for both models.
- The storage format (a 4-bit MXFP4 file against a Q4_K_XL one) made no measurable difference.
- Coder-Next scaled better than gpt-oss-120b: 176 against 110 tokens per second in total at 8 requests. Both are mixture-of-experts models; Coder-Next activates fewer parameters per token (about 3 billion against about 5), which may be why, but that was not tested.
- A dense 27-billion-parameter model was about five times slower for a single stream.
- This is speed only. Whether the faster model writes correct code was not measured with the tool.

## What went wrong on the way

Each of these cost a run, and each is fixed:

- **String literals were corrupted** when generated code was re-indented. Test fixtures with embedded text broke silently.
- **A request with no timeout hung forever** when a laptop lid closed mid-run. It now has a 900-second limit, and the long runs moved to a server.
- **The fill prompt did not include the spec**, so the model ignored a stated rounding method and wrote the plausible one.
- **The skeleton step turned finished code into stubs** when asked to build on it. Finished code is now frozen and shown, not rewritten.
- **The model gamed a lint** by adding a function that referenced every private helper. The tool now removes dead helpers instead of arguing.
- **A repair that ran out of attempts left `todo!()`**, and blame charged correct functions for stubs elsewhere. Both are in the chart above.
- **Test results were read from text.** A test that printed a line shaped like the harness's own output was counted twice, and a test that crashed the process by overflowing its stack simply vanished from the results. The tool now reads the harness's structured output and counts a test that started but never finished as a failure.
- **Memory.** Eight slots of 120,000 tokens each on a 121 GB machine took it to 99 GB used, and it stopped answering ssh until it was power cycled. The cache for such settings is now worked out before starting, and a watchdog kills the model server if memory runs short.

## What is not proven

- That the skeleton review splits functions well, or that it does not over-split.
- That starting later stages early from a contract draft pays: it is built, but its first real run finished before any draft had settled.
- That a different model writes correct code with the tool, as opposed to fast code.
- Anything about languages other than Rust.
