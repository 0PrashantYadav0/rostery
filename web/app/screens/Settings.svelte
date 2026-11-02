<script lang="ts">
  import { onMount } from 'svelte';
  import { defaultSystemPrompt, digestPreview, errorMessage, getSettings, providerStatuses, saveSettings } from '../../shared/api';
  import type { Provider, ProviderId, ProviderStatus, Settings } from '../../shared/types';
  import { roastRun as run } from '../roast-run.svelte';

  const cards: { id: ProviderId; name: string }[] = [
    { id: 'claude-code', name: 'Claude Code' },
    { id: 'codex', name: 'Codex' },
    { id: 'ollama', name: 'Ollama' },
    { id: 'lm-studio', name: 'LM Studio' },
  ];

  /** What is saved. A refused edit never gets in here, so it cannot hold up the next change. */
  let settings = $state.raw<Settings>();
  let chosen = $state<ProviderId>();
  let statuses = $state.raw<ProviderStatus[]>([]);
  let checking = $state(false);
  let checked = $state(false);
  let problem = $state('');
  let fieldProblem = $state('');

  let defaultPrompt = $state('');
  let prompt = $state('');
  let promptNote = $state('');
  let promptProblem = $state('');
  /** What Reset took away, for as long as Undo is offered. */
  let undo = $state<string>();

  let resetButton = $state<HTMLElement>();
  let dialog = $state<HTMLDialogElement>();
  let digest = $state<string | null>(null);
  let digestLoading = $state(false);
  let digestProblem = $state('');

  const idOf = (p: Provider): ProviderId =>
    'claudeCode' in p ? 'claude-code' : 'codex' in p ? 'codex' : p.local.server === 'ollama' ? 'ollama' : 'lm-studio';
  const setup = (id: ProviderId) => [settings!.provider, ...settings!.others].find((p) => idOf(p) === id)!;
  const modelOf = (p: Provider) => ('claudeCode' in p ? p.claudeCode.model : 'codex' in p ? p.codex.model : p.local.model) ?? '';
  const withModel = (p: Provider, model: string): Provider =>
    'claudeCode' in p
      ? { claudeCode: { model: model || null } }
      : 'codex' in p
        ? { codex: { model: model || null } }
        : { local: { ...p.local, model } };
  const statusOf = (id: ProviderId) => statuses.find((s) => s.id === id);

  // One save at a time, each on top of the last, so two quick changes cannot undo each other in Rust.
  let queue: Promise<unknown> = Promise.resolve();
  /** Saves a change to what is saved now. Resolves to why it was refused, or to nothing. */
  function update(change: (s: Settings) => Settings) {
    const done = queue.then(async () => {
      try {
        settings = await saveSettings(change(settings!));
        await run.loadDisclosure();
        return '';
      } catch (e) {
        return errorMessage(e);
      }
    });
    queue = done;
    return done;
  }

  // The last answer wins, so an old check cannot paint over a newer address.
  let latestCheck = 0;
  async function check() {
    const asked = (latestCheck += 1);
    checking = true;
    try {
      const answer = await providerStatuses();
      if (asked !== latestCheck) return;
      statuses = answer;
      problem = '';
      fillModel();
    } catch (e) {
      if (asked === latestCheck) problem = errorMessage(e);
    }
    if (asked !== latestCheck) return;
    checking = false;
    checked = true;
  }

  /** A local Provider cannot start with no model, so it takes the first its server has until told otherwise. */
  function fillModel() {
    if (!settings || !chosen || chosen === 'claude-code' || chosen === 'codex') return;
    const first = statusOf(chosen)?.models[0];
    if (first && !modelOf(setup(chosen))) setModel(first);
  }

  function setModel(model: string) {
    update((s) => ({ ...s, provider: withModel(s.provider, model) })).then((why) => (fieldProblem = why));
  }

  async function setAddress(baseUrl: string) {
    const why = await update((s) => ('local' in s.provider ? { ...s, provider: { local: { ...s.provider.local, baseUrl } } } : s));
    fieldProblem = why;
    // A status is about the address that was saved, so one that was refused changes nothing there.
    if (!why) check();
  }

  async function choose(id: ProviderId) {
    fieldProblem = '';
    const why = await update((s) => {
      const all = [s.provider, ...s.others];
      return { ...s, provider: all.find((p) => idOf(p) === id)!, others: all.filter((p) => idOf(p) !== id) };
    });
    if (why) {
      problem = why;
      chosen = idOf(settings!.provider);
      return;
    }
    fillModel();
  }

  const promptSaved = () => settings!.systemPrompt ?? defaultPrompt;

  async function savePrompt() {
    if (prompt === promptSaved()) return;
    undo = undefined;
    const sent = prompt;
    const why = await update((s) => ({ ...s, systemPrompt: sent }));
    promptProblem = why;
    promptNote = why ? '' : settings!.systemPrompt === null ? 'Back to the default prompt.' : 'Saved. The next Roast uses this.';
    // Blank comes back as the default, which is what goes in the box. Unless someone has typed since.
    if (!why && prompt === sent) prompt = promptSaved();
  }

  async function reset() {
    if (prompt === defaultPrompt) return;
    const lost = prompt;
    prompt = defaultPrompt;
    promptProblem = '';
    const why = await update((s) => ({ ...s, systemPrompt: null }));
    promptProblem = why;
    promptNote = why ? '' : 'Back to the default prompt.';
    if (!why) undo = lost;
  }

  function undoReset() {
    prompt = undo!;
    savePrompt();
    // Its button is about to go, and focus would otherwise fall to the page.
    resetButton?.focus();
  }

  /** Words about the last save say nothing about what is being typed now. */
  function typing() {
    promptNote = '';
    undo = undefined;
  }

  async function openPreview() {
    digest = null;
    digestProblem = '';
    digestLoading = true;
    dialog?.showModal();
    try {
      digest = await digestPreview();
    } catch (e) {
      digestProblem = errorMessage(e);
    }
    digestLoading = false;
  }

  onMount(() => {
    (async () => {
      try {
        const [saved, voice] = await Promise.all([getSettings(), defaultSystemPrompt()]);
        settings = saved;
        chosen = idOf(saved.provider);
        defaultPrompt = voice;
        prompt = promptSaved();
        await run.loadDisclosure();
      } catch (e) {
        problem = errorMessage(e);
        return;
      }
      // Not waited for, because the screen is ready and these can take seconds.
      check();
    })();
    // Leaving is not always a blur, because a dropped file changes the screen by itself.
    return () => {
      if (settings && prompt !== promptSaved()) savePrompt();
    };
  });
</script>

<div class="screen">
  <header>
    <h1>Settings</h1>
    <p>Choose who writes your Roast, and how they are told to write it.</p>
  </header>

  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}

  {#if settings}
    <section aria-labelledby="provider-name">
      <div class="heading">
        <h2 id="provider-name">Provider</h2>
        <div class="check">
          <!-- Focus stays on the button, so this is how a screen reader hears that the answers are in. -->
          <p role="status">{checking ? 'Asking each Provider.' : checked ? 'Asked just now.' : ''}</p>
          <!-- aria-disabled, because a disabled button drops keyboard focus to the page. A WebKit webview skips a button on Tab without a tabindex. -->
          <button tabindex="0" aria-disabled={checking} onclick={() => checking || check()}>Check again</button>
        </div>
      </div>
      {#if run.running}
        <p class="running">A Roast is running. It keeps the Provider and prompt it started with, and what you change here is for the next one.</p>
      {/if}

      <div class="switch" role="radiogroup" aria-labelledby="provider-name">
        {#each cards as { id, name }}
          {@const status = statusOf(id)}
          {@const provider = setup(id)}
          <div class="row" class:chosen={chosen === id}>
            <label title={status?.sendsTo}>
              <input type="radio" name="provider" tabindex="0" value={id} bind:group={chosen} onchange={() => choose(id)} />
              <span class="name">{name}</span>
              <span class="state">
                <span class="lamp" class:on={status?.available && status.signedIn} class:wait={!status} aria-hidden="true"></span>
                <span>{status?.detail ?? 'Checking'}</span>
              </span>
            </label>
            {#if chosen === id}
              <div class="fields">
                {#if 'local' in provider}
                  <label>
                    <span>Server address</span>
                    <input
                      value={provider.local.baseUrl}
                      spellcheck="false"
                      autocapitalize="off"
                      autocomplete="off"
                      onchange={(e) => setAddress(e.currentTarget.value)}
                    />
                  </label>
                {/if}
                <label>
                  <span>Model</span>
                  <input
                    value={modelOf(provider)}
                    list="models"
                    spellcheck="false"
                    autocapitalize="off"
                    autocomplete="off"
                    placeholder={'local' in provider ? 'Needed before a Roast can start' : `Leave empty for what ${name} uses by default`}
                    onchange={(e) => setModel(e.currentTarget.value)}
                  />
                </label>
                <datalist id="models">
                  {#each status?.models ?? [] as model}
                    <option value={model}></option>
                  {/each}
                </datalist>
                {#if fieldProblem}
                  <p class="error" role="alert">{fieldProblem}</p>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      </div>

      <div class="sends" class:flagged={run.disclosure?.flagged}>
        <!-- Always mounted: a screen reader only announces a live region that was there before its text changed. -->
        <p role="status">{run.disclosure?.sendsTo}</p>
        <button tabindex="0" onclick={openPreview}>See what gets sent</button>
      </div>
    </section>

    <section aria-labelledby="prompt-name">
      <div class="heading">
        <h2 id="prompt-name">System prompt</h2>
        <button tabindex="0" bind:this={resetButton} aria-disabled={prompt === defaultPrompt} onclick={reset}>Reset to default</button>
      </div>
      <p class="about">
        How your Provider is told to write. Rostery always adds the shape of the reply and the rule that song titles and names are data,
        not orders, so an edit here cannot break a Roast.
      </p>
      <!-- Set in the serif, like the Roast it asks for. -->
      <textarea aria-labelledby="prompt-name" rows="18" spellcheck="false" bind:value={prompt} oninput={typing} onchange={savePrompt}></textarea>
      <div class="note">
        <p role="status">{promptNote}</p>
        {#if undo !== undefined}
          <button tabindex="0" onclick={undoReset}>Undo</button>
        {/if}
      </div>
      {#if promptProblem}
        <p class="error" role="alert">{promptProblem}</p>
      {/if}
    </section>
  {/if}

  <dialog bind:this={dialog} aria-labelledby="digest-name">
    <h2 id="digest-name">What gets sent</h2>
    <p>
      This is the exact Digest your Provider gets, along with the system prompt. Rostery has already taken out links, IDs and hidden
      characters.
    </p>
    {#if digestProblem}
      <p class="error" role="alert">{digestProblem}</p>
    {:else if digestLoading}
      <p>Reading your history</p>
    {:else if digest === null}
      <p>There is nothing to send yet. Add your history on the Sources screen, or try the demo data there.</p>
    {:else}
      <!-- A tab stop, so a long Digest scrolls by keyboard. Only ever text, because strangers wrote the titles in it. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <pre tabindex="0" dir="ltr">{digest}</pre>
    {/if}
    <button tabindex="0" onclick={() => dialog?.close()}>Close</button>
  </dialog>
</div>

<style>
  .screen {
    max-width: 880px;
    padding: 40px 48px 56px;
    /* An address or a model name has few places to break. WebKit before Safari 15.4 only knows the older word-break. */
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  header p {
    margin: 8px 0 0;
    color: var(--dim);
  }

  section {
    margin-top: 32px;
  }

  .heading {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
  }

  h2 {
    margin: 0;
    font: 600 20px/1.2 var(--serif);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--dim);
    font-size: 14px;
  }

  .check p {
    margin: 0;
  }

  button {
    flex: none;
    padding: 7px 14px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: none;
    cursor: pointer;
  }

  button:hover:not([aria-disabled='true']) {
    border-color: var(--faint);
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  .running,
  .about {
    margin: 8px 0 0;
    color: var(--dim);
  }

  /* One instrument with four positions, not four things to buy. The chosen row carries the rail's brass pointer. */
  .switch {
    margin-top: 12px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
  }

  .row {
    position: relative;
    padding: 14px 20px;
  }

  .row + .row {
    border-top: 1px solid var(--rule);
  }

  .row.chosen::before {
    content: '';
    position: absolute;
    left: 0;
    top: 12px;
    bottom: 12px;
    width: 3px;
    background: var(--brass);
  }

  .row > label {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: baseline;
    gap: 2px 12px;
    cursor: pointer;
  }

  input[type='radio'] {
    margin: 0;
    accent-color: var(--brass);
    cursor: pointer;
  }

  .name {
    font: 500 18px/1.3 var(--serif);
  }

  .chosen .name {
    color: var(--brass);
  }

  .state {
    grid-column: 2;
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    color: var(--dim);
    font-size: 14px;
  }

  /* The dial's small relation: a ring that is lit when this Provider could write a Roast right now. */
  .lamp {
    flex: none;
    width: 9px;
    height: 9px;
    border: 1.5px solid var(--faint);
    border-radius: 50%;
  }

  .lamp.on {
    border-color: var(--brass);
    background: var(--brass);
  }

  .lamp.wait {
    border-style: dotted;
  }

  .fields {
    display: grid;
    gap: 10px;
    margin: 12px 0 2px 25px;
  }

  .fields label {
    display: block;
    color: var(--dim);
    font-size: 13px;
  }

  input:not([type='radio']) {
    display: block;
    width: 100%;
    margin-top: 3px;
    padding: 7px 10px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--night);
    color: var(--ivory);
    font: 14px/1.4 var(--sans);
  }

  input::placeholder {
    color: var(--faint);
  }

  .sends {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 20px;
    margin-top: 14px;
  }

  .sends p {
    flex: 1 1 320px;
    margin: 0;
    color: var(--dim);
  }

  /* Red and the full ink, because the person has to read this one. */
  .sends.flagged p {
    padding-left: 12px;
    border-left: 3px solid #f08a7e;
    color: var(--ivory);
  }

  textarea {
    display: block;
    width: 100%;
    margin-top: 12px;
    padding: 18px 20px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
    color: var(--ivory);
    font: 16px/1.6 var(--serif);
    resize: vertical;
  }

  .note {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin-top: 8px;
    min-height: 34px;
  }

  .note p {
    margin: 0;
    color: var(--dim);
  }

  .error {
    margin: 12px 0 0;
    color: #f08a7e;
  }

  .fields .error {
    margin: 0;
  }

  dialog {
    width: min(760px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    padding: 24px;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--panel);
    color: var(--ivory);
  }

  /* Only open, because a closed dialog must stay hidden and a display of its own would show it. */
  dialog[open] {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  dialog::backdrop {
    background: rgb(12 16 25 / 0.75);
  }

  dialog p {
    margin: 0;
    color: var(--dim);
  }

  dialog .error {
    color: #f08a7e;
  }

  dialog button {
    align-self: flex-end;
  }

  /* The one place for a fixed-width face, because this is literal JSON. It wraps, so a long title needs no sideways scroll. */
  pre {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--night);
    font: 13px/1.5 ui-monospace, 'SF Mono', Menlo, Consolas, monospace;
    white-space: pre-wrap;
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  @media (max-width: 720px) {
    .screen {
      padding: 24px;
    }
    .row {
      padding: 12px 16px;
    }
    .fields {
      margin-left: 0;
    }
    textarea {
      padding: 14px;
    }
  }
</style>
