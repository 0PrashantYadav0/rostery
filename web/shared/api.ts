// One typed function per Tauri command in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import type { ImportSummary, Totals } from './types';

export const totals = () => invoke<Totals>('totals');
export const loadDemo = () => invoke<Totals>('load_demo');
export const clearDemo = () => invoke<Totals>('clear_demo');
export const hasDemoData = () => invoke<boolean>('has_demo_data');
export const importPath = (path: string) => invoke<ImportSummary>('import_path', { path });

/** Opens the system file picker. Resolves to null when the person cancels. */
export const chooseExport = () =>
  open({ multiple: true, filters: [{ name: 'Export (.zip or .json)', extensions: ['zip', 'json'] }] });

/** Files dropped on the window. Async so a missing Tauri runtime (a plain browser) rejects instead of throwing. */
export async function onFileDrop(handler: (event: DragDropEvent) => void) {
  return getCurrentWebview().onDragDropEvent((event) => handler(event.payload));
}
