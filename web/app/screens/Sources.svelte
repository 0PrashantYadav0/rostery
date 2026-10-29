<script lang="ts">
  import { onMount } from 'svelte';
  import {
    chooseExport,
    chooseFolder,
    clearDemo,
    hasDemoData,
    importPaths,
    loadDemo,
    onFileDrop,
    totals as fetchTotals,
  } from '../../shared/api';
  import { describeImport } from '../../shared/summary';
  import type { Totals } from '../../shared/types';

  const none = { plays: 0, minutes: 0, creators: 0, firstPlay: null, lastPlay: null };

  let totals = $state<Totals>({ spotify: none, youtube: none });
  let hasDemo = $state(false);
  let loaded = $state(false);
  let busy = $state(false);
  let importing = $state(false);
  let dragging = $state(false);
  let result = $state('');
  let missedDrop = $state(false);
  let error = $state('');
  let dropZone = $state<HTMLElement>();

  // The Rust side sends its message as a string. Anything else is a real Error.
  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function refresh() {
    [totals, hasDemo] = await Promise.all([fetchTotals(), hasDemoData()]);
    loaded = true;
  }

  /** Runs an action, then shows what is stored now, even when the action failed part way. */
  async function run(action: () => Promise<unknown>) {
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = message(e);
    }
    try {
      await refresh();
    } catch (e) {
      error ||= message(e);
    }
    busy = false;
  }

  async function runImport(paths: string[]) {
    result = '';
    missedDrop = false;
    importing = true;
    await run(async () => {
      result = describeImport(await importPaths(paths));
    });
    importing = false;
  }

  async function choose(pick: () => Promise<string[] | null>) {
    try {
      const picked = await pick();
      if (picked?.length) await runImport(picked);
    } catch (e) {
      error = message(e);
    }
  }

  onMount(() => {
    refresh().catch((e) => (error = message(e)));
    let unlisten: (() => void) | undefined;
    let gone = false;
    onFileDrop((event) => {
      dragging = !busy && (event.type === 'enter' || event.type === 'over');
      if (event.type !== 'drop') return;
      if (busy) missedDrop = true;
      else runImport(event.paths);
    })
      .then((stop) => (gone ? stop() : (unlisten = stop)))
      // Outside the desktop app there is nothing to drop onto. Choose file still works.
      .catch(() => {});
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  const status = $derived.by(() => {
    const line = importing ? 'Reading your history. A big file takes a few seconds.' : result;
    if (!missedDrop) return line;
    const note = busy
      ? 'That file was not read. Drop it again when this finishes.'
      : 'The file you dropped during that was not read. Drop it again.';
    return `${line} ${note}`.trim();
  });

  // The panel sits under the dials, so without this a message can land off screen.
  $effect(() => {
    if ([dragging, importing, status, error].some(Boolean)) dropZone?.scrollIntoView({ block: 'nearest' });
  });

  const all = $derived(totals.spotify.plays + totals.youtube.plays);
  // Each dial's arc is that Source's share of every Play stored.
  const dials = $derived(
    (['spotify', 'youtube'] as const).map((key) => ({
      key,
      name: key === 'spotify' ? 'Spotify' : 'YouTube',
      t: totals[key],
      share: all ? (totals[key].plays / all) * 100 : 0,
    })),
  );

  const count = (n: number) => n.toLocaleString();
  const day = (secs: number | null) =>
    secs === null
      ? 'none yet'
      : new Date(secs * 1000).toLocaleDateString(undefined, {
          year: 'numeric',
          month: 'short',
          day: 'numeric',
        });
</script>

<div class="screen">
  <header>
    <h1>Sources</h1>
    <p>Your Spotify and YouTube history, stored on this computer.</p>
  </header>

  <div class="dials">
    {#each dials as { key, name, t, share }}
      <section class={key} aria-labelledby="{key}-name">
        <div class="dial">
          <svg viewBox="0 0 200 200" aria-hidden="true">
            {#each { length: 60 } as _, i}
              <line
                x1="100"
                y1={i % 5 === 0 ? 6 : 10}
                x2="100"
                y2="15"
                transform="rotate({i * 6} 100 100)"
                class:major={i % 5 === 0}
              />
            {/each}
            <circle class="track" cx="100" cy="100" r="76" />
            <circle
              class="arc"
              cx="100"
              cy="100"
              r="76"
              pathLength="100"
              stroke-dasharray="{share} 100"
              transform="rotate(-90 100 100)"
            />
          </svg>
          <p class="reading">
            <span class="num">{count(t.plays)}</span>
            <span>plays</span>
          </p>
        </div>
        <h2 id="{key}-name">{name}</h2>
        <dl>
          {#if t.minutes > 0}
            <div><dt>Minutes listened</dt><dd>{count(t.minutes)}</dd></div>
          {/if}
          <div><dt>Creators</dt><dd>{count(t.creators)}</dd></div>
          <div><dt>First play</dt><dd>{day(t.firstPlay)}</dd></div>
          <div><dt>Last play</dt><dd>{day(t.lastPlay)}</dd></div>
        </dl>
      </section>
    {/each}
  </div>

  <div class="import" class:over={dragging}>
    <div class="drop" bind:this={dropZone}>
      <h2>Add your Spotify history</h2>
      <p>Drop the zip, a JSON file or a folder from Spotify here.</p>
      <div class="picks">
        <button class="primary" onclick={() => choose(chooseExport)} disabled={busy}>Choose file</button>
        <button onclick={() => choose(chooseFolder)} disabled={busy}>Choose folder</button>
      </div>
      <!-- Always mounted: a screen reader only announces a live region that was there before its text changed. -->
      <p role="status" class:result={!importing}>{status}</p>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
    </div>
    <div class="howto">
      <h3>Where to get it</h3>
      <ol>
        <li>In Spotify, go to Account, Privacy settings, Download your data.</li>
        <li>
          Request <b>Extended streaming history</b>. It has your full lifetime of listening. The
          regular Account data only covers about a year.
        </li>
        <li>Spotify emails a link when it is ready, which can take up to 30 days. Drop the zip here.</li>
      </ol>
    </div>
  </div>

  <div class="actions">
    <button class="primary" onclick={() => run(loadDemo)} disabled={busy}>Try demo data</button>
    {#if hasDemo}
      <button onclick={() => run(clearDemo)} disabled={busy}>Clear demo data</button>
    {/if}
    {#if loaded && all === 0}
      <p>Nothing here yet. Demo data is made-up history, so you can look around first.</p>
    {/if}
  </div>
</div>

<style>
  .screen {
    max-width: 880px;
    padding: 40px 48px 56px;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  header p {
    margin: 8px 0 0;
    color: var(--dim);
  }

  .dials {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 24px;
    margin-top: 36px;
  }

  section {
    padding: 24px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
  }

  .spotify {
    --ink: var(--brass);
  }

  .youtube {
    --ink: var(--sky);
  }

  .dial {
    position: relative;
    width: 220px;
    margin: 0 auto;
  }

  svg {
    display: block;
    width: 100%;
  }

  line {
    stroke: var(--rule);
    stroke-width: 1;
  }

  line.major {
    stroke: var(--faint);
    stroke-width: 1.5;
  }

  circle {
    fill: none;
    stroke-width: 7;
  }

  .track {
    stroke: var(--rule);
  }

  .arc {
    stroke: var(--ink);
    transition: stroke-dasharray 0.9s cubic-bezier(0.2, 0.7, 0.2, 1);
  }

  .reading {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    margin: 0;
    color: var(--dim);
    font-size: 13px;
    line-height: 1.2;
  }

  .num {
    color: var(--ivory);
    font: 500 40px/1.1 var(--serif);
    font-variant-numeric: tabular-nums;
  }

  h2 {
    margin: 20px 0 12px;
    color: var(--ink);
    font: 600 18px/1.2 var(--serif);
  }

  dl {
    margin: 0;
  }

  dl div {
    display: flex;
    justify-content: space-between;
    padding: 7px 0;
    border-top: 1px solid var(--rule);
  }

  dt {
    color: var(--dim);
  }

  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }

  .import {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 28px;
    margin-top: 24px;
    padding: 24px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
  }

  /* The dashed edge marks a place to drop things; it turns brass while a file is over it. */
  .drop {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    padding: 20px;
    border: 1px dashed var(--faint);
    border-radius: 6px;
    transition:
      border-color 0.15s,
      background-color 0.15s;
  }

  .over .drop {
    border-color: var(--brass);
    background: color-mix(in srgb, var(--brass) 9%, transparent);
  }

  .drop h2 {
    margin: 0;
    color: var(--ivory);
  }

  .drop p {
    margin: 0;
    color: var(--dim);
    overflow-wrap: anywhere;
  }

  .drop .result {
    color: var(--ivory);
  }

  .drop .error {
    color: #f08a7e;
  }

  /* Out of the flow while it has nothing to say, so it adds no gap, but still there to be announced. */
  [role='status']:empty {
    position: absolute;
  }

  .picks {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  h3 {
    margin: 0 0 10px;
    font: 600 18px/1.2 var(--serif);
  }

  ol {
    margin: 0;
    padding-left: 22px;
    color: var(--dim);
  }

  li {
    padding-left: 4px;
  }

  li + li {
    margin-top: 8px;
  }

  li::marker {
    color: var(--brass);
    font-variant-numeric: tabular-nums;
  }

  b {
    color: var(--ivory);
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin-top: 28px;
  }

  .actions p {
    margin: 0 0 0 4px;
    color: var(--dim);
  }

  button {
    padding: 9px 18px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: var(--faint);
  }

  button.primary {
    border-color: var(--brass);
    background: var(--brass);
    color: var(--night);
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  @media (prefers-reduced-motion: reduce) {
    .arc,
    .drop {
      transition: none;
    }
  }
</style>
