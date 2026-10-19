---
title: "How the code graph is read"
lead: "The Code section is not hand-drawn. `sg graph` reads this repository the same way it reads a crate it is writing."
description: "What the code graph on this site is made of, and how far you can trust its edges."
---

## Is this AST-level parsing?

Yes, and no further. Each file is parsed with `syn` into a real syntax tree, with line numbers, so nodes are exact: every function, method, type, constant and test, with its signature, its doc comment and its source. Edges are found by walking the bodies in that tree. Nothing is type-checked and no compiler runs, so an edge is only as good as what the syntax alone can tell.

| Edge | How it is found | How far to trust it |
|---|---|---|
| Call by path, `plugin::serve(...)` | The path is resolved through the file's `use` lines | Exact |
| Call on `self`, `self.helper()` | Resolved to the enclosing type's methods | Exact |
| Call on any other value, `x.helper()` | The receiver's type is unknown, so it is matched by method name, to at most three candidates | Marked "possible" and drawn dashed |
| Type use | A type path in a signature or body | Exact |
| Constant use | An ALL_CAPS path | Exact |
| Call inside a macro, `assert_eq!(f(1), 2)` | Macro arguments are parsed as expressions where they can be | Best effort |

Common standard-library method names (`push`, `len`, `iter`) are skipped, so `Vec::push` is never mistaken for a method of this crate.

## Why do it this way

The same reading is what `sg` needs for a crate it is writing: a graph with a depth for each function (nothing it calls goes deeper), so leaves are filled first. The depth on every [Code](../code/) page comes from exactly that. A full type-checked graph would need a working build; this one is available for a crate that is still a skeleton with `todo!()` everywhere.

## Walk it

Start in the [explorer](../code/), or open a node such as {{< node "snippet-graph::plugin::serve" >}} and follow its links. Every function that calls another links to it; every function lists the tests that reach it.
