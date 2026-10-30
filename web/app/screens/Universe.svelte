<script lang="ts">
  import { onMount } from 'svelte';
  import { errorMessage, getUniverse } from '../../shared/api';
  import { describeUniverse } from '../../shared/summary';
  import type { Planet, Universe } from '../../shared/types';
  import type { UniverseHandle } from '../../universe/render';

  let host = $state<HTMLElement>();
  let universe = $state.raw<Universe>();
  let picked = $state.raw<Planet>();
  let error = $state('');

  onMount(() => {
    let handle: UniverseHandle | undefined;
    let gone = false;
    (async () => {
      const u = await getUniverse('both');
      universe = u;
      if (!u.planets.length) return;
      // A dynamic import, so three.js only loads for someone who opens this screen.
      const { mountUniverse } = await import('../../universe/render');
      if (gone) return;
      try {
        handle = mountUniverse(host!, u);
      } catch {
        error = 'This computer could not start 3D drawing, so Rostery cannot show the Universe here.';
        return;
      }
      handle.onSelect((id) => (picked = u.planets.find((p) => p.id === id)));
    })().catch((e) => (error = errorMessage(e)));
    return () => {
      gone = true;
      handle?.destroy();
    };
  });
</script>

<div class="screen">
  <div class="sky" bind:this={host}></div>
  <header>
    <h1>Universe</h1>
    {#if universe?.planets.length}
      <p>{describeUniverse(universe)}</p>
    {/if}
  </header>
  {#if error}
    <p class="note" role="alert">{error}</p>
  {:else if universe?.planets.length === 0}
    <p class="note">Nothing to draw yet. Add your history on the Sources screen, or try the demo data there.</p>
  {:else if universe}
    <p class="note" role="status">
      {#if picked}
        {picked.name}, in the {universe.galaxies[picked.galaxy].name} Galaxy
      {:else}
        Drag to turn. Scroll to zoom. Click a Planet to see its name.
      {/if}
    </p>
  {/if}
</div>

<style>
  .screen {
    position: relative;
    height: 100%;
    overflow: hidden;
  }

  .sky {
    position: absolute;
    inset: 0;
  }

  /* Words sit over the picture, so they must not catch the drag that turns it, and a Planet can pass behind them. */
  header,
  .note {
    position: absolute;
    left: 48px;
    right: 48px;
    pointer-events: none;
    text-shadow: 0 0 6px var(--night), 0 0 2px var(--night);
  }

  header {
    top: 40px;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  header p {
    margin: 8px 0 0;
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .note {
    bottom: 28px;
    margin: 0;
    color: var(--dim);
    overflow-wrap: anywhere;
    /* A name comes from a file and can be any length, so this keeps it to 4 lines. */
    max-height: 6em;
    overflow: hidden;
  }

  @media (max-width: 720px) {
    header,
    .note {
      left: 24px;
      right: 24px;
    }
    header {
      top: 24px;
    }
  }
</style>
