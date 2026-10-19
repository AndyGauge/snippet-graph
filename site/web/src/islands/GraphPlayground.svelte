<script>
  import { NODES, levels, stepsFor } from "../lib/graph.js";

  let hard = $state(true);
  let index = $state(0);
  const steps = $derived(stepsFor(hard));
  const step = $derived(steps[Math.min(index, steps.length - 1)]);
  const L = levels(NODES);

  const BOX_W = 168;
  const BOX_H = 50;
  const COL_X = [8, 250, 492];
  const byLevel = [0, 1, 2].map((l) => NODES.filter((n) => L[n.id] === l));
  const GAP = 72;
  const place = {};
  byLevel[0].forEach((n, i) => (place[n.id] = { x: COL_X[0], y: 10 + i * GAP }));
  const mid = 10 + ((byLevel[0].length - 1) * GAP) / 2;
  byLevel[1].forEach((n) => (place[n.id] = { x: COL_X[1], y: mid }));
  byLevel[2].forEach((n) => (place[n.id] = { x: COL_X[2], y: mid }));
  const HEIGHT = 10 + (byLevel[0].length - 1) * GAP + BOX_H + 10;

  const LABEL = {
    none: ["", "not written yet"],
    todo: ["○", "todo!()"],
    red: ["✗", "test fails (as it should)"],
    passing: ["✓", "tests pass"],
    failing: ["✗", "tests fail"],
    retry: ["↻", "being retried"],
    spent: ["■", "budget spent"],
  };

  function label(id) {
    const state = step.states[id];
    if (state === "retry" && step.attempts) return ["↻", `retry, attempt ${step.attempts} of 10`];
    return LABEL[state];
  }

  function edges() {
    const out = [];
    for (const n of NODES) {
      for (const u of n.uses) {
        const a = place[n.id];
        const b = place[u];
        out.push({ key: `${n.id}-${u}`, x1: a.x, y1: a.y + BOX_H / 2, x2: b.x + BOX_W, y2: b.y + BOX_H / 2 });
      }
    }
    return out;
  }
  const lines = edges();

  function next() { index = Math.min(index + 1, steps.length - 1); }
  function back() { index = Math.max(index - 1, 0); }
  $effect(() => { hard; index = 0; });
</script>

<div class="pg">
  <div class="pg-controls">
    <button onclick={back} disabled={index === 0}>Back</button>
    <button onclick={next} disabled={index >= steps.length - 1}>Next</button>
    <span class="pg-count">Step {Math.min(index, steps.length - 1) + 1} of {steps.length}</span>
    <label class="pg-toggle">
      <input type="checkbox" bind:checked={hard} />
      month_summary is a hard function
    </label>
  </div>

  <p class="pg-title"><strong>{step.title}.</strong> <span aria-live="polite">{step.text}</span></p>

  <svg viewBox={`0 0 660 ${HEIGHT}`} role="img" aria-label="The six functions of the commission module and which ones use which, with their state at this step.">
    {#each lines as e (e.key)}
      <line x1={e.x1} y1={e.y1} x2={e.x2} y2={e.y2} class="edge" />
    {/each}
    {#each NODES as n (n.id)}
      {@const p = place[n.id]}
      {@const s = step.states[n.id]}
      {@const [icon, text] = label(n.id)}
      <g transform={`translate(${p.x} ${p.y})`}>
        <rect width={BOX_W} height={BOX_H} rx="6" class="node" />
        <rect width="5" height={BOX_H} rx="2" class={`bar ${s}`} />
        <text x="16" y="21" class="name">{n.id}</text>
        <text x="16" y="39" class="state">{icon} {text}</text>
      </g>
    {/each}
  </svg>

  {#if step.attempts}
    <div class="pg-budget">
      <span>month_summary attempts</span>
      <div class="meter" role="img" aria-label={`${step.attempts} of 10 attempts used`}>
        <div class="fill" style={`width:${step.attempts * 10}%`}></div>
      </div>
      <span>{step.attempts} of 10{step.rollbacks ? `, ${step.rollbacks} free rollbacks` : ""}</span>
    </div>
  {/if}
  <p class="pg-note">The graph is the commission module from a real run: arrows point from a function to the functions it uses.</p>
</div>

<style>
  .pg { border: 1px solid var(--border); border-radius: 10px; padding: 1rem; background: var(--surface-1); }
  .pg-controls { display: flex; flex-wrap: wrap; gap: 0.5rem 0.75rem; align-items: center; margin-bottom: 0.75rem; }
  .pg-count { color: var(--text-secondary); font-size: 0.9rem; }
  .pg-toggle { margin-left: auto; font-size: 0.9rem; color: var(--text-secondary); display: flex; gap: 0.4rem; align-items: center; }
  button { font: inherit; padding: 0.35rem 0.9rem; border-radius: 6px; border: 1px solid var(--axis); background: var(--surface-1); color: var(--text-primary); cursor: pointer; }
  button:disabled { opacity: 0.4; cursor: default; }
  button:focus-visible, input:focus-visible { outline: 2px solid var(--series-1); outline-offset: 2px; }
  .pg-title { margin: 0.25rem 0 0.75rem; min-height: 3.2em; }
  svg { width: 100%; height: auto; display: block; }
  .edge { stroke: var(--axis); stroke-width: 1; }
  .node { fill: var(--surface-2); stroke: var(--grid); }
  .name { fill: var(--text-primary); font-size: 14px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
  .state { fill: var(--text-secondary); font-size: 12.5px; }
  .bar { fill: var(--muted); }
  .bar.passing { fill: var(--good); }
  .bar.failing, .bar.spent { fill: var(--critical); }
  .bar.retry { fill: var(--serious); }
  .pg-budget { display: flex; align-items: center; gap: 0.75rem; font-size: 0.9rem; color: var(--text-secondary); margin-top: 0.5rem; flex-wrap: wrap; }
  .meter { flex: 1; min-width: 120px; max-width: 280px; height: 8px; border-radius: 4px; background: var(--grid); overflow: hidden; }
  .fill { height: 100%; background: var(--serious); }
  .pg-note { color: var(--muted); font-size: 0.82rem; margin: 0.75rem 0 0; }
</style>
