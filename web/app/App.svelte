<script lang="ts">
  import { tick } from 'svelte';
  import { roastRun as run } from './roast-run.svelte';
  import Roast from './screens/Roast.svelte';
  import Settings, { unsaved } from './screens/Settings.svelte';
  import Share from './screens/Share.svelte';
  import Sources from './screens/Sources.svelte';
  import Universe from './screens/Universe.svelte';

  // The rest are placeholders for later screens.
  const rail = [
    { label: 'Sources', ready: true },
    { label: 'Universe', ready: true },
    { label: 'Roast', ready: true },
    { label: 'Share', ready: true },
    { label: 'Settings', ready: true },
  ];

  let screen = $state('Sources');
  let importing = $state(false);
  // Goes up each time Sources finishes changing what is stored, so the Universe draws it again.
  let stored = $state(0);
  let missedError = $state('');
  let scroller = $state<HTMLElement>();
  let nav = $state<HTMLElement>();

  // The Roast screen tells people to look around while they wait, so the rail says how it is going.
  const roastMark = $derived(
    run.running
      ? { kind: 'running', words: 'A Roast is being written.' }
      : run.outcome === 'ready'
        ? { kind: 'ready', words: 'Your Roast is ready.' }
        : run.outcome === 'failed'
          ? { kind: 'failed', words: 'That Roast did not finish. Open this screen to see why.' }
          : null,
  );

  function open(label: string) {
    screen = label;
    // Sources shows it too, so it stops following the person once they can go and read it there.
    missedError = '';
    // Every screen shares one scroller, so the next one would otherwise open at this one's offset.
    if (scroller) scroller.scrollTop = 0;
  }

  async function goToSources() {
    open('Sources');
    await tick();
    // The button that was pressed is gone, and focus would otherwise fall to the page.
    nav?.querySelector<HTMLElement>('[aria-current="page"]')?.focus();
  }
</script>

<div class="shell">
  <nav aria-label="Main" bind:this={nav}>
    <p class="mark">Rostery</p>
    <ul>
      {#each rail as { label, ready }}
        <li class:last={label === 'Settings'}>
          <!-- A WebKit webview skips a button on Tab unless it has a tabindex. -->
          <button
            tabindex="0"
            disabled={!ready}
            aria-current={label === screen ? 'page' : undefined}
            title={ready ? (label === 'Roast' ? roastMark?.words : undefined) : 'Not available yet'}
            onclick={() => open(label)}
          >
            {label}
            {#if label === 'Roast' && roastMark}
              <span class="pip {roastMark.kind}" aria-hidden="true">{roastMark.kind === 'failed' ? '!' : ''}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  </nav>
  <!-- A tab stop, because a long Roast has nothing else to hold focus while it scrolls by keyboard. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <main tabindex="0" bind:this={scroller}>
    <!-- Out here, because the screen it is about has gone and any of the others can be next. Settings says it for itself. -->
    {#if unsaved.why && screen !== 'Settings'}
      <p class="unsaved" role="alert">
        Your last change in Settings was not saved, so the next Roast will not use it. {unsaved.why} Open Settings to try again.
      </p>
    {/if}
    <!-- Sources stays mounted, so an import keeps its place and its result while you look elsewhere. -->
    <Sources
      active={screen === 'Sources'}
      bind:importing
      onopen={() => open('Sources')}
      onstored={() => (stored += 1)}
      onmissederror={(message) => (missedError = message)}
    />
    {#if screen === 'Universe'}
      <Universe {importing} {missedError} {stored} />
    {:else if screen === 'Roast'}
      <Roast {importing} {missedError} {stored} onsources={goToSources} />
    {:else if screen === 'Share'}
      <Share {importing} {missedError} {stored} />
    {:else if screen === 'Settings'}
      <Settings {missedError} />
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    /* minmax(0, ...), because a plain 1fr grows to fit its content and the whole app then scrolls sideways. */
    grid-template-columns: 176px minmax(0, 1fr);
    height: 100%;
  }

  nav {
    display: flex;
    flex-direction: column;
    padding: 28px 0 20px;
    border-right: 1px solid var(--rule);
    background: linear-gradient(180deg, var(--panel), var(--night));
  }

  .mark {
    margin: 0 0 32px 24px;
    font: 600 22px/1 var(--serif);
    letter-spacing: 0.01em;
  }

  ul {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li.last {
    margin-top: auto;
  }

  /* The active item gets a brass mark on the rail's edge, like a pointer on a scale. */
  button {
    position: relative;
    width: 100%;
    padding: 9px 24px;
    border: 0;
    background: none;
    text-align: left;
    color: var(--ivory);
    cursor: pointer;
  }

  /* Inside the button, because the window's edge and the narrow layout's scrolling list clip anything outside. */
  button:focus-visible {
    outline-offset: -2px;
  }

  button[aria-current='page']::before {
    content: '';
    position: absolute;
    left: 0;
    top: 8px;
    bottom: 8px;
    width: 3px;
    background: var(--brass);
  }

  button:disabled {
    color: var(--faint);
    cursor: default;
  }

  /* A small mark beside the name. A failure has its own shape, so it does not rest on colour alone. */
  .pip {
    display: inline-block;
    margin-left: 6px;
    vertical-align: 1px;
  }

  .pip.running,
  .pip.ready {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }

  .pip.running {
    border: 1.5px solid var(--brass);
    animation: waiting 1.6s ease-in-out infinite;
  }

  .pip.ready {
    background: var(--brass);
  }

  .pip.failed {
    color: #f08a7e;
    font-weight: 700;
    line-height: 1;
    vertical-align: baseline;
  }

  @keyframes waiting {
    50% {
      opacity: 0.35;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .pip.running {
      animation: none;
    }
  }

  /* Stays in sight, because a person can be down a long Roast when this arrives. */
  .unsaved {
    flex: none;
    position: sticky;
    top: 0;
    z-index: 1;
    margin: 0;
    padding: 12px 48px;
    border-bottom: 1px solid var(--rule);
    background: var(--night);
    color: #f08a7e;
    overflow-wrap: anywhere;
  }

  /* A column, so the Universe's full height is what the message above it leaves. Its screen clips, so it can shrink, and the others cannot go below their content, so they still scroll. */
  main {
    display: flex;
    flex-direction: column;
    overflow: auto;
    background: radial-gradient(ellipse 80% 60% at 70% 0%, #172036, transparent 70%);
  }

  /* Inside its own edge, because the window clips anything outside. */
  main:focus-visible {
    outline-offset: -2px;
  }

  @media (max-width: 720px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }
    nav {
      padding: 14px 0 0;
      border-right: 0;
      border-bottom: 1px solid var(--rule);
    }
    .mark {
      margin: 0 0 8px 16px;
    }
    .unsaved {
      padding: 12px 24px;
    }
    ul {
      flex-direction: row;
      overflow-x: auto;
    }
    li.last {
      margin: 0;
    }
    button {
      width: auto;
      padding: 10px 16px;
    }
    button[aria-current='page']::before {
      inset: auto 16px 0 16px;
      width: auto;
      height: 3px;
    }
  }
</style>
