# snippet-graph site

A Hugo site with four small Svelte pieces: a step-through of the pipeline on a real graph, a checker that scores a function's contract the way the skeleton review does, and two charts.

```sh
./build.sh          # tests the logic, builds the Svelte pieces, then the site into public/
hugo server         # after one build: live preview of the pages
```

Needs Hugo (extended) and Node. `build.sh` runs `npm ci` the first time.

## Layout

| Path | What it is |
|---|---|
| `content/` | The pages, in Markdown |
| `layouts/`, `assets/css/site.css` | Page templates and the stylesheet, with light and dark colour sets |
| `data/runs.json`, `data/throughput.json` | The measurements. Both the charts and the plain tables read these, so a page still shows its numbers with JavaScript off |
| `web/src/islands/` | The Svelte components, mounted on any element with `data-island="Name"` |
| `web/src/lib/` | The logic the components use (graph levels, blame, the contract check) and `logic.test.js`, which `npm test` runs |
| `static/islands/` | Build output of `web/`; not committed |

## Keeping it honest

- The contract check in `web/src/lib/duties.js` is a port of `snippet-graph/src/review.rs`. If the thresholds change there, change them here; the test uses the real `month_summary` contract.
- Every figure on the pages comes from a run on the machine described on the Results page. Add a run to `data/runs.json` rather than rewording a sentence.
