<script lang="ts">
  import Sources from './screens/Sources.svelte';
  import Universe from './screens/Universe.svelte';

  // The rest are placeholders for later screens.
  const rail = [
    { label: 'Sources', ready: true },
    { label: 'Universe', ready: true },
    { label: 'Roast', ready: false },
    { label: 'Share', ready: false },
    { label: 'Settings', ready: false },
  ];

  let screen = $state('Sources');
  let importing = $state(false);
  // Goes up each time Sources finishes changing what is stored, so the Universe draws it again.
  let stored = $state(0);
  let missedError = $state('');

  function open(label: string) {
    screen = label;
    // Sources shows it too, so it stops following the person once they can go and read it there.
    missedError = '';
  }
</script>

<div class="shell">
  <nav aria-label="Main">
    <p class="mark">Rostery</p>
    <ul>
      {#each rail as { label, ready }}
        <li class:last={label === 'Settings'}>
          <button
            disabled={!ready}
            aria-current={label === screen ? 'page' : undefined}
            title={ready ? undefined : 'Not available yet'}
            onclick={() => open(label)}
          >
            {label}
          </button>
        </li>
      {/each}
    </ul>
  </nav>
  <main>
    <!-- Sources stays mounted, so an import keeps its place and its result while you look elsewhere. -->
    <Sources
      active={screen === 'Sources'}
      bind:importing
      onopen={() => open('Sources')}
      onstored={() => (stored += 1)}
      onmissederror={(message) => (missedError = message)}
    />
    {#if screen === 'Universe'}
      {#key stored}
        <Universe {importing} {missedError} />
      {/key}
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

  main {
    overflow: auto;
    background: radial-gradient(ellipse 80% 60% at 70% 0%, #172036, transparent 70%);
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
