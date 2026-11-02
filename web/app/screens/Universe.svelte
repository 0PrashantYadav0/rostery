<script lang="ts">
  import { errorMessage, getUniverse, openSourceLink } from '../../shared/api';
  import type { Universe, UniverseFilter } from '../../shared/types';
  import UniverseView from '../../universe/UniverseView.svelte';
  import { roastRun as run } from '../roast-run.svelte';

  let { importing, missedError, stored }: { importing: boolean; missedError: string; stored: number } = $props();

  const FILTERS: UniverseFilter[] = ['both', 'spotify', 'youtube'];
  const NO_BROWSER = 'Rostery could not open your browser. Check that you have a default browser set, then try again.';

  let universes = $state.raw<Partial<Record<UniverseFilter, Universe>>>();
  let error = $state('');

  // Runs when this screen opens, each time Sources finishes changing what is stored, and when a Roast
  // starts or ends, because a Roast brings new Interests and Galaxy names even if it fails after the tags.
  // The last answer wins, so a slow one from before an import cannot replace the one from after it.
  let latest = 0;
  $effect(() => {
    stored;
    run.running;
    const asked = (latest += 1);
    Promise.all(FILTERS.map(getUniverse))
      .then((answers) => {
        if (asked !== latest) return;
        universes = Object.fromEntries(FILTERS.map((filter, i) => [filter, answers[i]]));
        error = '';
      })
      .catch((e) => {
        if (asked === latest) error = errorMessage(e);
      });
  });

  const hasPlanets = $derived(Object.values(universes ?? {}).some((u) => u.planets.length));
  const status = $derived.by(() => {
    if (importing) return 'Reading your history. The Universe will update when that finishes.';
    if (universes && !hasPlanets) return 'Nothing to draw yet. Add your history on the Sources screen, or try the demo data there.';
    return '';
  });

  function openLink(url: string) {
    error = '';
    openSourceLink(url).catch((e) => {
      // The opener's own words are about scopes and system calls, so they go to the log.
      console.error(e);
      error = NO_BROWSER;
    });
  }
</script>

{#snippet heading()}
  <h1>Universe</h1>
  <p class="status" role="status">{status}</p>
  {#if missedError}
    <p role="alert">{missedError}</p>
  {/if}
  {#if error}
    <p role="alert">{error}</p>
  {/if}
{/snippet}

<div class="screen">
  {#if universes && hasPlanets}
    <UniverseView {universes} showItems {openLink}>
      {@render heading()}
    </UniverseView>
  {:else}
    <header>
      {@render heading()}
    </header>
  {/if}
</div>

<style>
  .screen {
    height: 100%;
    overflow: hidden;
  }

  header {
    padding: 35px 47px;
    overflow-wrap: anywhere;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  p {
    margin: 8px 0 0;
    color: var(--dim);
    /* A name or a path can come from a file, so these keep their box. */
    overflow-wrap: anywhere;
  }

  .status:empty {
    margin: 0;
  }

  [role='alert'] {
    color: #f08a7e;
  }

  @media (max-width: 720px) {
    header {
      padding: 18px;
    }
    h1 {
      font-size: 26px;
    }
  }
</style>
