---
title: "Review"
lead: "Some functions are too big for the model to get right. It is cheaper to notice that before writing them."
description: "The skeleton review, the measured size checks after filling, and a checker you can try."
---

## What went wrong

In three runs of the same spec, one function, `month_summary`, accounted for most of the retries. It filtered, ordered, summed four totals and counted unpaid orders in a single body. The tool had even measured it as too large, and kept retrying it. Nothing in the pipeline treated "this function does too much" as a problem. It only saw "this attempt failed".

## Earlier is cheaper

The first place the problem is visible is the **contract** the skeleton writes, before any test or body exists. The test count was not a good signal: `month_summary` had four tests, under the limit of five. The contract was: one filter, an ordering rule, four aggregates and an exclusion in a single function.

Try the check below. It is a port of the one the tool runs, with the same thresholds, and the tests that pin it down use the real contract from that run.

{{< island name="DutyChecker" label="Check whether a contract describes one job" >}}

## What the review does

After the skeleton compiles, every function and method that is not frozen prelude code is checked:

- more than **three duties** (listed results, plus any of: an ordering rule, an exclusion, a filter, an empty case), or
- **60 or more words** with **two or more** duties, or
- more than **five parameters**.

Bullets under `# Arguments` and similar headings are not counted, because a list of parameters is not a list of duties.

A flagged function sends the skeleton back to the model with the reasons, to be rewritten with each step moved into its own small private helper. The rewrite has to compile, has to keep every existing item with the same signature, and has to add something. If it does not, it gets one retry with the reason stated, and then the earlier skeleton is kept. Up to two rounds run. Nothing has been built on the old shape yet, so a split costs one model call and no rework. `sg --no-review` turns it off.

On the contracts from three earlier runs the check scored `month_summary` 3 to 7 duties, `rep_statement` 4, and every other function 0. Whether the model's splits are good has not been measured. Whether it over-splits something like `rep_statement` is an open question.

## After filling

Three more signals are measured on the body that was written:

- **Size and shape**: more than 30 lines of code, complexity above 8 (branches, match arms beyond the first, loops, `&&` and `||`), or nesting deeper than 3.
- **The model's own note**: it may start a body with `// TODO: does too much: ...`. Ignored for a body of 8 lines or fewer, whatever it says.
- **Test load**: more than five tests *written for* the function (tests of its callers also touch it and say nothing about its size).

A function flagged this way gets a restructure instruction on its next attempt, and appears in the handoff report. Splitting a function into separate nodes after it already has callers and tests is not built.
