<script lang="ts">
  // The first launch: what Rostery is, how to add history, who writes the Roast, then the Roast. Each step
  // says what stays on this computer.
  import { errorMessage, loadDemo, providerStatuses } from '../../shared/api';
  import type { ProviderStatus } from '../../shared/types';

  let { onfinish }: { onfinish: (screen: string, demo: boolean) => void } = $props();

  let statuses = $state.raw<ProviderStatus[]>();
  let demoReady = $state(false);
  let working = $state(false);
  let problem = $state('');

  providerStatuses().then(
    (s) => (statuses = s),
    () => (statuses = []),
  );

  const ready = $derived(statuses?.filter((s) => s.available && s.signedIn) ?? []);
  const NAMES: Record<string, string> = { 'claude-code': 'Claude Code', codex: 'Codex', ollama: 'Ollama', 'lm-studio': 'LM Studio' };

  async function tryDemo() {
    if (working) return;
    working = true;
    problem = '';
    try {
      await loadDemo();
      demoReady = true;
    } catch (e) {
      problem = errorMessage(e);
    }
    working = false;
  }
</script>

<div class="welcome">
  <p class="kicker">Welcome to Rostery</p>
  <h1>Your taste, drawn as a universe, then roasted.</h1>
  <p class="lede">
    Rostery reads your Spotify and YouTube history on this computer, turns the artists and channels you love into Planets, and has an AI
    write you a Roast. Nothing leaves this computer unless you send it.
  </p>

  <ol class="steps">
    <li>
      <h2>Add your history</h2>
      <p>Drop in your Spotify Export or Google Takeout. Rostery keeps it in a database on this computer.</p>
      <div class="row">
        <button class="primary" tabindex="0" aria-disabled={working} onclick={() => onfinish('Sources', false)}>Add my history</button>
        {#if demoReady}
          <button class="primary" tabindex="0" onclick={() => onfinish('Universe', true)}>Show my Universe</button>
        {:else}
          <button tabindex="0" aria-disabled={working} onclick={tryDemo}>Try it with demo data</button>
        {/if}
      </div>
      <p class="note" aria-live="polite">{demoReady ? 'Demo data is in. It is made up, and it goes when your own history arrives.' : ''}</p>
      {#if problem}
        <p class="error" role="alert">{problem}</p>
      {/if}
    </li>
    <li>
      <h2>Choose who writes your Roast</h2>
      <p>
        Claude Code or Codex use the subscription you already have, and get a short Digest of your history. Ollama or LM Studio keep
        everything on this computer.
      </p>
      <p class="note" aria-live="polite">
        {#if !statuses}
          Asking which ones are ready.
        {:else if ready.length}
          Ready here: {ready.map((s) => NAMES[s.id] ?? s.id).join(', ')}.
        {:else}
          None is ready yet. Settings says what each one needs.
        {/if}
      </p>
      <button tabindex="0" onclick={() => onfinish('Settings', false)}>Open Settings</button>
    </li>
    <li>
      <h2>Get roasted</h2>
      <p>The Roast screen shows exactly where your Digest goes before you press Roast me. Share it, or keep it to yourself.</p>
      <button tabindex="0" onclick={() => onfinish('Roast', false)}>Go to the Roast</button>
    </li>
  </ol>
  <button class="skip" tabindex="0" onclick={() => onfinish('Sources', false)}>Skip the welcome</button>
</div>

<style>
  .welcome {
    max-width: 760px;
    padding: 56px 48px;
  }

  .kicker {
    margin: 0;
    color: var(--brass);
    font: 600 13px/1.4 var(--sans);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    margin: 12px 0 0;
    font: 500 44px/1.08 var(--serif);
    text-wrap: balance;
  }

  .lede {
    margin: 18px 0 0;
    font: 19px/1.55 var(--serif);
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 28px;
    margin: 40px 0 0;
    padding: 0;
    counter-reset: step;
    list-style: none;
  }

  .steps li {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 0 0 0 52px;
    position: relative;
    counter-increment: step;
  }

  .steps li::before {
    content: counter(step);
    position: absolute;
    top: 0;
    left: 0;
    width: 34px;
    height: 34px;
    border: 1px solid var(--brass);
    border-radius: 50%;
    color: var(--brass);
    font: 600 16px/32px var(--serif);
    text-align: center;
  }

  h2 {
    margin: 2px 0 0;
    font: 600 21px/1.3 var(--serif);
  }

  p {
    margin: 0;
  }

  .steps p {
    color: var(--dim);
  }

  .note {
    font-size: 14px;
  }

  .note:empty {
    position: absolute;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  button {
    padding: 9px 18px;
    border: 1px solid var(--brass);
    border-radius: 6px;
    background: none;
    color: var(--ivory);
    font-weight: 600;
    cursor: pointer;
  }

  button.primary {
    background: var(--brass);
    color: var(--night);
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  .skip {
    margin-top: 36px;
    padding: 0;
    border: none;
    color: var(--dim);
    font-weight: 500;
    text-decoration: underline;
  }

  .error {
    color: #f08a7e;
  }

  @media (max-width: 720px) {
    .welcome {
      padding: 28px 18px;
    }

    h1 {
      font-size: 32px;
    }
  }
</style>
