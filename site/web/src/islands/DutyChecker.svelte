<script>
  import { analyse, EXAMPLES, LIMITS } from "../lib/duties.js";

  let choice = $state("month_summary");
  let text = $state(EXAMPLES.month_summary);
  const result = $derived(analyse(text));

  function pick(name) {
    choice = name;
    text = EXAMPLES[name];
  }
</script>

<div class="dc">
  <div class="dc-picks" role="group" aria-label="Example contracts">
    {#each Object.keys(EXAMPLES) as name}
      <button class:on={choice === name} onclick={() => pick(name)}>{name}</button>
    {/each}
  </div>
  <label class="dc-label" for="dc-text">A function's doc comment and signature (edit it)</label>
  <textarea id="dc-text" bind:value={text} spellcheck="false" rows="12" oninput={() => (choice = "")}></textarea>

  <div class="dc-result" aria-live="polite">
    <p class="verdict">
      {#if result.flagged}
        <span class="badge bad">⚠ Looks like more than one job</span>
      {:else}
        <span class="badge ok">✓ Looks like one job</span>
      {/if}
    </p>
    <dl>
      <div><dt>Listed results</dt><dd>{result.bullets}</dd></div>
      <div><dt>Other duties</dt><dd>{result.found.length ? result.found.join(", ") : "none"}</dd></div>
      <div><dt>Duties in all</dt><dd>{result.duties} (limit {LIMITS.duties})</dd></div>
      <div><dt>Words</dt><dd>{result.words}</dd></div>
      <div><dt>Parameters</dt><dd>{result.params} (limit {LIMITS.params})</dd></div>
    </dl>
    {#if result.reasons.length}
      <ul class="reasons">{#each result.reasons as r}<li>{r}</li>{/each}</ul>
    {/if}
  </div>
  <p class="dc-note">Mirrors the check in <code>review.rs</code>: a function is flagged for more than {LIMITS.duties} duties, or {LIMITS.longWords}+ words with {LIMITS.longDuties}+ duties, or more than {LIMITS.params} parameters. Bullets under <code># Arguments</code> are not duties.</p>
</div>

<style>
  .dc { border: 1px solid var(--border); border-radius: 10px; padding: 1rem; background: var(--surface-1); }
  .dc-picks { display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.75rem; }
  button { font: inherit; padding: 0.3rem 0.8rem; border-radius: 999px; border: 1px solid var(--axis); background: var(--surface-1); color: var(--text-primary); cursor: pointer; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.85rem; }
  button.on { background: var(--text-primary); color: var(--surface-1); }
  button:focus-visible, textarea:focus-visible { outline: 2px solid var(--series-1); outline-offset: 2px; }
  .dc-label { display: block; font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 0.25rem; }
  textarea { width: 100%; box-sizing: border-box; font: 0.82rem/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; padding: 0.6rem; border: 1px solid var(--axis); border-radius: 6px; background: var(--surface-2); color: var(--text-primary); resize: vertical; }
  .dc-result { margin-top: 0.75rem; }
  .verdict { margin: 0 0 0.5rem; }
  .badge { display: inline-block; padding: 0.2rem 0.6rem; border-radius: 6px; font-weight: 600; border: 1px solid; }
  .badge.bad { color: var(--critical-text); border-color: var(--critical); }
  .badge.ok { color: var(--good-text); border-color: var(--good); }
  dl { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 0.5rem 1rem; margin: 0; }
  dt { font-size: 0.78rem; color: var(--muted); }
  dd { margin: 0; font-variant-numeric: tabular-nums; }
  .reasons { margin: 0.6rem 0 0; padding-left: 1.2rem; color: var(--text-secondary); }
  .dc-note { color: var(--muted); font-size: 0.82rem; margin: 0.75rem 0 0; }
</style>
