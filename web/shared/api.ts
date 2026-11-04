// One typed function per Tauri command in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { Connections, CreatorToTag, LocalServer, DeployResult, DeployStatus, DeployTarget, Disclosure, ExportOptions, ExportResult, FinishedRoast, ImportSummary, ProviderStatus, Settings, StoredRoast, SyncSummary, Totals, Universe, UniverseFilter } from './types';

export const totals = () => invoke<Totals>('totals');
export const loadDemo = () => invoke<Totals>('load_demo');
export const clearDemo = () => invoke<Totals>('clear_demo');
export const hasDemoData = () => invoke<boolean>('has_demo_data');
export const importPaths = (paths: string[]) => invoke<ImportSummary>('import_paths', { paths });
export const getUniverse = (filter: UniverseFilter) => invoke<Universe>('get_universe', { filter });
export const generateRoast = () => invoke<FinishedRoast>('generate_roast');
export const latestRoast = () => invoke<StoredRoast | null>('latest_roast');
/** The exact Digest as text, or null with no history to send. */
export const digestPreview = () => invoke<string | null>('digest_preview');
/** The exact names the next tag step would send, heaviest first. */
export const creatorsToTag = () => invoke<CreatorToTag[]>('creators_to_tag');
export const providerStatuses = () => invoke<ProviderStatus[]>('provider_statuses');
export const providerDisclosure = () => invoke<Disclosure>('provider_disclosure');
export const getSettings = () => invoke<Settings>('get_settings');
export const saveSettings = (settings: Settings) => invoke<Settings>('save_settings', { settings });
export const defaultSystemPrompt = () => invoke<string>('default_system_prompt');
/** Writes the Share Site to Rostery's own folder, replacing the last one. */
export const exportSite = (opts: ExportOptions) => invoke<ExportResult>('export_site', { opts });
export const openSitePreview = () => invoke<void>('open_site_preview');
export const revealSite = () => invoke<void>('reveal_site');
export const deployStatus = () => invoke<DeployStatus>('deploy_status');
/** Checked by Rust before it goes in the keychain. */
export const saveDeployToken = (target: DeployTarget, token: string) => invoke<void>('save_deploy_token', { target, token });
export const forgetDeployToken = (target: DeployTarget) => invoke<void>('forget_deploy_token', { target });
/** Opens the host's page for making a token, in the system browser. */
export const openTokenPage = (target: DeployTarget) => invoke<void>('open_token_page', { target });
/** Exports fresh from `opts`, then publishes. */
export const deployGithub = (repo: string, opts: ExportOptions) => invoke<DeployResult>('deploy_github', { repo, opts });
/** Exports fresh from `opts`, publishes, and waits until Vercel says the deployment is ready. */
export const connectionStatus = () => invoke<Connections>('connection_status');
/** Opens Spotify's sign-in in the browser and waits for it to come back. Refused without consent. */
export const connectSpotify = (clientId: string, consent: boolean) => invoke<SyncSummary>('connect_spotify', { clientId, consent });
export const syncSpotify = () => invoke<SyncSummary>('sync_spotify');
export const disconnectSpotify = () => invoke<Totals>('disconnect_spotify');
/** Opens Google's sign-in in the browser and waits for it to come back. */
export const connectYoutube = (clientId: string, clientSecret: string) => invoke<SyncSummary>('connect_youtube', { clientId, clientSecret });
export const syncYoutube = () => invoke<SyncSummary>('sync_youtube');
export const disconnectYoutube = () => invoke<Totals>('disconnect_youtube');
/** A fixed page in the system browser, such as YouTube's terms. */
export const openPolicyPage = (page: 'youtube-terms' | 'google-privacy' | 'spotify-developer-policy' | 'google-cloud-console' | 'spotify-dashboard') =>
  invoke<void>('open_policy_page', { page });
/** Kept in the system keychain. An empty key removes it. */
export const saveLocalKey = (server: LocalServer, key: string) => invoke<void>('save_local_key', { server, key });
export const forgetLocalKey = (server: LocalServer) => invoke<void>('forget_local_key', { server });
/** Which local servers have a key saved. The keys never reach the webview. */
export const localKeys = () => invoke<LocalServer[]>('local_keys');
export const needsWelcome = () => invoke<boolean>('needs_welcome');
export const finishWelcome = () => invoke<void>('finish_welcome');
/** Refused unless `confirmation` is exactly DELETE. */
export const wipeEverything = (confirmation: string) => invoke<void>('wipe_everything', { confirmation });
export const deployVercel = (project: string, opts: ExportOptions) => invoke<DeployResult>('deploy_vercel', { project, opts });

/** A short phrase each time a running Roast has something new to say. */
export const onRoastProgress = (handler: (phrase: string) => void) =>
  listen<string>('roast-progress', (event) => handler(event.payload));

/** Opens a link back to a Source in the system browser. The capability only allows those two hosts. */
export const openSourceLink = (url: string) => openUrl(url);

/** The Rust side sends its message as a string. Anything else is a real Error. */
export const errorMessage = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Opens the system file picker. Resolves to null when the person cancels. */
export const chooseExport = () =>
  // HTML is offered so that a Takeout in HTML gets its message, not a file the picker greys out.
  open({ multiple: true, filters: [{ name: 'Export', extensions: ['zip', 'json', 'html'] }] });

/** The same, for someone who unzipped their download. */
export const chooseFolder = () => open({ multiple: true, directory: true });

/** Files dropped on the window. Async so a missing Tauri runtime (a plain browser) rejects instead of throwing. */
export async function onFileDrop(handler: (event: DragDropEvent) => void) {
  return getCurrentWebview().onDragDropEvent((event) => handler(event.payload));
}
