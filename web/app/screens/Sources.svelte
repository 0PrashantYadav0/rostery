<script lang="ts">
  import { onMount } from 'svelte';
  import { clearDemo, loadDemo, totals as fetchTotals } from '../../shared/api';
  import type { Totals } from '../../shared/types';

  const none = { plays: 0, minutes: 0, creators: 0, firstPlay: null, lastPlay: null };

  let totals = $state<Totals>({ spotify: none, youtube: none });
  let loaded = $state(false);
  let busy = $state(false);
  let error = $state('');

  async function run(action: () => Promise<Totals>) {
    busy = true;
    error = '';
    try {
      totals = await action();
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => run(fetchTotals));

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

  <div class="actions">
    <button class="primary" onclick={() => run(loadDemo)} disabled={busy}>Try demo data</button>
    {#if all > 0}
      <button onclick={() => run(clearDemo)} disabled={busy}>Clear demo data</button>
    {/if}
    {#if loaded && all === 0}
      <p>Nothing here yet. Demo data is made-up history, so you can look around first.</p>
    {/if}
  </div>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
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

  .error {
    margin: 16px 0 0;
    color: #f08a7e;
  }

  @media (prefers-reduced-motion: reduce) {
    .arc {
      transition: none;
    }
  }
</style>
