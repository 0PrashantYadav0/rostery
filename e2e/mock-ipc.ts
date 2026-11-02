import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import type { Page } from '@playwright/test';

type Handlers = Record<string, (args: any) => unknown>;

// @tauri-apps/api/mocks has no imports, so its CommonJS file runs as-is in the page.
const mocks = readFileSync(createRequire(import.meta.url).resolve('@tauri-apps/api/mocks'), 'utf8');

export const CLAUDE_CODE = {
  sendsTo:
    'Your Digest goes to Anthropic through Claude Code. It holds song and video titles, Creator names, Interests and dates, never your raw Plays.',
  flagged: false,
};

/**
 * Answers the app's `invoke` calls with `handlers`, keyed by command name.
 * Handlers run in the test process; the page forwards each call to them.
 * Call before `page.goto`.
 */
export async function installMockIpc(page: Page, handlers: Handlers) {
  // The Roast screen asks this before it offers the button, and most tests have no view on it.
  const all: Handlers = { provider_disclosure: () => CLAUDE_CODE, ...handlers };
  await page.exposeFunction('__mockIpc', (cmd: string, args: unknown) => {
    const handler = all[cmd];
    if (!handler) throw new Error(`No mock IPC handler for "${cmd}"`);
    return handler(args);
  });
  await page.addInitScript(`(() => {
    const exports = {};
    ${mocks}
    exports.mockWindows('main');
    exports.mockIPC((cmd, args) => window.__mockIpc(cmd, args), { shouldMockEvents: true });
  })()`);
}

/** Sends the page a Tauri event, the way the desktop shell reports a file being dragged. */
export const emitEvent = (page: Page, event: string, payload?: unknown) =>
  page.evaluate(
    ([event, payload]) => (window as any).__TAURI_INTERNALS__.invoke('plugin:event|emit', { event, payload }),
    [event, payload] as const,
  );
