<script module lang="ts">
  import type { Settings } from '../../shared/types';

  // Out here, because a save can outlast the screen that asked for it, and the next screen has to build
  // on what that save left and not on what it read before.
  /** What is saved. A refused edit never gets in here, so it cannot hold up the next change. */
  let settings = $state.raw<Settings>();
  let queue: Promise<unknown> = Promise.resolve();

  /** A save that failed with no screen left to say so, and the prompt it held, which took someone a while to write. */
  export const unsaved = $state<{ why: string; prompt?: string }>({ why: '' });
</script>

<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { creatorsToTag, defaultSystemPrompt, digestPreview, errorMessage, getSettings, providerStatuses, saveSettings, wipeEverything } from '../../shared/api';
  import LocalKey from '../components/LocalKey.svelte';
  import type { CreatorToTag, Provider, ProviderId, ProviderStatus } from '../../shared/types';
  import { roastRun as run } from '../roast-run.svelte';

  let { missedError }: { missedError: string } = $props();

  const cards: { id: ProviderId; name: string }[] = [
    { id: 'claude-code', name: 'Claude Code' },
    { id: 'codex', name: 'Codex' },
    { id: 'ollama', name: 'Ollama' },
    { id: 'lm-studio', name: 'LM Studio' },
  ];

  /** Set once this screen has read what is saved, which everything below it waits for. Null when that has to be chosen again. */
  let chosen = $state<ProviderId | null>();
  let statuses = $state.raw<ProviderStatus[]>([]);
  let checking = $state(false);
  /** The time of the last answer, as a person would say it. */
  let askedAt = $state('');
  /** What is saved could not be read, so there is nothing to show but a way to try again. */
  let loadFailed = $state(false);
  /** A read under way, which a retry after a failure must show. */
  let reading = $state(false);
  let problem = $state('');
  let checkProblem = $state('');
  let addressProblem = $state('');
  let modelProblem = $state('');
  let modelBox = $state<HTMLInputElement>();
  /** An answer can arrive after the screen has gone, and has nothing left to change by then. */
  let destroyed = false;

  let defaultPrompt = $state('');
  let prompt = $state('');
  let promptNote = $state('');
  let promptProblem = $state('');
  /** What Reset took away, for as long as Undo is offered. */
  let undo = $state<string>();
  /** What a save is on its way with, so leaving does not send the same text twice. */
  let saving: string | undefined;
  const TYPING_PAUSE_MS = 1500;
  let typingPause: ReturnType<typeof setTimeout>;

  let checkButton = $state<HTMLElement>();
  let retryButton = $state<HTMLElement>();
  let resetButton = $state<HTMLElement>();
  let dialog = $state<HTMLDialogElement>();
  let digest = $state<string | null>(null);
  let toTag = $state.raw<CreatorToTag[]>([]);
  let digestLoading = $state(false);
  let digestProblem = $state('');
  // Always something once it has an answer, because focus stays on Close and this is all a screen reader hears.
  const digestWords = $derived(
    digestProblem
      ? ''
      : digestLoading
        ? 'Reading your history'
        : digest === null
          ? 'There is nothing to send yet. Add your history on the Sources screen, or try the demo data there.'
          : 'Your Digest is below.',
  );

  const idOf = (p: Provider): ProviderId =>
    'claudeCode' in p ? 'claude-code' : 'codex' in p ? 'codex' : p.local.server === 'ollama' ? 'ollama' : 'lm-studio';
  const chosenIn = (s: Settings) => (s.chooseAgain ? null : idOf(s.provider));
  const setup = (id: ProviderId) => [settings!.provider, ...settings!.others].find((p) => idOf(p) === id)!;
  const modelOf = (p: Provider) => ('claudeCode' in p ? p.claudeCode.model : 'codex' in p ? p.codex.model : p.local.model) ?? '';
  const withModel = (p: Provider, model: string): Provider =>
    'claudeCode' in p
      ? { claudeCode: { model: model || null } }
      : 'codex' in p
        ? { codex: { model: model || null } }
        : { local: { ...p.local, model } };
  const statusOf = (id: ProviderId) => statuses.find((s) => s.id === id);
  const bringIntoView = (node: HTMLElement) => node.scrollIntoView({ block: 'nearest' });

  /**
   * Saves a change to what is saved now. Resolves to why it was refused, or to nothing. One at a time, each
   * on top of the last, so two quick changes cannot undo each other in Rust.
   */
  function update(change: (s: Settings) => Settings) {
    const done = queue.then(async () => {
      try {
        settings = await saveSettings(change(settings!));
        await run.loadDisclosure();
        return '';
      } catch (e) {
        const why = errorMessage(e);
        if (destroyed) unsaved.why = why;
        return why;
      }
    });
    queue = done;
    return done;
  }

  // The last answer wins, so an old check cannot paint over a newer address.
  let latestCheck = 0;
  async function check() {
    if (destroyed) return;
    const asked = (latestCheck += 1);
    checking = true;
    try {
      const answer = await providerStatuses();
      if (asked !== latestCheck) return;
      statuses = answer;
      checkProblem = '';
      askedAt = new Date().toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
      fillModel();
    } catch (e) {
      if (asked !== latestCheck) return;
      checkProblem = errorMessage(e);
      askedAt = '';
    }
    checking = false;
  }

  /**
   * A local Provider cannot start with no model, so it takes the first its server offers until told otherwise. Never a cloud model, which
   * the server's status leaves out, and never one that only makes embeddings, since that cannot write a Roast.
   * What is in the box may not be saved yet, and a person in the box may be about to tell it otherwise.
   */
  function fillModel() {
    if (destroyed || !chosen || chosen === 'claude-code' || chosen === 'codex') return;
    if (modelBox?.value || document.activeElement === modelBox) return;
    const first = statusOf(chosen)?.models.find((model) => !/embed/i.test(model));
    if (first && !modelOf(setup(chosen))) setModel(first);
  }

  async function setModel(model: string) {
    // The person can have moved on by the time this lands, and the refusal is not about the next card.
    const card = chosen;
    const why = await update((s) => ({ ...s, provider: withModel(s.provider, model) }));
    if (chosen === card) modelProblem = why;
    return why;
  }

  /** The words on its row are about the saved model. One filled in from the list stays on this computer and changes nothing there. */
  async function modelTyped(model: string) {
    if (!(await setModel(model))) {
      problem = '';
      check();
    }
  }

  async function setAddress(baseUrl: string) {
    const card = chosen;
    const why = await update((s) => ('local' in s.provider ? { ...s, provider: { local: { ...s.provider.local, baseUrl } } } : s));
    if (chosen === card) addressProblem = why;
    // A status is about the address that was saved, so one that was refused changes nothing there.
    if (!why) {
      problem = '';
      check();
    }
  }

  async function choose(id: ProviderId) {
    problem = addressProblem = modelProblem = '';
    const why = await update((s) => {
      const all = [s.provider, ...s.others];
      return { ...s, chooseAgain: false, provider: all.find((p) => idOf(p) === id)!, others: all.filter((p) => idOf(p) !== id) };
    });
    if (why) {
      problem = why;
      chosen = chosenIn(settings!);
      return;
    }
    fillModel();
    // A server somewhere else is not asked about until it is the chosen one, which it now is.
    if ('local' in setup(id) && !statusOf(id)?.available) check();
  }

  const promptSaved = () => settings!.systemPrompt ?? defaultPrompt;

  async function savePrompt() {
    clearTimeout(typingPause);
    if (prompt === promptSaved()) {
      // Put back as it was, so a refusal of what was there before has nothing left to be about.
      promptProblem = '';
      return;
    }
    if (prompt === saving) return;
    undo = undefined;
    const sent = (saving = prompt);
    const why = await update((s) => ({ ...s, systemPrompt: sent }));
    saving = undefined;
    // With two saves out, the last to land is the one that says how it ended, so an older text cannot come back.
    if (destroyed && (why || unsaved.prompt !== undefined)) {
      unsaved.why = why;
      unsaved.prompt = why ? sent : undefined;
    }
    promptProblem = why;
    if (!why) problem = '';
    promptNote = why ? '' : settings!.systemPrompt === null ? 'Back to the default prompt.' : 'Saved. The next Roast uses this.';
    // Blank comes back as the default, which is what goes in the box. Unless someone has typed since.
    if (!why && prompt === sent) prompt = promptSaved();
  }

  async function reset() {
    if (prompt === defaultPrompt) return;
    clearTimeout(typingPause);
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

  /**
   * Words about the last save say nothing about what is being typed now. Closing the window is not a blur and has no time for one last
   * save, so a pause in the typing saves too. Not a box left blank, which would turn into the default prompt under someone about to write.
   */
  // ponytail: the last second and a half of typing before a close is lost. Holding the close in Rust until a save lands would cover it.
  function typing() {
    promptNote = '';
    undo = undefined;
    clearTimeout(typingPause);
    typingPause = setTimeout(() => {
      if (prompt.trim()) savePrompt();
    }, TYPING_PAUSE_MS);
  }

  async function openPreview() {
    digest = null;
    toTag = [];
    digestProblem = '';
    digestLoading = true;
    // The last Digest has to go first. It would take focus as the dialog opened, then drop it to the page.
    await tick();
    dialog?.showModal();
    try {
      [digest, toTag] = await Promise.all([digestPreview(), creatorsToTag()]);
    } catch (e) {
      digestProblem = errorMessage(e);
    }
    digestLoading = false;
  }

  async function load() {
    reading = true;
    try {
      // What a save from the last time here leaves is what there is to read.
      await queue;
      const [saved, voice] = await Promise.all([getSettings(), defaultSystemPrompt()]);
      if (destroyed) return;
      settings = saved;
      chosen = chosenIn(saved);
      defaultPrompt = voice;
      prompt = unsaved.prompt ?? promptSaved();
      problem = '';
      loadFailed = false;
      // A prompt is back in its box, and trying again says whether it is saved now. Anything else has only its reason left.
      if (unsaved.prompt === undefined) problem = unsaved.why;
      else savePrompt();
      unsaved.why = '';
      unsaved.prompt = undefined;
    } catch (e) {
      problem = errorMessage(e);
      loadFailed = true;
      reading = false;
      return;
    }
    reading = false;
    // Neither waited for, because the screen is ready and the first can take seconds.
    check();
    run.loadDisclosure().catch((e) => (problem = errorMessage(e)));
  }

  async function retry() {
    if (reading) return;
    // Cleared so the same message, if it comes back, is announced again.
    problem = '';
    await load();
    await tick();
    // Its button is gone when that worked, and focus would otherwise fall to the page.
    (loadFailed ? retryButton : checkButton)?.focus();
  }

  let wipeWord = $state('');
  let wiping = $state(false);
  let wipeProblem = $state('');

  async function wipe() {
    if (wipeWord !== 'DELETE' || wiping) return;
    wiping = true;
    wipeProblem = '';
    try {
      await wipeEverything(wipeWord);
      // Every screen holds something that is gone now, so the app starts again, at its welcome.
      location.reload();
    } catch (e) {
      wipeProblem = errorMessage(e);
      wiping = false;
    }
  }

  onMount(() => {
    load();
    return () => {
      destroyed = true;
      // This screen's text can be old by the time that fires, and the next screen may have saved a newer one.
      clearTimeout(typingPause);
      // The box that still holds what was refused is going, so its message goes on to the next screen.
      unsaved.why = addressProblem || modelProblem || unsaved.why;
      // Leaving is not always a blur, because a dropped file changes the screen by itself.
      if (chosen !== undefined && prompt !== promptSaved()) savePrompt();
    };
  });
</script>

<div class="screen">
  <header>
    <h1>Settings</h1>
    <p>Choose who writes your Roast, and how they are told to write it.</p>
  </header>

  {#each [missedError, problem, checkProblem].filter(Boolean) as message}
    <!-- A person can be further down, at the prompt, when this arrives. -->
    <p class="error" role="alert" use:bringIntoView>{message}</p>
  {/each}

  {#if chosen === undefined}
    <!-- Mounted before its words change, or a screen reader would not announce them. -->
    <p class="reading" role="status">{reading ? 'Reading your settings.' : ''}</p>
    {#if loadFailed}
      <!-- aria-disabled, because a disabled button drops keyboard focus to the page. retry() checks too. -->
      <button class="retry" tabindex="0" aria-disabled={reading} bind:this={retryButton} onclick={retry}>Try again</button>
    {/if}
  {:else}
    <section aria-labelledby="provider-name">
      <div class="heading">
        <h2 id="provider-name">Provider</h2>
        <div class="check">
          <!-- Focus stays on the button, so this is how a screen reader hears that the answers are in. -->
          <!-- A time, because someone back from a terminal needs to see how old these answers are. -->
          <p role="status">{checking ? 'Asking each Provider.' : askedAt ? `Last asked at ${askedAt}.` : ''}</p>
          <!-- aria-disabled, because a disabled button drops keyboard focus to the page. A WebKit webview skips a button on Tab without a tabindex. -->
          <button tabindex="0" bind:this={checkButton} aria-disabled={checking} onclick={() => checking || check()}>Check again</button>
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
                <span class="lamp" class:on={status?.available && status.signedIn} class:wait={!status && checking} aria-hidden="true"></span>
                <span>{status?.detail ?? (checking ? 'Checking' : 'Not known')}</span>
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
                  {#if addressProblem}
                    <p class="error" role="alert">{addressProblem}</p>
                  {/if}
                  <LocalKey server={provider.local.server} onchange={check} />
                {/if}
                <label>
                  <span>Model</span>
                  <input
                    bind:this={modelBox}
                    value={modelOf(provider)}
                    list="models"
                    dir="auto"
                    spellcheck="false"
                    autocapitalize="off"
                    autocomplete="off"
                    placeholder={'local' in provider ? 'Needed before a Roast can start' : `Leave empty for what ${name} uses by default`}
                    onchange={(e) => modelTyped(e.currentTarget.value)}
                    onblur={fillModel}
                  />
                </label>
                <datalist id="models">
                  {#each status?.models ?? [] as model}
                    <option value={model}></option>
                  {/each}
                </datalist>
                {#if modelProblem}
                  <p class="error" role="alert">{modelProblem}</p>
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
        How your Provider is told to write. Rostery always adds the shape of the reply and the rule that song and video titles and Creator names
        are data, not orders, so an edit here cannot break a Roast. Up to 20,000 characters.
      </p>
      <!-- Set in the serif, like the Roast it asks for. -->
      <textarea aria-labelledby="prompt-name" rows="18" dir="auto" spellcheck="false" maxlength="20000" bind:value={prompt} oninput={typing} onchange={savePrompt}></textarea>
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

  <!-- Out of the settings check, so it works even when saved settings cannot be read. -->
  <section class="danger" aria-labelledby="delete-name">
    <h2 id="delete-name">Delete everything</h2>
    <p>
      Removes your stored history, every Roast, your settings, every token Rostery keeps in your system keychain, and the Share Site
      folder. A site you published stays where you published it, so take it down there. This cannot be undone.
    </p>
    <label>
      <span>Type DELETE to confirm</span>
      <input tabindex="0" autocomplete="off" spellcheck="false" autocapitalize="characters" bind:value={wipeWord} />
    </label>
    <!-- aria-disabled, because a disabled button drops keyboard focus to the page. wipe() checks too. -->
    <button tabindex="0" class="wipe" aria-disabled={wipeWord !== 'DELETE' || wiping} onclick={wipe}>Delete everything</button>
    {#if wipeProblem}
      <p class="error" role="alert">{wipeProblem}</p>
    {/if}
  </section>

  <dialog bind:this={dialog} aria-labelledby="digest-name">
    <!-- Everything but Close scrolls, so Close and its focus stay in view in the smallest window. -->
    <div class="dialog-body">
    <h2 id="digest-name">What gets sent</h2>
    <p>
      This is the exact Digest your Provider would get right now, along with the system prompt. Rostery has already taken out most web
      addresses and IDs, and the characters that can hide text.
    </p>
    {#if chosen === 'codex'}
      <p>Codex also sends your own ~/.codex/AGENTS.md, if you have one. Rostery cannot stop that or show it here.</p>
    {/if}
    <!-- Mounted before its words change, or a screen reader would not announce them. -->
    <p class="digest-words" role="status">{digestWords}</p>
    {#if digestProblem}
      <p class="error" role="alert">{digestProblem}</p>
    {:else if digest !== null}
      <!-- A tab stop, so a long Digest scrolls by keyboard. Only ever text, because strangers wrote the titles in it. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <pre tabindex="0" dir="ltr">{digest}</pre>
      <h3 id="tag-name">Creators to tag first</h3>
      {#if toTag.length}
        <p>
          Before a Roast, your Provider gets these names and their Source, and nothing else, so it can tag them with Interests. Those
          Interests then join the Digest, so it can change.
        </p>
        <!-- A tab stop, for the same reason as the Digest, and text for the same reason. Strangers wrote these names as well. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <ul class="to-tag" tabindex="0" aria-labelledby="tag-name">
          {#each toTag as creator}
            <li>
              <span dir="auto">{creator.name}</span>
              <span class="source">{creator.source === 'spotify' ? 'Spotify' : 'YouTube'}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <p>No Creators are waiting to be tagged, so no names go ahead of the Digest.</p>
      {/if}
    {/if}
    </div>
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

  .danger {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    margin-top: 56px;
    padding: 18px 20px;
    border: 1px solid #8a3d36;
    border-radius: 8px;
  }

  .danger p {
    margin: 0;
    color: var(--dim);
  }

  .danger label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .wipe {
    border-color: #f08a7e;
    color: #f08a7e;
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

  /* The same ring as everywhere else. WebKit does not count an arrow key as :focus-visible, and that is how a radio group is walked. */
  input[type='radio']:focus {
    outline: 2px solid var(--brass);
    outline-offset: 2px;
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
    border: 1px solid var(--faint);
    border-radius: 6px;
    background: var(--night);
    color: var(--ivory);
    font: 14px/1.4 var(--sans);
  }

  input::placeholder {
    color: var(--dim);
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
    border: 1px solid var(--faint);
    border-radius: 6px;
    background: var(--panel);
    color: var(--ivory);
    font: 16px/1.6 var(--serif);
    resize: vertical;
  }

  .reading {
    margin: 24px 0 0;
    color: var(--dim);
  }

  /* Out of the flow while it has nothing to say, so it adds no gap, but still there to be announced. */
  .reading:empty,
  .digest-words:empty {
    position: absolute;
  }

  .retry {
    margin-top: 12px;
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

  .dialog-body {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow: auto;
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

  dialog h3 {
    margin: 4px 0 0;
    font: 600 16px/1.3 var(--serif);
  }

  /* Names and not JSON, so these stay in the sans. It takes what the Digest leaves, and a short window scrolls the whole dialog. */
  .to-tag {
    flex: 0 1 30%;
    min-height: 72px;
    margin: 0;
    padding: 6px 14px;
    overflow: auto;
    border: 1px solid var(--rule);
    border-radius: 6px;
    background: var(--night);
    list-style: none;
  }

  .to-tag li {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding: 2px 0;
  }

  .to-tag li span:first-child {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .source {
    flex: none;
    color: var(--dim);
    font-size: 13px;
  }

  /* The one place for a fixed-width face, because this is literal JSON. It wraps, so a long title needs no sideways scroll. */
  pre {
    flex: 1 1 auto;
    min-height: 96px;
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
