<script lang="ts">
  import type { Planet, Universe } from '../shared/types';
  import PlanetGlyph from './PlanetGlyph.svelte';
  import { cssColor, formatWeight, galaxyColor, neighbours, sourceLinkLabel, sourceUrl } from './scene-data';

  /** Linked Planets past this many are still in the sky, and a panel has to end somewhere. */
  const SHOWN_LINKS = 6;

  let {
    planet,
    universe,
    showItems,
    openLink,
    onclose,
    onselect,
  }: {
    planet: Planet;
    universe: Universe;
    showItems: boolean;
    /** How to leave for a Source. With none, as on a Share Site, a link is just a link. */
    openLink?: (url: string) => void;
    onclose: () => void;
    onselect: (planet: Planet) => void;
  } = $props();

  const galaxy = $derived(universe.galaxies[planet.galaxy]);
  const linked = $derived(neighbours(universe, planet.id).slice(0, SHOWN_LINKS));
  const home = $derived(sourceUrl(planet.url));
  const items = $derived(showItems ? planet.topItems : []);
  const who = $derived(`${planet.source} ${planet.name}`);

  let heading = $state<HTMLElement>();
  // The panel's content changes without a page load, so a keyboard or screen reader needs to be told. Only
  // for a different Planet, because new data for this one should not pull focus away from where it is.
  $effect(() => {
    who;
    heading?.focus({ preventScroll: true });
  });

  function follow(e: MouseEvent, url: string) {
    if (!openLink) return;
    e.preventDefault();
    openLink(url);
  }
</script>

{#snippet sourceLink(url: string, text: string, className: string)}
  <a class={className} href={url} tabindex="0" target="_blank" rel="noopener noreferrer" dir="auto" onclick={(e) => follow(e, url)}>{text}</a>
{/snippet}

<aside class="panel" aria-labelledby="planet-panel-name">
  <header>
    <h2 id="planet-panel-name" dir="auto" tabindex="-1" bind:this={heading}>{planet.name}</h2>
    <!-- After the heading, so it is the first stop from there. A WebKit webview skips a button on Tab without tabindex. -->
    <button class="close" tabindex="0" aria-label="Close Planet details" onclick={onclose}>
      <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
        <path d="M1 1l8 8M9 1L1 9" stroke="currentColor" stroke-width="1.4" fill="none" />
      </svg>
    </button>
    <p class="weight">
      <PlanetGlyph source={planet.source} color={cssColor(galaxyColor(galaxy?.hue ?? 0))} />
      {formatWeight(planet)} on {planet.source === 'spotify' ? 'Spotify' : 'YouTube'}
    </p>
    {#if galaxy}
      <p class="galaxy"><span dir="auto">{galaxy.name}</span> Galaxy</p>
    {/if}
  </header>

  {#if planet.interests.length}
    <section>
      <h3>Interests</h3>
      <ul class="interests">
        {#each planet.interests as interest}
          <li dir="auto">{interest}</li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if linked.length}
    <section>
      <h3>Linked Planets</h3>
      <ul class="linked">
        {#each linked as { planet: other, reason } (other.id)}
          <li>
            <button tabindex="0" onclick={() => onselect(other)}>
              <span class="name" dir="auto">{other.name}</span>
              <span class="reason" dir="auto">{reason}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if items.length}
    <section>
      <h3>{planet.source === 'spotify' ? 'Most played' : 'Most watched'}</h3>
      <ol class="items">
        {#each items as item}
          {@const url = sourceUrl(item.url)}
          <li>
            {#if url}
              {@render sourceLink(url, item.title, 'item')}
            {:else}
              <span class="item" dir="auto">{item.title}</span>
            {/if}
          </li>
        {/each}
      </ol>
    </section>
  {/if}

  {#if home}
    <p class="leave">{@render sourceLink(home, sourceLinkLabel(planet.source), 'source')}</p>
  {/if}
</aside>

<style>
  /* The tokens and the reset come from the page, as both the app and the Share Site load them. */
  .panel {
    height: 100%;
    padding: 22px 24px 26px;
    overflow-y: auto;
    border-inline-start: 1px solid var(--rule);
    background: var(--panel);
    overflow-wrap: anywhere;
  }

  header {
    position: relative;
  }

  .close {
    position: absolute;
    inset-inline-end: -8px;
    top: -6px;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--dim);
    cursor: pointer;
  }

  .close:hover {
    color: var(--ivory);
    background: var(--rule);
  }

  /* A name from a file can be any length. Four lines names the Planet and leaves room for the rest. */
  h2 {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    overflow: hidden;
    margin: 0;
    padding-inline-end: 26px;
    font: 500 28px/1.15 var(--serif);
    outline: none;
  }

  header p {
    margin: 10px 0 0;
    color: var(--dim);
  }

  .weight {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--ivory);
    font-variant-numeric: tabular-nums;
  }

  header .galaxy {
    margin-top: 2px;
    font: italic 15px/1.4 var(--serif);
  }

  .galaxy span {
    unicode-bidi: isolate;
  }

  section {
    margin-top: 22px;
    padding-top: 14px;
    border-top: 1px solid var(--rule);
  }

  h3 {
    margin: 0 0 8px;
    color: var(--dim);
    font: italic 15px/1.3 var(--serif);
  }

  ul,
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .interests {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
  }

  .linked {
    margin: 0 -10px;
  }

  .linked button {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: 4px;
    background: none;
    text-align: start;
    cursor: pointer;
  }

  .linked button:hover {
    background: var(--rule);
  }

  /* Any of these can come from a file, at any length, and the link back to the Source still has to be in reach. */
  .linked span,
  .items li,
  .interests li,
  header .galaxy {
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
  }

  .linked .name {
    font: 500 16px/1.3 var(--serif);
  }

  .reason {
    margin-top: 1px;
    color: var(--dim);
    font-size: 13px;
    line-height: 1.35;
  }

  .items li {
    padding: 3px 0;
  }

  a {
    color: inherit;
    text-decoration-color: var(--faint);
    text-underline-offset: 3px;
  }

  a:hover {
    text-decoration-color: var(--brass);
  }

  .leave {
    margin: 24px 0 0;
  }

  /* The one way out of the app, so it looks like the brass of the buttons on the Sources screen. */
  .leave a {
    display: inline-block;
    padding: 8px 14px;
    border: 1px solid var(--brass);
    border-radius: 6px;
    color: var(--brass);
    text-decoration: none;
  }

  .leave a:hover {
    background: var(--brass);
    color: var(--night);
  }
</style>
