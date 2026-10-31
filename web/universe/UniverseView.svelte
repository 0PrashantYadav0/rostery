<script lang="ts">
  // What both the app and the Share Site mount. A Share Site has no IPC, so everything comes through props,
  // and nothing here may reach Tauri or web/app. guard.test.ts checks that.
  import { onMount, tick, untrack, type Snippet } from 'svelte';
  import { describeUniverse } from '../shared/summary';
  import type { Planet, Universe, UniverseFilter } from '../shared/types';
  import PlanetGlyph from './PlanetGlyph.svelte';
  import PlanetList from './PlanetList.svelte';
  import PlanetPanel from './PlanetPanel.svelte';
  import type { UniverseHandle } from './render';
  import { availableFilters, bySize, cssColor, defaultFilter, galaxyColor } from './scene-data';

  /** More Galaxies than this is a list, not a legend, and the list view has them all. */
  const LEGEND_GALAXIES = 6;
  const FILTER_NAMES: Record<UniverseFilter, string> = { spotify: 'Spotify', youtube: 'YouTube', both: 'Both' };
  const NO_WEBGL = 'This computer could not start 3D drawing, so here is the list of Planets instead. Updating your graphics driver or restarting the computer may bring the picture back.';
  const NO_RENDERER = 'The 3D view could not load, so here is the list of Planets instead. Leave this page and come back to try again.';
  const DRAW_FAILED = 'Rostery could not draw this Universe, so here is the list of Planets instead.';
  const CONTEXT_LOST = 'Rostery lost its 3D drawing, which can happen when a graphics driver restarts. If the picture does not come back by itself, use the list, or leave this page and come back.';

  let {
    universes,
    showItems,
    openLink,
    children,
  }: {
    universes: Partial<Record<UniverseFilter, Universe>>;
    showItems: boolean;
    /** How to leave for a Source. With none, a link is just a link. */
    openLink?: (url: string) => void;
    /** Goes above the Planet count: a heading, or anything else a page wants to say there. */
    children?: Snippet;
  } = $props();

  let host = $state<HTMLElement>();
  let root = $state<HTMLElement>();
  let handle = $state.raw<UniverseHandle>();
  let picked = $state<UniverseFilter>();
  // A Planet's id can change with new data, so it is remembered by who it is.
  let selectedKey = $state<string>();
  let listView = $state(false);
  let noSky = $state('');
  let contextLost = $state(false);

  const filters = $derived(availableFilters(universes));
  const filter = $derived(picked && filters.includes(picked) ? picked : defaultFilter(filters));
  const universe = $derived(filter && universes[filter]);
  const keyOf = (planet: Planet) => `${planet.source} ${planet.name}`;
  const selected = $derived(universe?.planets.find((p) => keyOf(p) === selectedKey));
  const showList = $derived(listView || Boolean(noSky));
  const notice = $derived(noSky || (contextLost ? CONTEXT_LOST : ''));
  const legend = $derived(universe ? [...universe.galaxies].sort((a, b) => b.size - a.size || a.id - b.id) : []);
  const sources = $derived(new Set(universe?.planets.map((p) => p.source)));
  /** A small window keeps the legend folded, so it does not cover the sky. */
  const roomy = matchMedia('(min-width: 721px) and (min-height: 640px)').matches;

  // Drawn already, so an update with nothing new does not repaint or move the camera.
  let drawn: Universe | undefined;
  let drawnFilter: UniverseFilter | undefined;

  function giveUpOnSky(message: string) {
    noSky = message;
    handle?.destroy();
    handle = undefined;
  }

  onMount(() => {
    let gone = false;
    // A dynamic import, so three.js only loads for someone who opens the Universe.
    import('./render')
      .then(({ mountUniverse, NoWebGL }) => {
        if (gone || !universe) return;
        try {
          const mounted = mountUniverse(host!, universe);
          drawn = universe;
          drawnFilter = filter;
          mounted.onSelect((id) => {
            const planet = universe?.planets[id];
            if (planet) choose(planet);
          });
          mounted.onContextChange((lost) => (contextLost = lost));
          if (selected) {
            mounted.select(selected.id);
            mounted.flyTo(selected.id);
          }
          handle = mounted;
        } catch (e) {
          console.error(e);
          noSky = e instanceof NoWebGL ? NO_WEBGL : DRAW_FAILED;
        }
      })
      .catch((e) => {
        console.error(e);
        if (!gone) noSky = NO_RENDERER;
      });
    return () => {
      gone = true;
      handle?.destroy();
    };
  });

  // New data keeps the camera. A new filter frames the new set of Planets.
  $effect(() => {
    const next = universe;
    const nextFilter = filter;
    if (!handle || !next || next === drawn) return;
    untrack(() => {
      try {
        handle!.setUniverse(next, { frame: nextFilter !== drawnFilter });
        drawn = next;
        drawnFilter = nextFilter;
        if (selected) {
          handle!.select(selected.id);
          handle!.flyTo(selected.id);
        } else if (selectedKey) {
          // The new data does not have this Planet any more.
          close();
        }
      } catch (e) {
        console.error(e);
        giveUpOnSky(DRAW_FAILED);
      }
    });
  });

  function choose(planet: Planet) {
    selectedKey = keyOf(planet);
    handle?.select(planet.id);
    handle?.flyTo(planet.id);
  }

  /** Back to the list item for this Planet, or to the sky. */
  async function close() {
    const id = selected?.id;
    selectedKey = undefined;
    handle?.select(null);
    handle?.frameAll();
    await tick();
    const target = showList
      ? (root?.querySelector<HTMLElement>(`[data-planet="${id}"]`) ?? root?.querySelector<HTMLElement>('[data-planet]'))
      : host?.querySelector('canvas');
    target?.focus();
  }

  function pick(next: UniverseFilter) {
    if (next === filter) return;
    picked = next;
    selectedKey = undefined;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && selected) close();
  }}
/>

<div class="view" class:listing={showList} bind:this={root}>
  <div class="layout">
    <div class="stage">
      <div class="bar">
        <div class="title" data-covers-sky>
          {@render children?.()}
          {#if universe}
            <p class="count">{describeUniverse(universe)}</p>
          {:else}
            <p class="count">There are no Planets to show.</p>
          {/if}
        </div>
        {#if universe}
          <div class="controls" data-covers-sky>
            <!-- A WebKit webview skips a button on Tab unless it has a tabindex. -->
            <div class="tabs" role="group" aria-label="Which Source to show">
              {#each filters as option (option)}
                <button tabindex="0" aria-pressed={option === filter} onclick={() => pick(option)}>{FILTER_NAMES[option]}</button>
              {/each}
            </div>
            {#if !noSky}
              <button class="toggle" tabindex="0" onclick={() => (listView = !listView)}>{listView ? 'Show 3D view' : 'Show list'}</button>
            {/if}
          </div>
        {/if}
        {#if notice}
          <p class="notice" role="alert" data-covers-sky>{notice}</p>
        {/if}
      </div>

      <div class="sky" bind:this={host}></div>
      {#if showList && universe}
        <div class="scroll">
          <PlanetList {universe} selectedId={selected?.id} onselect={choose} />
        </div>
      {/if}

      {#if universe && !showList && !selected}
        <details class="legend" open={roomy} data-covers-sky>
          <summary>Galaxies</summary>
          <ul>
            {#each legend.slice(0, LEGEND_GALAXIES) as galaxy (galaxy.id)}
              <li>
                <span class="swatch" style:background={cssColor(galaxyColor(galaxy.hue))}></span>
                <span class="name" dir="auto">{galaxy.name}</span>
                <span class="size">{galaxy.size}</span>
              </li>
            {/each}
          </ul>
          {#if legend.length > LEGEND_GALAXIES}
            <p class="more">and {(legend.length - LEGEND_GALAXIES).toLocaleString('en')} more in the list</p>
          {/if}
          {#if sources.size > 1}
            <p class="key">
              <span><PlanetGlyph source="spotify" color="var(--dim)" /> Banded is Spotify</span>
              <span><PlanetGlyph source="youtube" color="var(--dim)" /> Ringed is YouTube</span>
            </p>
          {/if}
        </details>
      {/if}
    </div>

    {#if selected && universe}
      <div class="side">
        <PlanetPanel planet={selected} {universe} {showItems} {openLink} onclose={close} onselect={choose} />
      </div>
    {/if}
  </div>
</div>

<style>
  /* The tokens and the reset come from the page, as both the app and the Share Site load them. */
  .view {
    container-type: size;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .layout {
    display: flex;
    height: 100%;
  }

  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* The panel sits beside the sky, not over it, so a label cannot end up behind or on top of it. */
  .side {
    flex: none;
    width: min(360px, 42cqw);
    min-height: 0;
  }

  .sky {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--night);
  }

  .sky :global(canvas:focus-visible) {
    outline-offset: -3px;
  }

  .listing .sky {
    display: none;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .bar {
    position: absolute;
    inset: 25px 25px auto 33px;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: flex-start;
    gap: 10px;
    z-index: 1;
    /* A drag that starts between the boxes still turns the sky. */
    pointer-events: none;
  }

  .listing .bar {
    position: static;
    padding: 25px 25px 18px 33px;
    border-bottom: 1px solid var(--rule);
  }

  /*
   * Words sit over the picture and a Planet can pass behind them in any colour, so they have a solid backing.
   * The left and top offsets line the words up with the other screens.
   */
  .title,
  .controls,
  .notice,
  .legend {
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
  }

  .title {
    max-width: min(100%, 46ch);
    padding: 10px 14px;
    overflow-wrap: anywhere;
  }

  .count {
    margin: 8px 0 0;
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .controls {
    display: flex;
    pointer-events: auto;
  }

  .tabs {
    display: flex;
  }

  .controls button {
    position: relative;
    padding: 8px 14px;
    border: 0;
    background: none;
    color: var(--dim);
    white-space: nowrap;
    cursor: pointer;
  }

  .controls button:hover {
    color: var(--ivory);
  }

  .controls button:focus-visible {
    outline-offset: -2px;
  }

  .tabs button[aria-pressed='true'] {
    color: var(--ivory);
  }

  /* The nav rail's brass pointer, turned on its side. */
  .tabs button[aria-pressed='true']::after {
    content: '';
    position: absolute;
    inset: auto 14px 0 14px;
    height: 3px;
    background: var(--brass);
  }

  .controls .toggle {
    border-inline-start: 1px solid var(--rule);
    color: var(--ivory);
  }

  .notice {
    flex: 1 0 100%;
    margin: 0;
    padding: 10px 14px;
    color: #f08a7e;
    pointer-events: auto;
  }

  .sky :global(.labels) {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .sky :global(.label) {
    position: absolute;
    top: 0;
    left: 0;
    max-width: min(210px, 46cqw);
    padding: 1px 7px 2px;
    border-radius: 3px;
    /* Solid enough to read over the brightest Planet, and it lets the sky through. */
    background: rgb(12 16 25 / 0.74);
    color: var(--ivory);
    font: italic 14px/1.3 var(--serif);
    will-change: transform;
  }

  /* One line, whatever the name holds. The list and panel have the rest of it. */
  .sky :global(.label span) {
    display: block;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    unicode-bidi: plaintext;
  }

  .sky :global(.label.chosen) {
    color: var(--brass);
  }

  .sky :global(.label.tooltip) {
    padding: 5px 10px 6px;
    background: var(--panel);
    box-shadow: 0 0 0 1px var(--rule);
    font-size: 15px;
  }

  .sky :global(.label .detail) {
    margin-top: 1px;
    color: var(--dim);
    font: 13px/1.3 var(--sans);
  }

  .legend {
    position: absolute;
    left: 33px;
    bottom: 22px;
    width: min(240px, 60cqw);
    padding: 9px 14px;
  }

  summary {
    color: var(--ivory);
    font: italic 16px/1.3 var(--serif);
    cursor: pointer;
  }

  .legend ul {
    margin: 7px 0 0;
    padding: 0;
    list-style: none;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 1px 0;
    font-size: 14px;
  }

  .swatch {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
  }

  .legend .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    unicode-bidi: plaintext;
  }

  .size,
  .more {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .more {
    margin: 4px 0 0;
    font-size: 13px;
  }

  .key {
    display: grid;
    gap: 2px;
    margin: 9px 0 0;
    padding-top: 8px;
    border-top: 1px solid var(--rule);
    color: var(--dim);
    font-size: 13px;
  }

  .key span {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  /* A narrow window has room for the panel only under the sky. */
  @container (max-width: 720px) {
    .layout {
      flex-direction: column;
    }
    .side {
      width: 100%;
      height: min(62cqh, 420px);
    }
    .side :global(.panel) {
      border-inline-start: 0;
      border-top: 1px solid var(--rule);
    }
    .bar {
      inset: 9px 9px auto 9px;
    }
    .listing .bar {
      padding: 9px;
    }
    .count {
      margin-top: 2px;
      font-size: 14px;
    }
    .legend {
      left: 9px;
      bottom: 9px;
    }
  }
</style>
