<script lang="ts">
  // A local server's API key, kept in the system keychain. The key goes in and never comes back out.
  import { errorMessage, forgetLocalKey, localKeys, saveLocalKey } from '../../shared/api';
  import type { LocalServer } from '../../shared/types';

  let { server, onchange }: { server: LocalServer; onchange: () => void } = $props();

  let held = $state(false);
  let key = $state('');
  let note = $state('');
  let problem = $state('');

  $effect(() => {
    const asked = server;
    localKeys().then(
      (servers) => (held = servers.includes(asked)),
      (e) => (problem = errorMessage(e)),
    );
  });

  async function act(task: () => Promise<unknown>, done: string) {
    problem = '';
    try {
      await task();
      note = done;
      onchange();
    } catch (e) {
      problem = errorMessage(e);
    }
  }

  const save = () =>
    act(async () => {
      await saveLocalKey(server, key);
      held = key.trim() !== '';
      key = '';
    }, 'Saved in your system keychain.');
  const forget = () =>
    act(async () => {
      await forgetLocalKey(server);
      held = false;
    }, 'Removed from your system keychain.');
</script>

<div class="key">
  <label>
    <span>API key, only if your server wants one</span>
    <input type="password" tabindex="0" autocomplete="off" spellcheck="false" placeholder={held ? 'A key is saved' : 'Most local servers need none'} bind:value={key} />
  </label>
  <div class="row">
    <button tabindex="0" onclick={save}>Save key</button>
    {#if held}
      <button tabindex="0" onclick={forget}>Remove key</button>
    {/if}
  </div>
  <p class="note" aria-live="polite">{note}</p>
  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}
</div>

<style>
  .key {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    gap: 10px;
  }

  .note {
    margin: 0;
    color: var(--dim);
    font-size: 14px;
  }

  .note:empty {
    position: absolute;
  }

  .error {
    margin: 0;
    color: #f08a7e;
  }
</style>
