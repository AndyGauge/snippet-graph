---
title: "Handoff"
lead: "The tool does not promise a finished crate. It promises an honest account of the unfinished one."
description: "The handoff repository and HANDOFF.md: status, depth, attempts, last error, and where to start."
---

The aim is a best-effort result handed to a person, or a stronger model, to finish. That only works if the receiver can tell, in a minute, what is solid, what is broken, and where to begin.

## The repository

Next to the crate, `sg` writes a git repository. There is **one commit per stored state**, from the skeleton to the final view, with a message that says how many tests were failing and which functions changed, with their attempt numbers. `git log` is the sequence of attempts, and `git show` on any commit is what the model produced then.

## HANDOFF.md

It opens with a count ("9 of 11 functions pass their tests"), then:

- **Start here**: functions that are not passing and whose own dependencies all are. Those are the ones whose fix is not blocked by something else.
- **Structure**: functions that look like they do too much, with the reason. Splitting one is likely to help more than another attempt.
- **All functions**, leaves first, each with:

| Column | Meaning |
|---|---|
| Status | passing; compiles, tests fail; does not compile; unfilled (`todo!()`); or unverified (the crate does not build, so nothing can be judged) |
| Depth | 0 if it uses no other function, otherwise one more than the deepest thing it uses |
| Attempts | stored versions, marked "budget spent" if it used all 10 |
| Last error | the first line of the compiler error or failing test |

- **Failures**: how many distinct failures were seen, how many are still open, and the open ones.
- **Skeleton review**: which functions were flagged and what was done.

## Why the order matters

Leaves first, and "start here" limited to unblocked functions, because a fix to a function at the bottom of the graph can make failures above it disappear. Starting at the top, where the symptoms are, is the common way to waste an afternoon.

## What it does not do

It does not say a passing function is correct, only that every test written for it passes. The tests are generated from the same spec by the same kind of model. A hand-written check, an "oracle", is the only independent evidence, and every module in the nine-module port was also run against one.
