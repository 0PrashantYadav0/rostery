// One typed function per Tauri command in src-tauri/src/commands.rs.
import { invoke } from '@tauri-apps/api/core';
import type { Totals } from './types';

export const totals = () => invoke<Totals>('totals');
export const loadDemo = () => invoke<Totals>('load_demo');
export const clearDemo = () => invoke<Totals>('clear_demo');
