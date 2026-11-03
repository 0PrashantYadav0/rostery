<script lang="ts">
  import { deployGithub, deployStatus, deployVercel, errorMessage, exportSite, openSitePreview, revealSite, totals } from '../../shared/api';
  import type { DeployStatus, ExportOptions, ExportResult, Totals } from '../../shared/types';
  import Publish from '../Publish.svelte';
  import { roastRun as run } from '../roast-run.svelte';

  let { importing, missedError, stored }: { importing: boolean; missedError: string; stored: number } = $props();

  const ROAST_WARNING =
    'Your Roast quotes song and video titles and names from all your history, and the other choices here do not change its words. Read it before you share it.';

  let opts = $state<ExportOptions>({
    displayName: 'Anonymous',
    includeRoast: true,
    includeSpotify: true,
    includeYoutube: true,
    includeItemTitles: false,
  });
  let have = $state.raw<Totals>();
  let result = $state.raw<ExportResult>();
  let problem = $state('');
  let working = $state(false);
  let hosts = $state.raw<DeployStatus>();
  let hostsProblem = $state('');

  // Asks GitHub whose token it is, so it is its own call that can fail without taking the screen with it.
  async function loadHosts() {
    try {
      hosts = await deployStatus();
      hostsProblem = '';
    } catch (e) {
      hostsProblem = errorMessage(e);
    }
  }
  loadHosts();

  // Runs when this screen opens and each time Sources finishes changing what is stored.
  $effect(() => {
    stored;
    Promise.all([totals(), run.load()]).then(
      ([t]) => (have = t),
      (e) => (problem = errorMessage(e)),
    );
  });

  const roast = $derived(run.latest?.roast);
  const spotify = $derived((have?.spotify.plays ?? 0) > 0);
  const youtube = $derived((have?.youtube.plays ?? 0) > 0);
  const going = $derived.by(() => {
    const lines: string[] = [];
    if (opts.includeRoast && roast) lines.push(`Your Roast, "${roast.headline}", and its Verdict`);
    if (opts.includeSpotify && spotify) lines.push(`Your Spotify Universe, ${have!.spotify.creators} artists`);
    if (opts.includeYoutube && youtube) lines.push(`Your YouTube Universe, ${have!.youtube.creators} channels`);
    if (opts.includeSpotify && spotify && opts.includeYoutube && youtube) lines.push('The two together, as one Universe');
    if (lines.length) lines.push(opts.includeItemTitles ? 'The top song and video titles of each Planet' : 'No song or video titles, beyond what the Roast quotes');
    return lines;
  });
  const size = (bytes: number) => (bytes > 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.round(bytes / 1000)} KB`);

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

  // aria-disabled leaves the button pressable, so this checks for itself.
  const save = () =>
    going.length &&
    act(async () => {
      result = await exportSite(opts);
    });
</script>

<div class="screen">
  <header>
    <h1>Share</h1>
    <p>Build a web page of your Roast and your Universe that anyone can open. It is made here, from what you choose below, and goes nowhere until you send it.</p>
  </header>

  {#if importing}
    <p class="status" role="status">Reading your history. This screen will update when that finishes.</p>
  {/if}
  {#each [missedError, problem].filter(Boolean) as message}
    <p class="error" role="alert">{message}</p>
  {/each}

  <form onsubmit={(e) => (e.preventDefault(), save())}>
    <label class="name">
      <span>Your name on the page</span>
      <input type="text" maxlength="60" dir="auto" bind:value={opts.displayName} />
    </label>

    <fieldset>
      <legend>What goes in</legend>
      <label class="choice">
        <input type="checkbox" tabindex="0" bind:checked={opts.includeRoast} disabled={!roast} />
        <span>
          Your Roast
          {#if !roast}<span class="aside">You have no Roast yet. Write one on the Roast screen.</span>{/if}
        </span>
      </label>
      <!-- Always there, beside the choice, because nothing else here changes what the Roast says. -->
      <p class="warning">{ROAST_WARNING}</p>
      <label class="choice">
        <input type="checkbox" tabindex="0" bind:checked={opts.includeSpotify} disabled={!spotify} />
        <span>Spotify Universe {#if !spotify}<span class="aside">No Spotify history stored.</span>{/if}</span>
      </label>
      <label class="choice">
        <input type="checkbox" tabindex="0" bind:checked={opts.includeYoutube} disabled={!youtube} />
        <span>YouTube Universe {#if !youtube}<span class="aside">No YouTube history stored.</span>{/if}</span>
      </label>
      <label class="choice">
        <input type="checkbox" tabindex="0" bind:checked={opts.includeItemTitles} />
        <span>Song and video titles <span class="aside">A video title can say far more about you than a channel name.</span></span>
      </label>
    </fieldset>

    <section class="going" aria-labelledby="going-name">
      <h2 id="going-name">On the page</h2>
      {#if going.length}
        <ul>
          {#each going as line}
            <li dir="auto">{line}</li>
          {/each}
        </ul>
        <p class="aside">Nothing else from your history, and none of your settings or keys.</p>
      {:else}
        <p>Nothing yet. Choose your Roast or a Universe.</p>
      {/if}
    </section>

    <div class="actions">
      <button type="submit" class="primary" tabindex="0" aria-disabled={working || !going.length}>Export</button>
      {#if result}
        <button type="button" tabindex="0" aria-disabled={working} onclick={() => act(openSitePreview)}>Preview in browser</button>
        <button type="button" tabindex="0" aria-disabled={working} onclick={() => act(revealSite)}>Reveal folder</button>
      {/if}
    </div>
    <p class="result" role="status">
      {result ? `Saved ${size(result.bytes)} to ${result.dir}. Open it in your browser to check it before you send it anywhere.` : ''}
    </p>
  </form>

  <div class="hosts">
    <h2>Publish</h2>
    <p class="aside">Each publish builds the page again from the choices above, then sends it with your own account. Rostery keeps the token in your system keychain.</p>
    {#if hostsProblem}
      <p class="error" role="alert">{hostsProblem}</p>
    {/if}
    {#if hosts}
      <Publish
        target="github"
        title="GitHub Pages"
        connectedAs={hosts.github}
        forgettable={!hosts.githubThroughGh}
        tokenHelp="Your browser shows GitHub's page for a new token with the public_repo scope. Create it, then paste it here."
        nameLabel="Repo name"
        defaultName="rostery-universe"
        publicNote={(name) => `The repo ${name} and its site are public. Anyone with the address can see them.`}
        waitNote="GitHub Pages can take a minute to show a new site."
        blocked={going.length ? '' : 'Choose your Roast or a Universe above first.'}
        publish={(repo) => deployGithub(repo, opts)}
        onchange={loadHosts}
      />
      <Publish
        target="vercel"
        title="Vercel"
        connectedAs={hosts.vercel ? 'your Vercel account' : null}
        forgettable
        tokenHelp="Your browser shows Vercel's page for tokens. Create one, then paste it here."
        nameLabel="Project name"
        defaultName="rostery-universe"
        publicNote={(name) => `The Vercel project ${name} and its site are public. Anyone with the address can see them.`}
        waitNote="Rostery waited until Vercel said the site was ready, so it is live now."
        blocked={going.length ? '' : 'Choose your Roast or a Universe above first.'}
        publish={(project) => deployVercel(project, opts)}
        onchange={loadHosts}
      />
    {/if}
  </div>
</div>

<style>
  .screen {
    max-width: 760px;
    padding: 40px 48px 56px;
    overflow-wrap: anywhere;
  }

  h1 {
    margin: 0;
    font: 500 38px/1.1 var(--serif);
  }

  header p {
    max-width: 36em;
    margin: 8px 0 0;
    color: var(--dim);
  }

  .status {
    color: var(--dim);
  }

  .error {
    margin: 20px 0 0;
    color: #f08a7e;
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 24px;
    margin-top: 32px;
  }

  .name {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 360px;
  }

  input[type='text'] {
    padding: 8px 10px;
    border: 1px solid var(--dim);
    border-radius: 6px;
    background: var(--night);
    color: var(--ivory);
    font: inherit;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 0;
    padding: 18px 20px;
    border: 1px solid var(--rule);
    border-radius: 8px;
  }

  legend {
    padding: 0 6px;
    font: 600 16px/1.3 var(--serif);
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }

  .choice input {
    margin-top: 4px;
    accent-color: var(--brass);
  }

  .aside {
    display: block;
    color: var(--dim);
    font-size: 14px;
  }

  .warning {
    margin: -4px 0 4px 26px;
    padding-left: 12px;
    border-left: 3px solid var(--brass);
    font-size: 14px;
  }

  .going h2 {
    margin: 0;
    font: 600 16px/1.3 var(--serif);
  }

  .going ul {
    margin: 8px 0 0;
    padding-left: 20px;
  }

  .going p {
    margin: 8px 0 0;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
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

  .result {
    margin: 0;
    color: var(--dim);
  }

  .hosts {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-top: 40px;
  }

  .hosts > h2 {
    margin: 0;
    font: 500 26px/1.2 var(--serif);
  }

  .hosts > p {
    margin: 0;
  }

  .result:empty {
    position: absolute;
  }

  @media (max-width: 720px) {
    .screen {
      padding: 18px;
    }
  }
</style>
