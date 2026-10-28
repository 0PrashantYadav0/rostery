import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import type { Page } from '@playwright/test';

type Handlers = Record<string, (args: any) => unknown>;

// @tauri-apps/api/mocks has no imports, so its CommonJS file runs as-is in the page.
const mocks = readFileSync(createRequire(import.meta.url).resolve('@tauri-apps/api/mocks'), 'utf8');

/**
 * Answers the app's `invoke` calls with `handlers`, keyed by command name.
 * Handlers run in the test process; the page forwards each call to them.
 * Call before `page.goto`.
 */
export async function installMockIpc(page: Page, handlers: Handlers) {
  await page.exposeFunction('__mockIpc', (cmd: string, args: unknown) => {
    const handler = handlers[cmd];
    if (!handler) throw new Error(`No mock IPC handler for "${cmd}"`);
    return handler(args);
  });
  await page.addInitScript(`(() => {
    const exports = {};
    ${mocks}
    exports.mockIPC((cmd, args) => window.__mockIpc(cmd, args));
  })()`);
}
