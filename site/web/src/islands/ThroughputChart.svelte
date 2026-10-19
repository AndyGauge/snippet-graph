<script>
  let { data } = $props();
  let view = $state("chart");
  let hoverIndex = $state(null);
  let width = $state(640);

  const H = 310;
  const compact = $derived(width < 600);
  const M = $derived({ top: 34, right: compact ? 18 : 196, bottom: 40, left: 46 });
  const short = { gpt2: "120b, 2 slots", gpt8: "120b, 8 slots", cn8: "Coder-Next, 8 slots", q27: "Qwen3.8-27B" };

  const plotW = $derived(Math.max(200, width - M.left - M.right));
  const plotH = $derived(H - M.top - M.bottom);
  const top = $derived(Math.ceil(Math.max(...data.series.flatMap((s) => s.total.filter((v) => v != null))) / 50) * 50);
  const x = (i) => M.left + (i / (data.streams.length - 1)) * plotW;
  const y = (v) => M.top + plotH - (v / top) * plotH;
  const ticks = $derived([0, 1, 2, 3, 4].map((k) => (top / 4) * k));

  function colour(s) {
    return s.emphasis ? `var(--series-${s.slot})` : "var(--series-gray)";
  }
  function path(s) {
    let d = "";
    s.total.forEach((v, i) => {
      if (v == null) return;
      d += `${d ? "L" : "M"}${x(i)},${y(v)} `;
    });
    return d;
  }
  function lastIndex(s) {
    let k = -1;
    s.total.forEach((v, i) => { if (v != null) k = i; });
    return k;
  }
  function onMove(e) {
    const box = e.currentTarget.getBoundingClientRect();
    const px = ((e.clientX - box.left) / box.width) * width;
    const i = Math.round(((px - M.left) / plotW) * (data.streams.length - 1));
    hoverIndex = Math.max(0, Math.min(data.streams.length - 1, i));
  }
  const all = $derived([...data.series.map((s) => ({ name: s.name, total: s.total })), ...data.also]);
</script>

<div class="tc" bind:clientWidth={width}>
  <div class="tc-head">
    <div role="group" aria-label="View">
      <button class:on={view === "chart"} onclick={() => (view = "chart")}>Chart</button>
      <button class:on={view === "table"} onclick={() => (view = "table")}>Table</button>
    </div>
    <ul class="tc-legend">
      {#each data.series as s (s.id)}
        <li><svg width="22" height="10" aria-hidden="true"><line x1="0" y1="5" x2="22" y2="5" stroke={colour(s)} stroke-width="2" stroke-linecap="round" /></svg>{s.name}</li>
      {/each}
    </ul>
  </div>

  {#if view === "chart"}
    <svg viewBox={`0 0 ${width} ${H}`} width={width} height={H} role="img" aria-label={`Total tokens per second for ${data.series.length} setups at ${data.streams.join(", ")} simultaneous requests. Coder-Next with 8 slots reaches ${data.series.find((s) => s.id === "cn8").total[3]} at 8 requests.`} onpointermove={onMove} onpointerleave={() => (hoverIndex = null)}>
      {#each ticks as t}
        <line x1={M.left} x2={M.left + plotW} y1={y(t)} y2={y(t)} class="grid" />
        <text x={M.left - 8} y={y(t) + 4} class="tick" text-anchor="end">{t}</text>
      {/each}
      <line x1={M.left} x2={M.left + plotW} y1={y(0)} y2={y(0)} class="axis" />
      {#each data.streams as s, i}
        <text x={x(i)} y={H - 20} class="tick" text-anchor="middle">{s}</text>
      {/each}
      <text x={M.left + plotW / 2} y={H - 4} class="axislabel" text-anchor="middle">simultaneous requests</text>
      <text x="0" y="12" class="axislabel">tokens per second, all streams</text>

      {#if hoverIndex !== null}
        <line x1={x(hoverIndex)} x2={x(hoverIndex)} y1={M.top} y2={M.top + plotH} class="cross" />
      {/if}

      {#each [...data.series].sort((a, b) => Number(a.emphasis) - Number(b.emphasis)) as s (s.id)}
        <path d={path(s)} fill="none" stroke={colour(s)} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        {#each s.total as v, i}
          {#if v != null && (hoverIndex === i || i === lastIndex(s))}
            <circle cx={x(i)} cy={y(v)} r="4.5" fill={colour(s)} stroke="var(--surface-1)" stroke-width="2" />
          {/if}
        {/each}
        {@const k = lastIndex(s)}
        {#if !compact}
          <text x={x(k) + 10} y={y(s.total[k]) + 4} class="direct">{short[s.id]} · {s.total[k]}</text>
        {/if}
      {/each}
    </svg>

    {#if hoverIndex !== null}
      <div class="tc-tip" style={`left:${Math.min(x(hoverIndex) + 12, width - 230)}px`} role="status">
        <strong>{data.streams[hoverIndex]} simultaneous request{data.streams[hoverIndex] > 1 ? "s" : ""}</strong>
        {#each data.series as s (s.id)}
          {#if s.total[hoverIndex] != null}
            <div><svg width="14" height="8" aria-hidden="true"><line x1="0" y1="4" x2="14" y2="4" stroke={colour(s)} stroke-width="2" /></svg>{short[s.id]}: {s.total[hoverIndex]} total, {s.perStream[hoverIndex]} each</div>
          {/if}
        {/each}
      </div>
    {/if}
  {:else}
    <table>
      <thead><tr><th>Setup</th>{#each data.streams as s}<th class="n">{s}</th>{/each}</tr></thead>
      <tbody>
        {#each all as row}
          <tr><td>{row.name}</td>{#each row.total as v}<td class="n">{v ?? ""}</td>{/each}</tr>
        {/each}
      </tbody>
    </table>
  {/if}
  <p class="tc-note">{data.note}</p>
</div>

<style>
  .tc { position: relative; border: 1px solid var(--border); border-radius: 10px; padding: 1rem; background: var(--surface-1); }
  .tc-head { display: flex; justify-content: space-between; align-items: center; gap: 0.75rem; flex-wrap: wrap; margin-bottom: 0.5rem; }
  button { font: inherit; font-size: 0.85rem; padding: 0.25rem 0.8rem; border: 1px solid var(--axis); background: var(--surface-1); color: var(--text-primary); cursor: pointer; }
  button:first-child { border-radius: 6px 0 0 6px; }
  button:last-child { border-radius: 0 6px 6px 0; border-left: 0; }
  button.on { background: var(--text-primary); color: var(--surface-1); }
  button:focus-visible { outline: 2px solid var(--series-1); outline-offset: 2px; }
  .tc-legend { list-style: none; display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; margin: 0; padding: 0; font-size: 0.82rem; color: var(--text-secondary); }
  .tc-legend li { display: flex; align-items: center; gap: 0.4rem; }
  svg { max-width: 100%; touch-action: pan-y; }
  .grid { stroke: var(--grid); stroke-width: 1; }
  .axis { stroke: var(--axis); stroke-width: 1; }
  .cross { stroke: var(--axis); stroke-width: 1; }
  .tick { fill: var(--muted); font-size: 11.5px; }
  .axislabel { fill: var(--muted); font-size: 11.5px; }
  .direct { fill: var(--text-secondary); font-size: 12px; }
  .tc-tip { position: absolute; top: 3.2rem; background: var(--surface-1); border: 1px solid var(--axis); border-radius: 8px; padding: 0.5rem 0.7rem; font-size: 0.82rem; color: var(--text-secondary); box-shadow: 0 2px 10px rgba(0,0,0,0.12); pointer-events: none; max-width: 230px; }
  .tc-tip strong { color: var(--text-primary); display: block; margin-bottom: 0.2rem; }
  .tc-tip div { display: flex; align-items: center; gap: 0.35rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
  th, td { padding: 0.4rem 0.5rem; border-bottom: 1px solid var(--grid); text-align: left; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  .tc-note { color: var(--muted); font-size: 0.82rem; margin: 0.75rem 0 0; }
</style>
