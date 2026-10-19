<script>
  let { data } = $props();
  let view = $state("chart");
  let hover = $state(null);
  let width = $state(640);

  const rows = $derived(data.runs);
  const panels = $derived([
    { key: "retries", title: "Retries", unit: "", max: Math.max(...rows.map((r) => r.retries)) },
    { key: "minutes", title: "Wall time", unit: " min", max: Math.max(...rows.map((r) => r.minutes)) },
  ]);

  const ROW = 38;
  const BAR = 18;
  const TOP = 8;
  const LEFT = 64;
  const height = $derived(TOP + rows.length * ROW + 6);

  function nice(max) {
    const step = max > 50 ? 20 : max > 20 ? 10 : 5;
    return Math.ceil(max / step) * step;
  }
  function barPath(x, y, w, h, r = 4) {
    const rr = Math.min(r, w / 2, h / 2);
    return `M${x},${y} H${x + w - rr} a${rr},${rr} 0 0 1 ${rr},${rr} V${y + h - rr} a${rr},${rr} 0 0 1 ${-rr},${rr} H${x} Z`;
  }
  const fmt = (v, key) => (key === "minutes" ? v.toFixed(1) : String(v));
</script>

<div class="rc" bind:clientWidth={width}>
  <div class="rc-head">
    <div role="group" aria-label="View">
      <button class:on={view === "chart"} onclick={() => (view = "chart")}>Chart</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    <span class="rc-legend"><span class="ok" aria-hidden="true">✓</span> passed &nbsp; <span class="bad" aria-hidden="true">✗</span> failed</span>
  </div>

  {#if view === "chart"}
    <div class="rc-panels" style={`--cols:${width > 560 ? 2 : 1}`}>
      {#each panels as p (p.key)}
        {@const top = nice(p.max)}
        {@const plotW = Math.max(120, (width > 560 ? width / 2 : width) - LEFT - 90)}
        <figure>
          <figcaption>{p.title}</figcaption>
          <svg width="100%" viewBox={`0 0 ${LEFT + plotW + 70} ${height}`} role="img" aria-label={`${p.title} for each run: ${rows.map((r) => `${r.id} ${fmt(r[p.key], p.key)}${p.unit}`).join(", ")}`}>
            {#each [0, top / 2, top] as t}
              <line x1={LEFT + (t / top) * plotW} x2={LEFT + (t / top) * plotW} y1={TOP} y2={height - 6} class="grid" />
              <text x={LEFT + (t / top) * plotW} y={height + 2} class="tick" text-anchor="middle">{t}</text>
            {/each}
            {#each rows as r, i (r.id)}
              {@const w = Math.max(2, (r[p.key] / top) * plotW)}
              {@const y = TOP + i * ROW + (ROW - BAR) / 2}
              <g onpointerenter={() => (hover = { r, key: p.key })} onpointerleave={() => (hover = null)} role="presentation">
                <rect x="0" y={TOP + i * ROW} width={LEFT + plotW + 70} height={ROW} fill="transparent" />
                <text x="0" y={y + 13} class="rowlabel"><tspan class={r.result === "passed" ? "ok" : "bad"}>{r.result === "passed" ? "✓" : "✗"}</tspan> Run {r.id}</text>
                <path d={barPath(LEFT, y, w, BAR)} class="bar" />
                <text x={LEFT + w + 8} y={y + 13} class="value">{fmt(r[p.key], p.key)}{p.unit}</text>
              </g>
            {/each}
          </svg>
        </figure>
      {/each}
    </div>
    <div class="rc-tip" aria-live="polite">
      {#if hover}
        <strong>Run {hover.r.id}, {hover.r.result}.</strong> {hover.r.build}. {hover.r.retries} retries, {hover.r.minutes} min. {hover.r.detail}.
      {:else}
        Hover a bar for the build and what happened.
      {/if}
    </div>
  {:else}
    <table>
      <thead><tr><th>Run</th><th>Tool build</th><th>Result</th><th class="n">Retries</th><th class="n">Minutes</th><th>What happened</th></tr></thead>
      <tbody>
        {#each rows as r}
          <tr><td>{r.id}</td><td>{r.build}</td><td>{r.result}</td><td class="n">{r.retries}</td><td class="n">{r.minutes.toFixed(1)}</td><td>{r.detail}</td></tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <p class="rc-note">{data.note}</p>
</div>

<style>
  .rc { border: 1px solid var(--border); border-radius: 10px; padding: 1rem; background: var(--surface-1); }
  .rc-head { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem; flex-wrap: wrap; gap: 0.5rem; }
  button { font: inherit; font-size: 0.85rem; padding: 0.25rem 0.8rem; border: 1px solid var(--axis); background: var(--surface-1); color: var(--text-primary); cursor: pointer; }
  button:first-child { border-radius: 6px 0 0 6px; }
  button:last-child { border-radius: 0 6px 6px 0; border-left: 0; }
  button.on { background: var(--text-primary); color: var(--surface-1); }
  button:focus-visible { outline: 2px solid var(--series-1); outline-offset: 2px; }
  .rc-legend { font-size: 0.85rem; color: var(--text-secondary); }
  .ok { color: var(--good-text); font-weight: 700; }
  .bad { color: var(--critical-text); font-weight: 700; }
  .rc-panels { display: grid; grid-template-columns: repeat(var(--cols), 1fr); gap: 1rem; }
  figure { margin: 0; }
  figcaption { font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 0.15rem; }
  svg { overflow: visible; display: block; margin-bottom: 1rem; }
  .grid { stroke: var(--grid); stroke-width: 1; }
  .tick { fill: var(--muted); font-size: 11px; }
  .rowlabel { fill: var(--text-secondary); font-size: 12.5px; }
  .value { fill: var(--text-primary); font-size: 12.5px; font-variant-numeric: tabular-nums; }
  .bar { fill: var(--series-1); }
  g:hover .bar { opacity: 0.8; }
  .rc-tip { min-height: 2.6em; font-size: 0.88rem; color: var(--text-secondary); margin-top: 0.25rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
  th, td { text-align: left; padding: 0.4rem 0.5rem; border-bottom: 1px solid var(--grid); vertical-align: top; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  .rc-note { color: var(--muted); font-size: 0.82rem; margin: 0.75rem 0 0; }
</style>
