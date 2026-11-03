<script lang="ts">
  // A page anyone can open, so it reaches no IPC and nothing in web/app but the Dial. Every string in
  // the payload was written by a stranger or a Provider, so it only ever goes in as text.
  import Dial from '../app/Dial.svelte';
  import type { SharePayload } from '../shared/types';
  import UniverseView from '../universe/UniverseView.svelte';

  let { data }: { data: SharePayload } = $props();

  const roast = $derived(data.roast);
  const universes = $derived(data.universes);
  const hasUniverse = $derived(Object.values(universes).some((u) => u && u.planets.length));
  // Rostery leaves titles out of the file when they were not chosen, so their presence is the choice.
  const showItems = $derived(Object.values(universes).some((u) => u?.planets.some((p) => p.topItems.length)));
  const day = $derived(
    new Date(data.generatedAt * 1000).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' }),
  );
</script>

<div class="page">
  <header>
    <p class="kicker" dir="auto">{roast ? `${data.displayName}'s Roast` : `${data.displayName}'s Universe`}</p>
    {#if roast}
      <h1 dir="auto">{roast.headline}</h1>
      <p class="opening" dir="auto">{roast.opening}</p>
    {:else}
      <h1 dir="auto">What {data.displayName} listens to and watches</h1>
    {/if}
  </header>

  {#if roast}
    <ol class="burns">
      {#each roast.burns as burn}
        <li>
          <h2 dir="auto">{burn.title}</h2>
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
  {/if}

  {#if hasUniverse}
    <!-- Room above and below on a phone, because a finger on the sky turns it rather than the page. -->
    <section class="universe" aria-label="Universe">
      <UniverseView {universes} {showItems}>
        <h2 class="universe-name">Universe</h2>
        <p class="hint">Each Planet is an artist or a channel, sized by time spent. Lines join the ones that go together.</p>
      </UniverseView>
    </section>
  {/if}

  <footer>
    {#each data.attribution as line}
      <p>{line}</p>
    {/each}
    <p>Made with Rostery on {day}. Generated on one person's machine.</p>
  </footer>
</div>

<style>
  .page {
    max-width: 980px;
    margin: 0 auto;
    padding: 64px 32px 48px;
    /* A Provider can send a long word with nowhere to break. */
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  .kicker {
    margin: 0;
    color: var(--brass);
    font: 600 13px/1.4 var(--sans);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    max-width: 18em;
    margin: 14px 0 0;
    font: 500 56px/1.04 var(--serif);
    letter-spacing: -0.015em;
    text-wrap: balance;
  }

  .opening {
    max-width: 34em;
    margin: 22px 0 0;
    font: 21px/1.55 var(--serif);
    white-space: pre-line;
  }

  .burns {
    margin: 56px 0 0;
    padding: 40px 0 0;
    border-top: 1px solid var(--rule);
    list-style: none;
    font-family: var(--serif);
  }

  .burns li + li {
    margin-top: 32px;
  }

  .burns h2 {
    margin: 0;
    color: var(--brass);
    font: 600 21px/1.25 var(--serif);
  }

  .burns p {
    max-width: 36em;
    margin: 6px 0 0;
    font-size: 18px;
    line-height: 1.6;
    white-space: pre-line;
  }

  .verdict {
    --ink: var(--brass);
    --dial-size: 190px;
    display: flex;
    align-items: center;
    gap: 36px;
    margin-top: 56px;
    padding: 32px;
    border: 1px solid var(--rule);
    border-radius: 10px;
    background: linear-gradient(160deg, var(--panel), var(--night));
  }

  .verdict :global(.dial) {
    flex: none;
    margin: 0;
  }

  .judgement h2 {
    margin: 0;
    color: var(--dim);
    font: 600 13px/1.4 var(--sans);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .archetype {
    margin: 6px 0 0;
    font: 500 34px/1.1 var(--serif);
  }

  .sentence {
    margin: 10px 0 0;
    font: 18px/1.5 var(--serif);
  }

  .judgement ul {
    margin: 14px 0 0;
    padding-left: 18px;
    color: var(--dim);
  }

  .universe {
    height: min(82vh, 820px);
    min-height: 460px;
    margin-top: 64px;
    overflow: hidden;
    border: 1px solid var(--rule);
    border-radius: 10px;
  }

  .universe-name {
    margin: 0;
    font: 500 30px/1.1 var(--serif);
  }

  .hint {
    max-width: 30em;
    margin: 6px 0 0;
    color: var(--dim);
  }

  footer {
    margin-top: 48px;
    padding-top: 20px;
    border-top: 1px solid var(--rule);
    color: var(--dim);
    font-size: 14px;
  }

  footer p {
    margin: 4px 0 0;
  }

  @media (max-width: 720px) {
    .page {
      padding: 36px 18px 32px;
    }

    h1 {
      font-size: 36px;
    }

    .opening {
      font-size: 18px;
    }

    .verdict {
      --dial-size: 140px;
      flex-direction: column;
      align-items: flex-start;
      gap: 20px;
      padding: 22px;
    }

    .archetype {
      font-size: 28px;
    }

    .universe {
      height: 70svh;
      min-height: 380px;
    }
  }
</style>
