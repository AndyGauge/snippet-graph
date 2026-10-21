# Changelog

## Unreleased

## 0.2.0

- `sg schema <spec.md> <crate_dir>` writes a compiling schema (types, constants and signatures with `todo!()` bodies) and prints it with its `[dependencies]`. The model writes one draft; after that each compile error is traced to the item it sits in and only the failing items are rewritten, so one bad type no longer costs the ones that were right. The same builder is available as `snippet_graph::schema::build`.
- `sg schema <spec.md> <crate_dir> --from <schema> --change <request.md>` upgrades an existing schema instead of drafting a new one. The model restates only the items that are new or must change (and lists names to delete under `// === remove ===`); they are merged by name and the same per-item repair brings the result back to compiling. The diff (`+` added, `~` changed, `-` removed) is printed to stderr, so only those items need fresh tests and bodies. Library: `snippet_graph::schema::upgrade`, `schema::diff`.

## 0.1.1

- A test run that outlives 300 seconds (`SG_TEST_TIMEOUT` to change it) is killed along with its children, and the test that was running is reported as failed with a note that it is probably in an infinite loop. Before, one looping generated test stalled the whole run indefinitely.

- First standalone release, split from the queue project's workspace.
- Model access moved out of process: `sg` talks to `sg-provider-NAME` programs over JSON lines. `sg-provider-openai` is included.
- `sg graph` reads a crate's source into a graph of functions, types, constants and tests, with exact and name-matched edges.
- A site that explains the design and walks this repository's own graph.
- Earlier work: skeleton and tests first, blame and repair, content-addressed history, the failure ledger, skeleton review, structural checks and the handoff repository.
