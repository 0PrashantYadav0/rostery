// One typed function per Tauri command in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import type { Disclosure, FinishedRoast, ImportSummary, ProviderStatus, Settings, StoredRoast, Totals, Universe, UniverseFilter } from './types';

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
export const providerStatuses = () => invoke<ProviderStatus[]>('provider_statuses');
export const providerDisclosure = () => invoke<Disclosure>('provider_disclosure');
export const getSettings = () => invoke<Settings>('get_settings');
export const saveSettings = (settings: Settings) => invoke<Settings>('save_settings', { settings });
export const defaultSystemPrompt = () => invoke<string>('default_system_prompt');

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
