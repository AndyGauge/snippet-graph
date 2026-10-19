---
title: "Failures"
lead: "Blame picks who to retry, the ledger remembers what failed, and a budget decides when to stop."
description: "Blame, the failure ledger, last good versions, sweep, free rollbacks and the per-function attempt budget."
---

## Blame

When tests fail, the tool does not retry everything they touch. It ranks **functions and methods** (never types, never frozen prelude code) by how likely each is to be the cause.

- A function reached by passing tests as well as failing ones is **cleared**. Its own tests pass.
- A function reached only by failing tests is a suspect, more so when few tests reach it, and more so if it was just filled.
- If one suspect uses another, the one it uses is retried first, so the deepest failing helper is fixed before its callers are blamed.
- A test that fails by reaching a `todo!()` **blames nobody**. It says a function is unfilled, not that anything it touches is wrong, and unfilled functions are filled anyway.

That last rule came from a real run. A one-line `net_sales` was rewritten 15, 11 and 14 times across three runs because tests for `month_summary`, which was still a stub, passed through it. See [results](../results/#what-went-wrong-on-the-way).

## The failure ledger

Every distinct failure is a record, linked to the node versions it was seen with. It is not stored separately: it is **computed from the stored views and their test results**, so it cannot drift from the history.

- **Open** until a later view runs that test and it no longer fails. Then it is **resolved**, and the record keeps which new versions made the difference. If nothing changed, it says so, which points at a flaky test.
- It **reopens** if it comes back.
- Two failures are the same if they have the same test and the same text once digit runs are removed, so a different thread id or line number does not make a new failure.
- A failure whose test no longer exists is resolved.

`sg failures <store> [--node ID] [--open]` lists them.

## Last good, and sweep

`sg good <store> <node>` shows the newest version of a function that compiled, preferring the newest whose tests passed. A version with no body, or one that never compiled, is never offered.

`sg sweep <store> <view>` moves every function that has an open failure, or no body, back to its last good version, and makes a new view. At the end of a run this happens automatically, and the result is kept only if it does not increase the number of failing tests.

## Rollbacks are free

If a repair attempt returns exactly the body the model already tried for that function, nothing is run again: the earlier failure is reused, no retry is charged, and the next prompt says "that exact body was already tried and failed this way; write a different one". Twenty repeats in a row with nothing new between them count as a retry, so a stuck model cannot loop forever.

## The attempt budget

Each function gets **10 distinct attempts for the whole run**, counted across the first fill and every repair pass. A function that spends them is no longer chosen for repair, and the sweep gives it back its last good version, or `todo!()` if it never compiled. The report marks it "budget spent".

The budget exists because the alternative, a retry limit that resets with every pass, allowed about 36 attempts on one function in the worst case, and one real run spent 27 retries on a single function that was too large to get right.
