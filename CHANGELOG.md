# Changelog

## Unreleased

- First standalone release, split from the queue project's workspace.
- Model access moved out of process: `sg` talks to `sg-provider-NAME` programs over JSON lines. `sg-provider-openai` is included.
- `sg graph` reads a crate's source into a graph of functions, types, constants and tests, with exact and name-matched edges.
- A site that explains the design and walks this repository's own graph.
- Earlier work: skeleton and tests first, blame and repair, content-addressed history, the failure ledger, skeleton review, structural checks and the handoff repository.
