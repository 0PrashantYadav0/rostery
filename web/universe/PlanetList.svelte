<script lang="ts">
  import type { Planet, Universe } from '../shared/types';
  import PlanetGlyph from './PlanetGlyph.svelte';
  import { bySize, cssColor, formatWeight, galaxyColor } from './scene-data';

  let {
    universe,
    selectedId,
    onselect,
  }: { universe: Universe; selectedId: number | undefined; onselect: (planet: Planet) => void } = $props();

  const planets = $derived(bySize(universe.planets));
  const galaxy = (planet: Planet) => universe.galaxies[planet.galaxy];
</script>

<!-- role is not redundant here: Safari and a WebKit webview drop a list's meaning along with its bullets. -->
<ol class="list" role="list" aria-label="Planets, largest first">
  {#each planets as planet (planet.id)}
    <li>
      <!-- A WebKit webview skips a button on Tab unless it has a tabindex. -->
      <button
        tabindex="0"
        data-planet={planet.id}
        aria-current={planet.id === selectedId ? 'true' : undefined}
        onclick={() => onselect(planet)}
      >
        <PlanetGlyph source={planet.source} color={cssColor(galaxyColor(galaxy(planet)?.hue ?? 0))} />
        <span class="name" dir="auto">{planet.name}</span>
        <span class="weight">{formatWeight(planet)}</span>
        <span class="galaxy" dir="auto">{galaxy(planet)?.name ?? ''}</span>
      </button>
    </li>
  {/each}
</ol>

<style>
  .list {
    margin: 0;
    padding: 0 0 24px;
    list-style: none;
  }

  button {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) 7em minmax(0, 11em);
    align-items: baseline;
    gap: 12px;
    width: 100%;
    padding: 9px 33px;
    border: 0;
    border-bottom: 1px solid color-mix(in srgb, var(--rule) 55%, transparent);
    background: none;
    text-align: start;
    cursor: pointer;
  }

  button:hover {
    background: var(--panel);
  }

  button:focus-visible {
    outline-offset: -2px;
  }

  /* The same brass pointer as the nav rail, since this is the same idea: you are here. */
  button[aria-current='true'] {
    background: var(--panel);
    box-shadow: inset 3px 0 0 var(--brass);
  }

  /* A name comes from a file, so it can be any length and in any script. */
  .name,
  .galaxy {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
    unicode-bidi: isolate;
  }

  .name {
    font: 500 16px/1.3 var(--serif);
  }

  .weight {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
    text-align: end;
    white-space: nowrap;
  }

  .galaxy {
    color: var(--dim);
    font: italic 14px/1.3 var(--serif);
  }

  @media (max-width: 720px) {
    button {
      grid-template-columns: 22px minmax(0, 1fr) auto;
      padding: 9px 16px;
    }
    .galaxy {
      grid-column: 2 / -1;
    }
  }
</style>
