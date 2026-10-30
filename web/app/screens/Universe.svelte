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
      const { mountUniverse, NoWebGL } = await import('../../universe/render');
      if (gone) return;
      try {
        handle = mountUniverse(host!, u);
      } catch (e) {
        console.error(e);
        error =
          e instanceof NoWebGL
            ? 'This computer could not start 3D drawing, so Rostery cannot show the Universe here. Update your graphics driver or restart this computer, then open this screen again.'
            : 'Rostery could not draw your Universe. Close Rostery, open it again, then come back to this screen.';
        return;
      }
      handle.onSelect((id) => (picked = u.planets.find((p) => p.id === id)));
      handle.onContextChange((lost) => {
        error = lost
          ? 'Rostery lost its 3D drawing, which can happen when a graphics driver restarts. If the picture does not come back by itself, open another screen and come back to this one.'
          : '';
      });
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

  /*
   * Words sit over the picture, so they must not catch the drag that turns it. A Planet can pass behind them in
   * any colour, so they have a solid backing. The left and top offsets line the words up with the other screens.
   */
  header,
  .note {
    position: absolute;
    left: 33px;
    max-width: calc(100% - 66px);
    padding: 10px 14px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
    overflow-wrap: anywhere;
    pointer-events: none;
  }

  header {
    top: 25px;
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
    bottom: 13px;
    margin: 0;
    color: var(--dim);
    /* A name comes from a file and can be any length, so this keeps it to 4 lines. */
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    overflow: hidden;
  }

  [role='alert'] {
    color: #f08a7e;
  }

  @media (max-width: 720px) {
    header,
    .note {
      left: 9px;
      max-width: calc(100% - 18px);
    }
    header {
      top: 9px;
    }
  }
</style>
