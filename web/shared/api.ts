// One typed function per Tauri command in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import type { ImportSummary, Totals, Universe, UniverseFilter } from './types';

export const totals = () => invoke<Totals>('totals');
export const loadDemo = () => invoke<Totals>('load_demo');
export const clearDemo = () => invoke<Totals>('clear_demo');
export const hasDemoData = () => invoke<boolean>('has_demo_data');
export const importPaths = (paths: string[]) => invoke<ImportSummary>('import_paths', { paths });
export const getUniverse = (filter: UniverseFilter) => invoke<Universe>('get_universe', { filter });

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
