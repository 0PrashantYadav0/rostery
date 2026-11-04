<script lang="ts">
  // Opt-in, behind a plain warning that quotes Spotify's policy (docs/adr/0002). An Export is the better path.
  import { connectSpotify, disconnectSpotify, errorMessage, syncSpotify } from '../../shared/api';
  import type { SyncSummary } from '../../shared/types';

  let { connected, onchange }: { connected: boolean; onchange: () => void } = $props();

  const REDIRECT = 'http://127.0.0.1:43117/callback';

  let consent = $state(false);
  let clientId = $state('');
  let working = $state('');
  let problem = $state('');
  let summary = $state.raw<SyncSummary>();
  let copied = $state(false);

  const said = (s: SyncSummary) =>
    `Added ${s.playsAdded} new ${s.playsAdded === 1 ? 'play' : 'plays'}, ${s.affinities} top artists and ${s.interests} genres.` +
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
    act('Waiting for you to sign in to Spotify in your browser.', async () => {
      summary = await connectSpotify(clientId, consent);
    });
  const sync = () =>
    act('Syncing with Spotify.', async () => {
      summary = await syncSpotify();
    });
  const disconnect = () =>
    act('Disconnecting.', async () => {
      await disconnectSpotify();
      summary = undefined;
    });

  async function copy() {
    await navigator.clipboard.writeText(REDIRECT).then(
      () => (copied = true),
      () => (problem = 'Rostery could not copy it. Select the address and copy it yourself.'),
    );
  }
</script>

<section class="connect" aria-labelledby="spotify-connect-name">
  <h3 id="spotify-connect-name">Connect Spotify</h3>

  {#if connected}
    <p>Spotify is connected. A sync adds your last 50 plays and your top artists with their genres.</p>
    <div class="row">
      <button class="primary" tabindex="0" aria-disabled={Boolean(working)} onclick={sync}>Sync now</button>
      <button tabindex="0" aria-disabled={Boolean(working)} onclick={disconnect}>Disconnect</button>
    </div>
    <p class="aside">Disconnecting removes the token and everything this Connection added. Your Export stays.</p>
  {:else}
    <p class="warning">
      You do not need this. Your Spotify Export has your whole history, where Spotify's API gives only your last 50 plays, and it
      needs no setup.
    </p>
    <p>Connecting means making your own Spotify app and accepting Spotify's developer terms yourself. Spotify's Developer Policy says:</p>
    <blockquote>
      <p>"Do not analyze the Spotify Content or the Spotify Service for any purpose, including without limitation, creating new or derived listenership metrics"</p>
      <p>"Do not use the Spotify Platform or any Spotify Content to train a machine learning or AI model or otherwise ingest Spotify Content into a machine learning or AI model."</p>
    </blockquote>
    <p>A Universe and a Roast do both of those, so Spotify could object.</p>
    <label class="choice">
      <input type="checkbox" tabindex="0" bind:checked={consent} />
      <span>I have read this, and I connect my own Spotify app knowing what Spotify's policy says.</span>
    </label>

    {#if consent}
      <ol class="steps">
        <li>You need Spotify Premium, because Spotify only lets Premium accounts use an app in development mode.</li>
        <li>Open developer.spotify.com/dashboard, create an app, and tick Web API.</li>
        <li>
          Add this exact Redirect URI:
          <span class="redirect">
            <code>{REDIRECT}</code>
            <button tabindex="0" class="small" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
          </span>
        </li>
        <li>Copy the Client ID from the app's settings and paste it here.</li>
      </ol>
      <label class="field">
        <span>Client ID</span>
        <input type="text" tabindex="0" spellcheck="false" autocomplete="off" maxlength="64" bind:value={clientId} />
      </label>
    {/if}
    <button class="primary" tabindex="0" aria-disabled={!consent || Boolean(working)} onclick={() => consent && connect()}>Connect Spotify</button>
  {/if}

  <!-- aria-live rather than role=status: Sources stays mounted behind every screen, and its import status is the one status there. -->
  <p class="status" aria-live="polite">{working || (summary ? said(summary) : '')}</p>
  {#if problem}
    <p class="error" role="alert">{problem}</p>
  {/if}
  <p class="attribution">Listening data from Spotify.</p>
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

  blockquote {
    margin: 0;
    padding: 10px 14px;
    border-radius: 6px;
    background: var(--panel);
    color: var(--dim);
    font: italic 15px/1.5 var(--serif);
  }

  blockquote p + p {
    margin-top: 8px;
  }

  .choice {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }

  .choice input {
    margin-top: 4px;
    accent-color: var(--brass);
  }

  .steps {
    margin: 0;
    padding-left: 20px;
  }

  .steps li + li {
    margin-top: 6px;
  }

  .redirect {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  code {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--night);
    font: 13px ui-monospace, 'SF Mono', Menlo, monospace;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: min(360px, 100%);
  }

  input[type='text'] {
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

  button.small {
    padding: 2px 10px;
    font-size: 13px;
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
