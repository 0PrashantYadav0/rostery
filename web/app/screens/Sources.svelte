<script lang="ts">
  import { onMount } from 'svelte';
  import {
    chooseExport,
    chooseFolder,
    clearDemo,
    errorMessage,
    hasDemoData,
    importPaths,
    loadDemo,
    onFileDrop,
    totals as fetchTotals,
  } from '../../shared/api';
  import { describeImport } from '../../shared/summary';
  import type { Totals } from '../../shared/types';
  import Dial from '../Dial.svelte';

  let {
    active,
    importing = $bindable(false),
    onopen,
    onstored,
    onmissederror,
  }: {
    active: boolean;
    importing?: boolean;
    /** A file was dropped on another screen, and its progress and result show here. */
    onopen: () => void;
    /** An action has finished, so what is stored may have changed. */
    onstored: () => void;
    /** An import failed after the person left, so the alert on this screen is out of their sight. */
    onmissederror: (message: string) => void;
  } = $props();

  const none = { plays: 0, minutes: 0, creators: 0, firstPlay: null, lastPlay: null };

  let totals = $state<Totals>({ spotify: none, youtube: none });
  let hasDemo = $state(false);
  let loaded = $state(false);
  let busy = $state(false);
  let dragging = $state(false);
  let result = $state('');
  let missedDrop = $state(false);
  let error = $state('');
  let dropZone = $state<HTMLElement>();
  let tryDemo = $state<HTMLElement>();

  async function refresh() {
    [totals, hasDemo] = await Promise.all([fetchTotals(), hasDemoData()]);
    loaded = true;
  }

  /** Runs an action, then shows what is stored now, even when the action failed part way. */
  async function run(action: () => Promise<unknown>) {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = errorMessage(e);
    }
    try {
      await refresh();
    } catch (e) {
      error ||= errorMessage(e);
    }
    busy = false;
    onstored();
  }

  async function runImport(paths: string[]) {
    if (busy) {
      missedDrop = true;
      return;
    }
    result = '';
    missedDrop = false;
    importing = true;
    await run(async () => {
      result = describeImport(await importPaths(paths));
    });
    importing = false;
    if (error && !active) onmissederror(error);
  }

  async function choose(pick: () => Promise<string[] | null>) {
    if (busy) return;
    try {
      const picked = await pick();
      if (picked?.length) await runImport(picked);
    } catch (e) {
      error = errorMessage(e);
    }
  }

  async function clearDemoData() {
    await run(clearDemo);
    // Its button is gone now, and focus would otherwise fall to the page.
    if (!hasDemo) tryDemo?.focus();
  }

  onMount(() => {
    refresh().catch((e) => (error = errorMessage(e)));
    let unlisten: (() => void) | undefined;
    let gone = false;
    onFileDrop((event) => {
      // On another screen there is no drop zone to show.
      dragging = active && !busy && (event.type === 'enter' || event.type === 'over');
      if (event.type === 'drop') {
        onopen();
        runImport(event.paths);
      }
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

  // The panel sits under the dials, so without this a message can land off screen. That includes one
  // that came in on another screen.
  $effect(() => {
    if (active && [dragging, importing, status, error].some(Boolean)) dropZone?.scrollIntoView({ block: 'nearest' });
  });

  const all = $derived(totals.spotify.plays + totals.youtube.plays);
  // Rust refuses demo data once there is real history, so there is nothing here to offer.
  const realHistoryOnly = $derived(loaded && all > 0 && !hasDemo);
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

<div class="screen" hidden={!active}>
  <header>
    <h1>Sources</h1>
    <p>Your Spotify and YouTube history, stored on this computer.</p>
  </header>

  <!-- Above the dials, because they are what this button changes, and they have to be on screen when it does. A WebKit webview skips a button on Tab without a tabindex. -->
  {#if !realHistoryOnly}
    <div class="actions">
      <button class:primary={loaded && all === 0} tabindex="0" bind:this={tryDemo} onclick={() => run(loadDemo)} aria-disabled={busy}>Try demo data</button>
      {#if hasDemo}
        <button tabindex="0" onclick={clearDemoData} aria-disabled={busy}>Clear demo data</button>
      {/if}
      {#if loaded && all === 0}
        <p>Nothing here yet. Demo data is made-up history, so you can look around first.</p>
      {/if}
    </div>
  {/if}

  <div class="dials">
    {#each dials as { key, name, t, share }}
      <section class={key} aria-labelledby="{key}-name">
        <Dial {share} value={count(t.plays)} unit="plays" />
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
      <h2>Add your history</h2>
      <p>Drop the zip, a JSON file or a folder from Spotify or Google Takeout here.</p>
      <div class="picks">
        <!-- aria-disabled, because a disabled button drops keyboard focus to the page. Each handler checks busy. -->
        <button class="primary" tabindex="0" onclick={() => choose(chooseExport)} aria-disabled={busy}>Choose file</button>
        <button tabindex="0" onclick={() => choose(chooseFolder)} aria-disabled={busy}>Choose folder</button>
      </div>
      <!-- Always mounted: a screen reader only announces a live region that was there before its text changed. -->
      <p role="status" class:result={!importing}>{status}</p>
      {#if error}
        <p class="error" role="alert">{error}</p>
      {/if}
    </div>
    <div class="howto spotify">
      <h3>Get your Spotify Export</h3>
      <ol>
        <li>In Spotify, go to Account, Privacy settings, Download your data.</li>
        <li>
          Request <b>Extended streaming history</b>. It has your full lifetime of listening. The
          regular Account data only covers about a year.
        </li>
        <li>Spotify emails a link when it is ready, which can take up to 30 days. Drop the zip here.</li>
      </ol>
    </div>
    <div class="howto youtube">
      <h3>Get your YouTube Export</h3>
      <ol>
        <li>Go to takeout.google.com, click <b>Deselect all</b>, then tick <b>YouTube and YouTube Music</b>.</li>
        <li>
          Takeout gives you history as HTML unless you change it, and Rostery cannot read HTML. Click
          <b>Multiple formats</b> and set history to <b>JSON</b>.
        </li>
        <li>Create the Export. Google emails you a link when it is ready. Drop the zip here.</li>
      </ol>
      <p>YouTube does not let apps read your watch history through its API, so this Export is the only way to get it.</p>
    </div>
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
    grid-template-columns: repeat(auto-fit, minmax(min(300px, 100%), 1fr));
    gap: 24px;
    margin-top: 28px;
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
    grid-template-columns: repeat(auto-fit, minmax(min(280px, 100%), 1fr));
    gap: 28px;
    margin-top: 24px;
    padding: 24px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
  }

  /* The dashed edge marks a place to drop things; it turns brass while a file is over it. */
  .drop {
    grid-column: 1 / -1;
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
    /* A file path has few places to break. WebKit before Safari 15.4 only knows the older word-break. */
    word-break: break-word;
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

  /* Each how-to sits under its Source's dial and takes that dial's colour. */
  h3 {
    margin: 0 0 10px;
    color: var(--ink);
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
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .howto p {
    margin: 14px 0 0;
    padding-top: 12px;
    border-top: 1px solid var(--rule);
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
    margin-top: 20px;
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

  button:hover:not([aria-disabled='true']) {
    border-color: var(--faint);
  }

  button.primary {
    border-color: var(--brass);
    background: var(--brass);
    color: var(--night);
    font-weight: 600;
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  @media (max-width: 720px) {
    .screen {
      padding: 24px;
    }
    .dials {
      gap: 16px;
    }
    .import {
      padding: 16px;
    }
    section {
      padding: 16px;
    }
    .drop {
      padding: 16px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .drop {
      transition: none;
    }
  }
</style>
