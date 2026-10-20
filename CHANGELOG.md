# Changelog

## Unreleased

## 0.1.1

- A test run that outlives 300 seconds (`SG_TEST_TIMEOUT` to change it) is killed along with its children, and the test that was running is reported as failed with a note that it is probably in an infinite loop. Before, one looping generated test stalled the whole run indefinitely.

- First standalone release, split from the queue project's workspace.
- Model access moved out of process: `sg` talks to `sg-provider-NAME` programs over JSON lines. `sg-provider-openai` is included.
- `sg graph` reads a crate's source into a graph of functions, types, constants and tests, with exact and name-matched edges.
- A site that explains the design and walks this repository's own graph.
- Earlier work: skeleton and tests first, blame and repair, content-addressed history, the failure ledger, skeleton review, structural checks and the handoff repository.
