# Snippet Graph: Incremental Code Generation

## Problem

One-shot generation asks a model to emit an entire file or module at once. That is not how people write code. A person sketches the shape, gets it compiling, fills in one piece, checks, and repeats. One-shot output skips that loop, so interface mismatches between pieces surface late, a single bad line invalidates a whole file, and the model works with far more context than any one piece needs.

## Proposal

Represent the program being generated as a graph of small snippets, and generate it one node at a time in dependency order.

### Nodes

A node is one small unit: a type, a function, or an impl block. Each node has:

- a signature (the public shape other nodes may rely on)
- a body (empty until filled in)
- a placement (file, module, position within the module)

### Edges

An edge means "uses". A node's generation context is the signatures of the nodes it points to, not whole files.

### Stages

1. **Skeleton.** Generate only types, function signatures, and module layout, with bodies as `todo!()`. In Rust this compiles, so the compiler validates the whole design before any logic exists.
2. **Skeleton review.** Check the signatures and edges for coherence. Errors here are the most expensive, because every dependent node inherits them.
3. **Fill.** Walk the graph in topological order and generate one body at a time, given only the signatures of its dependencies. Independent nodes can run in parallel.
4. **Verify per node.** After each fill, assemble the files and run `cargo check`. A failure is attributed to one node and only that node is retried. Tests and lints run once the relevant nodes are filled.
5. **Edit.** Changing existing code means locating the affected nodes, rewriting only those, and re-checking their dependents.

### Assembly

Files are rendered from the graph: nodes are grouped by placement and emitted in a stable order. The graph is the source of truth during generation; the files are a projection of it.

## Benefits

- Interface errors are caught at the skeleton, where they are cheapest to fix.
- Context per call is small and relevant.
- Failures are localised, so retries are targeted rather than whole-file.
- Independent nodes parallelise.
- Initial generation and later edits use the same mechanism.

## Risks and open questions

- **Wrong signatures cascade.** The skeleton needs its own review, and the fill stage needs a way to say "this signature must change". That sends the graph back for revision and invalidates the dependents of the changed node.
- **Granularity.** Too fine and the overhead and lack of local context hurt; too coarse and the one-shot problems return. Function-level is the likely starting point.
- **Edges the model can't see.** Trait impls, macros, and generics create dependencies that are not obvious from signatures alone. Edges may need to be derived from the compiler rather than declared by the model.
- **Placement conflicts.** Two nodes claiming the same position, or ordering that affects semantics, need deterministic resolution.

## Proposed first experiment

Run the planning-store self-rewrite through the skeleton-and-graph stages. It is bounded and already has a compile and test gate. Success means the skeleton compiles, every body fills with at most a small number of per-node retries, and the assembled result passes the existing tests.

## Status: what was built

The `snippet-graph` crate implements this design, extended with tests and versions.

- **Graph:** nodes are types, constants, functions, methods, tests and benchmarks, with "uses" edges, including method-call edges. A skeleton with `todo!()` bodies is checked by the compiler before any body is written.
- **Schema on its own:** `sg schema` (or `snippet_graph::schema::build`) writes just the skeleton and its `[dependencies]`. After one draft, each compile error is traced to the item it sits in and only the failing items are rewritten, with the rest of the schema shown for context; errors that fall outside every item ask for the file again. Callers that run their own pipeline use this for the type design and do the filling elsewhere.
- **Schema upgrade:** `sg schema --from SCHEMA --change REQUEST` (or `schema::upgrade`) applies a feature change to a schema that already exists. The model sees the whole current schema and restates only the items that are new or must change, plus a `remove` list; they are merged by node name and the normal per-item repair runs. The returned diff names the items added, changed and removed, which are the only ones that need new tests and bodies.
- **Tests before bodies:** each function gets small tests generated from the spec alone. A test must compile and fail against the empty skeleton. A function that keeps failing its tests gets its tests regenerated with the instruction to assert only what the spec states.
- **Fill and repair:** bodies are filled level by level in parallel. Failures are attributed to the deepest failing function and only that node is retried, with the earlier failure text in its prompt. Only tests that reach the changed node or its dependents are rerun.
- **Versions:** every attempt is stored as a content-addressed version. A run is a view mapping nodes to versions. `sg history`, `diff`, `undo`, `swap` and `export` work on a directory store; export writes a view to a crate and can commit it to git.
- **A/B testing:** `sg ab` generates a benchmark for a function if none exists, generates N variants in parallel, keeps those that pass the tests, benchmarks them against the baseline, and builds a new view with the winner after a combined check.
- **Edit mode:** `sg edit` rewrites chosen nodes of an existing view against its tests.

See `SNIPPET_GRAPH_PLAN.md` for the task breakdown and measured results.
