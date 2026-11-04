<script lang="ts">
  // Subscriptions, likes and channel topics. YouTube's API has no watch history, so Takeout stays the real history.
  import { connectYoutube, disconnectYoutube, errorMessage, openPolicyPage, syncYoutube } from '../../shared/api';
  import type { SyncSummary } from '../../shared/types';

  let { connected, onchange }: { connected: boolean; onchange: () => void } = $props();

  let clientId = $state('');
  let clientSecret = $state('');
  let working = $state('');
  let problem = $state('');
  let summary = $state.raw<SyncSummary>();

  const said = (s: SyncSummary) =>
    `Added ${s.affinities} subscribed and liked channels and ${s.interests} topics.` +
    (s.demoCleared ? ' The demo data is gone, so your Universe and Roast are about you now.' : '');

  async function act(doing: string, task: () => Promise<unknown>) {
    if (working) return;
    working = doing;
    problem = '';
    try {
      await task();
    } catch (e) {
      problem = errorMessage(e);
    }
    working = '';
    onchange();
  }

  const connect = () =>
    act('Waiting for you to sign in to Google in your browser.', async () => {
      summary = await connectYoutube(clientId, clientSecret);
    });
  const sync = () =>
    act('Syncing with YouTube.', async () => {
      summary = await syncYoutube();
    });
  const disconnect = () =>
    act('Disconnecting.', async () => {
      await disconnectYoutube();
      summary = undefined;
    });
  const open = (page: Parameters<typeof openPolicyPage>[0]) => openPolicyPage(page).catch((e) => (problem = errorMessage(e)));
</script>

<section class="connect" aria-labelledby="youtube-connect-name">
  <h3 id="youtube-connect-name">Connect YouTube</h3>
  <p class="warning">
    YouTube's API has no watch history, so this adds your subscriptions, likes and channel topics, and nothing you watched. Your
    Takeout is what makes your YouTube Universe real.
  </p>

  {#if connected}
    <p>YouTube is connected. Rostery removes what it adds after 30 days, as YouTube's policies ask, so sync again now and then.</p>
    <div class="row">
      <button class="primary" tabindex="0" aria-disabled={Boolean(working)} onclick={sync}>Sync now</button>
      <button tabindex="0" aria-disabled={Boolean(working)} onclick={disconnect}>Disconnect</button>
    </div>
    <p class="aside">Disconnecting removes the tokens and everything this Connection added. Your Takeout stays.</p>
  {:else}
    <ol class="steps">
      <li>
        In the <button class="link" tabindex="0" onclick={() => open('google-cloud-console')}>Google Cloud console</button>, make a
        project and enable the YouTube Data API v3.
      </li>
      <li>Set up the OAuth consent screen as External, and add yourself as a test user.</li>
      <li>Create an OAuth client ID of type Desktop app, then paste its Client ID and Client secret here.</li>
    </ol>
    <p class="aside">While your Google app is in Testing, Google ends the connection after 7 days, and you connect again.</p>
    <div class="fields">
      <label class="field">
        <span>Client ID</span>
        <input type="text" tabindex="0" spellcheck="false" autocomplete="off" bind:value={clientId} />
      </label>
      <label class="field">
        <span>Client secret</span>
        <input type="password" tabindex="0" spellcheck="false" autocomplete="off" bind:value={clientSecret} />
      </label>
    </div>
    <button class="primary" tabindex="0" aria-disabled={Boolean(working)} onclick={connect}>Connect YouTube</button>
  {/if}

  <!-- aria-live rather than role=status: Sources stays mounted behind every screen, and its import status is the one status there. -->
  <p class="status" aria-live="polite">{working || (summary ? said(summary) : '')}</p>
  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}
  <p class="attribution">
    Data from YouTube. By connecting you agree to the
    <button class="link" tabindex="0" onclick={() => open('youtube-terms')}>YouTube Terms of Service</button>, and Google's use of
    your data follows the <button class="link" tabindex="0" onclick={() => open('google-privacy')}>Google Privacy Policy</button>.
  </p>
</section>

<style>
  .connect {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    padding: 20px 22px;
    border: 1px solid var(--rule);
    border-radius: 8px;
  }

  h3 {
    margin: 0;
    font: 600 18px/1.3 var(--serif);
  }

  p {
    margin: 0;
  }

  .warning {
    padding-left: 12px;
    border-left: 3px solid var(--brass);
  }

  .steps {
    margin: 0;
    padding-left: 20px;
  }

  .steps li + li {
    margin-top: 6px;
  }

  .fields {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    width: 100%;
  }

  .field {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-width: min(280px, 100%);
  }

  input {
    padding: 8px 10px;
    border: 1px solid var(--dim);
    border-radius: 6px;
    background: var(--night);
    color: var(--ivory);
    font: 14px ui-monospace, 'SF Mono', Menlo, monospace;
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

  button.link {
    padding: 0;
    border: none;
    color: var(--brass);
    font: inherit;
    text-decoration: underline;
  }

  button[aria-disabled='true'] {
    opacity: 0.5;
    cursor: default;
  }

  .aside,
  .status,
  .attribution {
    color: var(--dim);
    font-size: 14px;
  }

  .status:empty {
    position: absolute;
  }

  .error {
    color: #f08a7e;
  }
</style>
