// The Share Site as a stranger gets it: built by `demo_site` (see share-setup.ts), opened from disk, from a
// subpath and from a domain root, with no app and no IPC behind it.
import { existsSync, readFileSync, statSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { extname, join, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { expect, test, type Page } from '@playwright/test';

const root = resolve(import.meta.dirname, '..');
const PLAIN = join(root, 'dist/demo-site');
const HOSTILE = join(root, 'dist/demo-site-hostile');
const TYPES: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css' };

let server: Server;
let origin = '';

test.beforeAll(async () => {
  // The same site under /some/subpath/ and at the root, the way GitHub Pages and Vercel serve one.
  server = createServer((req, res) => {
    const path = new URL(req.url!, 'http://localhost').pathname;
    const rel = decodeURIComponent(path.replace(/^\/some\/subpath\//, '/').slice(1)) || 'index.html';
    const file = join(PLAIN, rel);
    if (!file.startsWith(PLAIN + sep) || !existsSync(file) || statSync(file).isDirectory()) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(readFileSync(file));
  });
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done));
  origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

test.afterAll(() => new Promise<void>((done) => server.close(() => done())));

const places = () => ({
  'file://': pathToFileURL(join(PLAIN, 'index.html')).href,
  subpath: `${origin}/some/subpath/`,
  'domain root': `${origin}/`,
});

/** Makes every request for a WebGL context fail, the way a computer with no graphics driver does. */
const noWebGL = (page: Page) =>
  page.addInitScript(() => {
    const real = HTMLCanvasElement.prototype.getContext as any;
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...rest: unknown[]) {
      return kind === 'webgl2' || kind === 'webgl' ? null : real.call(this, kind, ...rest);
    } as any;
  });

for (const place of ['file://', 'subpath', 'domain root'] as const) {
  test(`roast and verdict show, from ${place}`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.goto(places()[place]);
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Three hundred nights with Moth Season');
    await expect(page.getByText("Sam's Roast")).toBeVisible();
    await expect(page.getByText('The Night Clerk')).toBeVisible();
    await expect(page.getByText('72', { exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Soup, again' })).toBeVisible();
    await expect(page.getByText('Listening history from Spotify.')).toBeVisible();
    await expect(page.getByText('Watch history from YouTube.')).toBeVisible();
    await expect(page).toHaveTitle("Sam's Roast: Three hundred nights with Moth Season");
    expect(errors).toEqual([]);
  });

  test(`universe tabs work, from ${place}`, async ({ page }) => {
    await page.goto(places()[place]);
    const tabs = page.getByRole('group', { name: 'Which Source to show' });
    for (const name of ['Spotify', 'YouTube', 'Both']) {
      const tab = tabs.getByRole('button', { name, exact: true });
      await tab.click();
      await expect(tab).toHaveAttribute('aria-pressed', 'true');
    }
    await expect(page.locator('canvas')).toHaveCount(1);
  });

  test(`no third party requests, from ${place}`, async ({ page }) => {
    const requests: string[] = [];
    page.on('request', (r) => requests.push(r.url()));
    await page.goto(places()[place]);
    await expect(page.locator('canvas')).toHaveCount(1);
    await page.waitForLoadState('networkidle');
    const allowed = place === 'file://' ? 'file://' : origin;
    const elsewhere = requests.filter((url) => !url.startsWith(allowed) && !url.startsWith('data:') && !url.startsWith('blob:'));
    expect(elsewhere).toEqual([]);
  });
}

test('the page carries a strict CSP and noindex, and paths stay relative', () => {
  const html = readFileSync(join(PLAIN, 'index.html'), 'utf8');
  expect(html).toContain("default-src 'none'; script-src 'self'");
  expect(html).toContain('<meta name="robots" content="noindex" />');
  expect(html).not.toMatch(/(src|href)="\//);
  expect(html).not.toContain('type="module"');
  expect(existsSync(join(PLAIN, '.nojekyll'))).toBe(true);
});

test('hostile payload stays inert', async ({ page }) => {
  let dialogs = 0;
  page.on('dialog', async (d) => {
    dialogs += 1;
    await d.dismiss();
  });
  await page.goto(pathToFileURL(join(HOSTILE, 'index.html')).href);
  await expect(page.getByRole('heading', { level: 1 })).toContainText('<img src=x');
  await page.getByRole('button', { name: 'Show list' }).click();
  await expect(page.getByText('</script><img src=x').first()).toBeVisible();
  expect(dialogs).toBe(0);
  expect(await page.evaluate(() => (window as any).ran)).toBeUndefined();
  await expect(page.locator('img[src="x"]')).toHaveCount(0);
});

test('phone width has no sideways scroll', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 740 });
  await page.goto(pathToFileURL(join(PLAIN, 'index.html')).href);
  await expect(page.locator('canvas')).toHaveCount(1);
  const widths = await page.evaluate(() => ({ scroll: document.documentElement.scrollWidth, client: document.documentElement.clientWidth }));
  expect(widths.scroll).toBeLessThanOrEqual(widths.client);
  // Room above the sky to scroll the page with a finger, since a finger on the sky turns it.
  const box = (await page.locator('section[aria-label="Universe"]').boundingBox())!;
  expect(box.height).toBeLessThan(740);
});

test('falls back to list without webgl', async ({ page }) => {
  await noWebGL(page);
  await page.goto(pathToFileURL(join(PLAIN, 'index.html')).href);
  await expect(page.getByRole('alert')).toContainText('here is the list of Planets');
  await expect(page.getByRole('list', { name: 'Planets, largest first' }).getByRole('listitem').first()).toBeVisible();
  await expect(page.locator('canvas')).toHaveCount(0);
});

test('share js under budget', () => {
  const size = gzipSync(readFileSync(join(root, 'dist/share/assets/share.js'))).length;
  expect(size).toBeLessThan(350 * 1024);
});
