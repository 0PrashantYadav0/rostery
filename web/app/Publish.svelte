<script lang="ts">
  // One host on the Share screen: connect once, then publish with a confirmation that says it is public.
  import { untrack } from 'svelte';
  import { errorMessage, forgetDeployToken, openTokenPage, saveDeployToken } from '../shared/api';
  import type { DeployResult, DeployTarget } from '../shared/types';

  let {
    target,
    title,
    connectedAs,
    forgettable,
    tokenHelp,
    nameLabel,
    defaultName,
    publicNote,
    waitNote,
    blocked,
    publish,
    onchange,
  }: {
    target: DeployTarget;
    title: string;
    /** Who a publish would act as, or null when not connected. */
    connectedAs: string | null;
    forgettable: boolean;
    tokenHelp: string;
    nameLabel: string;
    defaultName: string;
    publicNote: (name: string) => string;
    waitNote: string;
    /** Why nothing can be published yet, or ''. */
    blocked: string;
    publish: (name: string) => Promise<DeployResult>;
    onchange: () => void;
  } = $props();

  // Only the starting value. After that the box is the person's.
  let name = $state(untrack(() => defaultName));
  let pasting = $state(false);
  let token = $state('');
  let confirming = $state(false);
  let working = $state(false);
  let problem = $state('');
  let result = $state.raw<DeployResult>();
  let copied = $state(false);

  async function act(task: () => Promise<unknown>) {
    if (working) return;
    working = true;
    problem = '';
    try {
      await task();
    } catch (e) {
      problem = errorMessage(e);
    }
    working = false;
  }

  const connect = () =>
    act(async () => {
      pasting = true;
      await openTokenPage(target);
    });

  const save = () =>
    act(async () => {
      if (!token.trim()) throw new Error(`Paste the ${title} token first.`);
      await saveDeployToken(target, token);
      token = '';
      pasting = false;
      onchange();
    });

  const forget = () =>
    act(async () => {
      await forgetDeployToken(target);
      result = undefined;
      onchange();
    });

  const go = () =>
    act(async () => {
      confirming = false;
      result = await publish(name.trim());
    });

  async function copy() {
    if (!result) return;
    await navigator.clipboard.writeText(result.url).then(
      () => (copied = true),
      () => (problem = 'Rostery could not copy the address. Select it and copy it yourself.'),
    );
  }
</script>

<section class="host" aria-labelledby="{target}-name">
  <h2 id="{target}-name">{title}</h2>

  {#if connectedAs}
    <p class="who">
      Connected as {connectedAs}.
      {#if forgettable}
        <button type="button" class="link" tabindex="0" aria-disabled={working} onclick={forget}>Disconnect</button>
      {/if}
    </p>
    <label class="field">
      <span>{nameLabel}</span>
      <input type="text" maxlength="100" bind:value={name} oninput={() => (confirming = false)} />
    </label>
    <p class="public">{publicNote(name.trim() || defaultName)}</p>
    {#if blocked}
      <p class="aside">{blocked}</p>
    {:else if confirming}
      <div class="confirm" role="group" aria-label="Confirm publishing">
        <p>Publish your Share Site to {title} as {name.trim()}, where anyone can see it?</p>
        <div class="row">
          <button type="button" class="primary" tabindex="0" onclick={go}>Publish</button>
          <button type="button" tabindex="0" onclick={() => (confirming = false)}>Cancel</button>
        </div>
      </div>
    {:else}
      <button type="button" class="primary" tabindex="0" aria-disabled={working} onclick={() => !working && (confirming = true)}>
        {working ? 'Publishing' : `Publish to ${title}`}
      </button>
    {/if}
  {:else if pasting}
    <p class="aside">{tokenHelp}</p>
    <form class="row" onsubmit={(e) => (e.preventDefault(), save())}>
      <label class="field grow">
        <span>Token</span>
        <input type="password" autocomplete="off" spellcheck="false" bind:value={token} />
      </label>
      <button type="submit" class="primary" tabindex="0" aria-disabled={working}>Save token</button>
      <button type="button" tabindex="0" onclick={() => (pasting = false)}>Cancel</button>
    </form>
  {:else}
    <button type="button" tabindex="0" aria-disabled={working} onclick={connect}>Connect {title}</button>
  {/if}

  <p class="status" role="status">{working && !pasting ? 'Working on it.' : ''}</p>
  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}
  {#if result}
    <div class="live">
      <p>Published. Your page is at</p>
      <p class="url" dir="ltr">{result.url}</p>
      <div class="row">
        <button type="button" tabindex="0" onclick={copy}>{copied ? 'Copied' : 'Copy address'}</button>
      </div>
      <p class="aside">{waitNote}</p>
    </div>
  {/if}
</section>

<style>
  .host {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 18px 20px;
    border: 1px solid var(--rule);
    border-radius: 8px;
  }

  h2 {
    margin: 0;
    font: 600 18px/1.3 var(--serif);
  }

  p {
    margin: 0;
  }

  .who,
  .aside,
  .status {
    color: var(--dim);
  }

  .status:empty {
    position: absolute;
  }

  .public {
    padding-left: 12px;
    border-left: 3px solid var(--brass);
    font-size: 14px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: min(320px, 100%);
  }

  .grow {
    flex: 1;
  }

  input {
    padding: 8px 10px;
    border: 1px solid var(--dim);
    border-radius: 6px;
    background: var(--night);
    color: var(--ivory);
    font: inherit;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px;
    width: 100%;
  }

  .confirm {
    display: flex;
    flex-direction: column;
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

  button.link {
    padding: 0;
    border: none;
    color: var(--brass);
    font-weight: 500;
    text-decoration: underline;
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    color: #f08a7e;
  }

  .live {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    padding: 14px 16px;
    border-radius: 6px;
    background: var(--panel);
  }

  .url {
    font: 600 16px/1.4 var(--sans);
    overflow-wrap: anywhere;
  }
</style>
