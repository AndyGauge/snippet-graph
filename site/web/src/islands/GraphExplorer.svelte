<script>
  import { index, neighbours, search, levelCounts, visit, firstLine } from "../lib/walk.js";

  let { data } = $props();

  let graph = $state(null);
  let failed = $state(false);
  let query = $state("");
  let module = $state("");
  let showTests = $state(false);
  let current = $state("");
  let trail = $state([]);

  const ix = $derived(graph ? index(graph) : null);
  const modules = $derived(graph ? [...new Set(graph.nodes.map((n) => n.module))].sort() : []);
  const kinds = $derived(
    new Set(showTests ? ["fn", "method", "struct", "enum", "trait", "const", "type_alias", "test"] : ["fn", "method", "struct", "enum", "trait", "const", "type_alias"]),
  );
  const found = $derived(ix ? search(ix, query, { kinds, module }) : { total: 0, nodes: [] });
  const node = $derived(ix && current ? ix.byId.get(current) : null);
  const near = $derived(node ? neighbours(ix, node.id) : null);
  const levels = $derived(graph ? levelCounts(graph) : []);
  const tallest = $derived(Math.max(1, ...levels));

  $effect(() => {
    fetch(data.src)
      .then((r) => r.json())
      .then((g) => {
        graph = g;
        fromHash();
      })
      .catch(() => (failed = true));
    window.addEventListener("hashchange", fromHash);
    return () => window.removeEventListener("hashchange", fromHash);
  });

  function fromHash() {
    const slug = decodeURIComponent(location.hash.slice(1));
    if (!slug || !graph) return;
    const hit = graph.nodes.find((n) => n.slug === slug);
    if (hit) go(hit.id, false);
  }

  function go(id, push = true) {
    current = id;
    trail = visit(trail, id);
    if (push) {
      const n = ix.byId.get(id);
      history.replaceState(null, "", `#${n.slug}`);
    }
  }

  function back() {
    if (trail.length < 2) return;
    go(trail[trail.length - 2]);
  }

  const W = 640;
  const ROW = 26;
  const layout = $derived.by(() => {
    if (!near) return null;
    const left = [...near.calledBy, ...near.tests.slice(0, 3)].slice(0, 10);
    const right = [
      ...near.calls.map((x) => ({ ...x, possible: false })),
      ...near.possible.map((x) => ({ ...x, possible: true })),
    ].slice(0, 10);
    const rows = Math.max(left.length, right.length, 1);
    return { left, right, h: rows * ROW + 24, cy: (rows * ROW + 24) / 2 };
  });

  function y(i, count, h) {
    return h / 2 + (i - (count - 1) / 2) * ROW;
  }

  function label(id) {
    return ix.byId.get(id)?.name ?? id;
  }

  function clip(s, n = 22) {
    return s.length > n ? `${s.slice(0, n - 1)}…` : s;
  }
</script>

{#if failed}
  <p>The graph could not be loaded. The <a href={data.pages}>index of pages</a> works without it.</p>
{:else if !graph}
  <p aria-live="polite">Loading the graph…</p>
{:else}
  <div class="ge">
    <div class="ge-find">
      <label for="ge-q">Find a function, type or constant</label>
      <input id="ge-q" type="search" bind:value={query} placeholder="e.g. blame, fill, Node" autocomplete="off" />
      <div class="ge-filters">
        <label class="inline">
          Module
          <select bind:value={module}>
            <option value="">all</option>
            {#each modules as m}<option value={m}>{m}</option>{/each}
          </select>
        </label>
        <label class="inline"><input type="checkbox" bind:checked={showTests} /> include tests</label>
      </div>
      <p class="muted" aria-live="polite">{found.total} match{found.total === 1 ? "" : "es"}{found.total > found.nodes.length ? `, showing ${found.nodes.length}` : ""}</p>
      <ul class="ge-results">
        {#each found.nodes as n (n.id)}
          <li>
            <button class:on={n.id === current} onclick={() => go(n.id)}>
              <code>{n.name}</code>
              <span class="muted">{n.kind} · {n.module}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>

    <div class="ge-view">
      {#if node && near && layout}
        <div class="ge-trail">
          <button onclick={back} disabled={trail.length < 2}>← Back</button>
          {#each trail.slice(0, -1) as t}
            <button class="crumb" onclick={() => go(t)}>{label(t)}</button>
          {/each}
        </div>
        <h3><code>{node.name}</code> <span class="muted">{node.kind} · {node.crate_name}::{node.module}</span></h3>
        {#if node.doc}<p class="ge-doc">{firstLine(node.doc)}</p>{/if}
        <pre class="ge-sig">{node.signature}</pre>

        <svg viewBox={`0 0 ${W} ${layout.h}`} role="img" aria-label={`${node.name} calls ${layout.right.length} and is called by ${layout.left.length}. The lists below say the same.`}>
          {#each layout.left as l, i}
            <line x1="200" y1={y(i, layout.left.length, layout.h)} x2="260" y2={layout.cy} class="edge" />
          {/each}
          {#each layout.right as r, i}
            <line x1="380" y1={layout.cy} x2="440" y2={y(i, layout.right.length, layout.h)} class="edge" class:possible={r.possible} />
          {/each}
          <rect x="260" y={layout.cy - 14} width="120" height="28" rx="6" class="self" />
          <text x="320" y={layout.cy + 4} text-anchor="middle" class="self-t">{clip(node.name, 16)}</text>
          {#each layout.left as l, i}
            <g role="button" tabindex="0" onclick={() => go(l.id)} onkeydown={(e) => e.key === "Enter" && go(l.id)} class="hit">
              <rect x="0" y={y(i, layout.left.length, layout.h) - 11} width="200" height="22" rx="4" class="chip" />
              <text x="192" y={y(i, layout.left.length, layout.h) + 4} text-anchor="end">{clip(label(l.id))}</text>
              <title>{l.id}</title>
            </g>
          {/each}
          {#each layout.right as r, i}
            <g role="button" tabindex="0" onclick={() => go(r.id)} onkeydown={(e) => e.key === "Enter" && go(r.id)} class="hit">
              <rect x="440" y={y(i, layout.right.length, layout.h) - 11} width="200" height="22" rx="4" class="chip" class:possible={r.possible} />
              <text x="448" y={y(i, layout.right.length, layout.h) + 4}>{clip(label(r.id))}</text>
              <title>{r.id}{r.possible ? " (matched by name only)" : ""}</title>
            </g>
          {/each}
        </svg>
        <p class="ge-key"><span class="solid"></span> resolved call <span class="dash"></span> matched by name only</p>

        <div class="ge-lists">
          {#each [["Calls", near.calls], ["May call", near.possible], ["Uses types", near.types], ["Uses constants", near.constants], ["Called by", near.calledBy], ["Tests", near.tests], ["Used as a type by", near.usedAsType]] as [title, items]}
            {#if items.length}
              <div>
                <h4>{title} <span class="muted">{items.length}</span></h4>
                <ul>
                  {#each items as it (it.id)}
                    <li><button class="link" onclick={() => go(it.id)}><code>{label(it.id)}</code></button></li>
                  {/each}
                </ul>
              </div>
            {/if}
          {/each}
        </div>
        <p><a href={`${data.pages}${node.slug}/`}>Page for this node, with its source</a></p>
      {:else}
        <p class="ge-empty">Pick something on the left. From there, click any neighbour to walk the graph one step at a time.</p>
        <h3>Depth: the order the functions would be filled in</h3>
        <p class="muted">Functions that call nothing sit at depth 0 and come first; a function sits one deeper than the deepest thing it calls.</p>
        <ol class="ge-levels" aria-label="Number of functions at each depth">
          {#each levels as c, i}
            <li>
              <span class="lvl">{i}</span>
              <span class="bar" style={`width:${(c / tallest) * 100}%`}></span>
              <span class="num">{c}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </div>
  </div>
{/if}

<style>
  .ge { display: grid; grid-template-columns: minmax(0, 18rem) minmax(0, 1fr); gap: 1.25rem; border: 1px solid var(--border); border-radius: 10px; padding: 1rem; background: var(--surface-1); }
  @media (max-width: 760px) { .ge { grid-template-columns: 1fr; } }
  label { display: block; font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 0.25rem; }
  label.inline { display: inline-flex; gap: 0.4rem; align-items: center; margin-right: 1rem; }
  input[type="search"], select { font: inherit; padding: 0.4rem 0.6rem; border-radius: 6px; border: 1px solid var(--axis); background: var(--surface-1); color: var(--text-primary); }
  input[type="search"] { width: 100%; box-sizing: border-box; }
  .ge-filters { margin-top: 0.5rem; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  ul { list-style: none; padding: 0; margin: 0; }
  .ge-results { max-height: 26rem; overflow: auto; border-top: 1px solid var(--grid); }
  .ge-results button { width: 100%; text-align: left; display: flex; flex-direction: column; border-radius: 0; border: 0; border-bottom: 1px solid var(--grid); padding: 0.4rem 0.5rem; }
  button { font: inherit; background: transparent; color: var(--text-primary); border: 1px solid var(--axis); border-radius: 999px; padding: 0.2rem 0.7rem; cursor: pointer; }
  button.on { background: var(--surface-2); }
  button:disabled { opacity: 0.4; cursor: default; }
  button.link { border: 0; padding: 0.1rem 0; text-decoration: underline; text-decoration-color: var(--axis); border-radius: 0; }
  button:focus-visible, input:focus-visible, select:focus-visible, g:focus-visible { outline: 2px solid var(--series-1); outline-offset: 2px; }
  .ge-trail { display: flex; gap: 0.4rem; flex-wrap: wrap; margin-bottom: 0.5rem; }
  .crumb { font-family: ui-monospace, Menlo, monospace; font-size: 0.8rem; }
  h3 { margin: 0.25rem 0; }
  h4 { margin: 0.75rem 0 0.25rem; font-size: 0.9rem; }
  .ge-doc { margin: 0.25rem 0; color: var(--text-secondary); }
  .ge-sig { background: var(--surface-2); padding: 0.5rem 0.75rem; border-radius: 6px; overflow: auto; font-size: 0.8rem; max-height: 8rem; }
  svg { width: 100%; height: auto; margin-top: 0.5rem; font-size: 12px; fill: var(--text-primary); }
  .edge { stroke: var(--axis); stroke-width: 1.5; }
  .edge.possible { stroke-dasharray: 4 3; }
  .chip { fill: var(--surface-2); stroke: var(--axis); }
  .chip.possible { stroke-dasharray: 4 3; }
  .self { fill: var(--series-1); }
  .self-t { fill: #fff; font-weight: 600; }
  .hit { cursor: pointer; }
  .hit:hover .chip { stroke: var(--series-1); }
  .ge-key { font-size: 0.8rem; color: var(--muted); }
  .solid, .dash { display: inline-block; width: 1.5rem; border-top: 2px solid var(--axis); vertical-align: middle; margin: 0 0.25rem 0 0.5rem; }
  .dash { border-top-style: dashed; }
  .ge-lists { display: grid; grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr)); gap: 0 1rem; }
  .ge-levels { list-style: none; padding: 0; display: grid; gap: 4px; }
  .ge-levels li { display: grid; grid-template-columns: 2rem 1fr 3rem; align-items: center; gap: 0.5rem; }
  .bar { display: block; height: 14px; background: var(--series-1); border-radius: 0 4px 4px 0; min-width: 2px; }
  .lvl, .num { font-variant-numeric: tabular-nums; font-size: 0.8rem; color: var(--text-secondary); }
  .ge-empty { color: var(--text-secondary); }
</style>
