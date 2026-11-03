<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { errorMessage, totals } from '../../shared/api';
  import Dial from '../Dial.svelte';
  import { roastRun as run } from '../roast-run.svelte';

  let {
    importing,
    missedError,
    stored,
    onsources,
  }: { importing: boolean; missedError: string; stored: number; onsources: () => void } = $props();

  let problem = $state('');
  let ask = $state<HTMLElement>();

  // The two are asked apart, because only history and where it would go decide the button. A last Roast
  // that cannot be read must not take away the way to write the one that replaces it. The last answer wins.
  let latest = 0;
  async function check() {
    const asked = (latest += 1);
    await Promise.all([
      run.load(),
      Promise.all([totals(), run.loadDisclosure()]).then(
        ([t]) => {
          if (asked !== latest) return;
          run.hasHistory = t.spotify.plays + t.youtube.plays > 0;
          // A failure to send what is no longer stored has nothing left to say.
          if (!run.hasHistory) run.error = '';
          problem = '';
        },
        (e) => {
          if (asked === latest) problem = errorMessage(e);
        },
      ),
    ]);
  }

  // Runs when this screen opens and each time Sources finishes changing what is stored, and for nothing else.
  $effect(() => {
    stored;
    untrack(check);
  });

  // One message, because a broken database fails both of these in the same words.
  const unread = $derived(problem || run.readError);
  const errors = $derived([missedError, run.error, unread].filter(Boolean));
  // The Roast waits under the button it would otherwise be pushed down by, so this says why nothing is here yet.
  const known = $derived(run.hasHistory !== undefined);
  const status = $derived(
    importing && !run.running
      ? 'Reading your history. This screen will update when that finishes.'
      : run.phrase || (known || unread ? '' : 'Looking at what is stored.'),
  );
  const shown = $derived(run.running ? run.sendingTo : run.disclosure);

  // Seen by being here. The rail only has to say so for someone who is elsewhere.
  $effect(() => {
    if (run.outcome) run.outcome = null;
  });

  async function retry() {
    await check();
    await tick();
    // Its button is gone when that worked, and focus would otherwise fall to the page.
    if (!unread) ask?.querySelector('button')?.focus();
  }

  const bringIntoView = (node: HTMLElement) => node.scrollIntoView({ block: 'nearest' });

  const day = (secs: number) =>
    new Date(secs * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
</script>

<div class="screen">
  <header>
    <h1>Roast</h1>
    <p>An AI reads a Digest of your history and tells you what it really thinks.</p>
  </header>

  <!-- Every button has a tabindex, because a WebKit webview skips one on Tab without it. -->
  {#if run.hasHistory === false && !importing}
    <div class="ask" bind:this={ask}>
      {#if run.latest}
        <p>Add your history on the Sources screen to write a new Roast.</p>
      {:else}
        <p>There is nothing to roast yet. Add your history on the Sources screen, or try the demo data there.</p>
      {/if}
      <button tabindex="0" onclick={onsources}>Go to Sources</button>
    </div>
  {:else if run.hasHistory && shown}
    <div class="ask" bind:this={ask}>
      <p class:flagged={shown.flagged}>{shown.sendsTo}</p>
      <!-- aria-disabled, because a disabled button drops keyboard focus to the page. start() checks too. -->
      <button tabindex="0" aria-disabled={run.running} onclick={() => run.start()}>
        {run.latest ? 'Roast me again' : 'Roast me'}
      </button>
    </div>
  {/if}

  <div class="progress" class:on={status}>
    {#if run.running}
      <Dial share={18} />
    {/if}
    <div>
      <!-- Always mounted: a screen reader only announces a live region that was there before its text changed. -->
      <p role="status" class="phrase">{status}</p>
      {#if run.running}
        <p class="hint">This can take a minute or two. You can look at other screens while it runs.</p>
      {/if}
    </div>
  </div>

  {#each errors as message}
    <!-- A person can be further down, reading the last Roast, when this arrives. -->
    <p class="error" role="alert" use:bringIntoView>{message}</p>
  {/each}
  {#if unread}
    <button class="retry" tabindex="0" onclick={retry}>Try again</button>
  {/if}
  {#if run.note}
    <p class="note">{run.note}</p>
  {/if}

  <!-- All of this was written by a Provider, so it only ever goes in as text. -->
  {#if run.latest && (known || unread)}
    {@const roast = run.latest.roast}
    <article>
      <h2 class="headline" dir="auto">{roast.headline}</h2>
      <p class="opening" dir="auto">{roast.opening}</p>
      <p class="byline">Written by {run.latest.providerLabel} on {day(run.latest.createdAt)}.</p>

      <ol class="burns">
        {#each roast.burns as burn}
          <li>
            <h3 dir="auto">{burn.title}</h3>
            <p dir="auto">{burn.body}</p>
          </li>
        {/each}
      </ol>

      <section class="verdict" aria-labelledby="verdict-name">
        <Dial share={roast.verdict.score} value={String(roast.verdict.score)} unit="out of 100" />
        <div class="judgement">
          <h2 id="verdict-name">Verdict</h2>
          <p class="archetype" dir="auto">{roast.verdict.archetype}</p>
          <p class="sentence" dir="auto">{roast.verdict.sentence}</p>
          {#if roast.verdict.evidence.length}
            <ul>
              {#each roast.verdict.evidence as exhibit}
                <li dir="auto">{exhibit}</li>
              {/each}
            </ul>
          {/if}
        </div>
      </section>
    </article>
  {/if}
</div>

<style>
  .screen {
    max-width: 880px;
    padding: 40px 48px 56px;
    /* A Provider can send a long word with nowhere to break. WebKit before Safari 15.4 only knows the older word-break. */
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  header p {
    margin: 8px 0 0;
    color: var(--dim);
  }

  .ask {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    margin-top: 24px;
  }

  .ask p {
    margin: 0;
  }

  /* Red and the full ink, because the person has to read this one before they press anything. */
  .ask .flagged {
    padding-left: 12px;
    border-left: 3px solid #f08a7e;
  }

  button {
    padding: 9px 18px;
    border: 1px solid var(--brass);
    border-radius: 6px;
    background: var(--brass);
    color: var(--night);
    font-weight: 600;
    cursor: pointer;
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  .progress {
    --dial-size: 64px;
    --ink: var(--brass);
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .progress.on {
    margin-top: 24px;
  }

  .progress :global(.dial) {
    flex: none;
    margin: 0;
  }

  /* The lit part goes round and round: a reading is being taken. The dash and its gap add up to what the sweep moves, so the loop has no seam. */
  .progress :global(.arc) {
    stroke-dasharray: 18 82;
    animation: sweep 2.4s linear infinite;
  }

  @keyframes sweep {
    to {
      stroke-dashoffset: -100;
    }
  }

  .phrase {
    margin: 0;
    font: 500 20px/1.3 var(--serif);
  }

  /* Out of the flow while it has nothing to say, so it adds no gap, but still there to be announced. */
  .phrase:empty {
    position: absolute;
  }

  .hint {
    margin: 2px 0 0;
    color: var(--dim);
    font-size: 14px;
  }

  .error {
    margin: 20px 0 0;
    color: #f08a7e;
  }

  .note {
    margin: 20px 0 0;
    color: var(--dim);
  }

  .retry {
    margin-top: 12px;
    background: none;
    color: var(--ivory);
  }

  /* What a Provider wrote is set in the serif, to read as something said about you. The app's own words stay in the sans. */
  article {
    margin-top: 40px;
    padding-top: 36px;
    border-top: 1px solid var(--rule);
    font-family: var(--serif);
  }

  .headline {
    margin: 0;
    max-width: 20em;
    font: 500 46px/1.08 var(--serif);
    letter-spacing: -0.01em;
    text-wrap: balance;
  }

  .opening {
    max-width: 34em;
    margin: 18px 0 0;
    font-size: 20px;
    line-height: 1.55;
    white-space: pre-line;
  }

  .byline {
    margin: 14px 0 0;
    color: var(--dim);
    font: 13px/1.4 var(--sans);
  }

  .burns {
    margin: 40px 0 0;
    padding: 0;
    list-style: none;
  }

  .burns li + li {
    margin-top: 30px;
  }

  h3 {
    margin: 0;
    color: var(--brass);
    font: 600 20px/1.25 var(--serif);
  }

  .burns p {
    max-width: 36em;
    margin: 6px 0 0;
    font-size: 17px;
    line-height: 1.6;
    white-space: pre-line;
  }

  .verdict {
    --ink: var(--brass);
    --dial-size: 190px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 28px 36px;
    margin-top: 48px;
    padding: 28px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
    font-family: var(--sans);
  }

  /* The reading draws in once when the Verdict appears. This is the one flourish on the screen. */
  .verdict :global(.arc) {
    animation: draw 0.9s cubic-bezier(0.2, 0.7, 0.2, 1);
  }

  @keyframes draw {
    from {
      stroke-dasharray: 0 100;
    }
  }

  /* It wraps under the dial once a line of it would get too short to read. */
  .judgement {
    flex: 1 1 300px;
    min-width: 0;
  }

  .verdict h2 {
    margin: 0;
    color: var(--dim);
    font: 500 15px/1.3 var(--sans);
  }

  .archetype {
    margin: 6px 0 0;
    color: var(--brass);
    font: 500 32px/1.15 var(--serif);
  }

  .sentence {
    margin: 12px 0 0;
    font: 18px/1.5 var(--serif);
  }

  /* WebKit takes an `li`'s alignment from the direction of its parent, so right to left text would sit at the left. */
  li {
    text-align: start;
  }

  ul {
    margin: 16px 0 0;
    padding: 12px 0 0;
    border-top: 1px solid var(--rule);
    color: var(--dim);
    font-size: 14px;
    list-style: none;
  }

  ul li + li {
    margin-top: 4px;
  }

  @media (max-width: 720px) {
    .screen {
      padding: 24px;
    }
    .headline {
      font-size: 34px;
    }
    .opening {
      font-size: 18px;
    }
    .verdict {
      padding: 20px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress :global(.arc),
    .verdict :global(.arc) {
      animation: none;
    }
  }
</style>
