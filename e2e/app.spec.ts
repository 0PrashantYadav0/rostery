import { expect, test } from '@playwright/test';
import { installMockIpc } from './mock-ipc';
import type { ImportSummary, SourceTotals, Totals } from '../web/shared/types';

const none: SourceTotals = { plays: 0, minutes: 0, creators: 0, firstPlay: null, lastPlay: null };

test('demo button shows totals', async ({ page }) => {
  let stored: Totals = { spotify: none, youtube: none };
  let demo = false;
  const calls: string[] = [];
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => demo,
    load_demo: () => {
      calls.push('load_demo');
      demo = true;
      stored = {
        spotify: { plays: 1234, minutes: 4321, creators: 30, firstPlay: 1_790_000_000, lastPlay: 1_799_000_000 },
        youtube: { plays: 987, minutes: 0, creators: 30, firstPlay: 1_790_000_000, lastPlay: 1_799_000_000 },
      };
      return stored;
    },
  });

  await page.goto('/');
  await expect(page.getByText('Nothing here yet')).toBeVisible();

  await page.getByRole('button', { name: 'Try demo data' }).click();

  await expect(page.getByRole('region', { name: 'Spotify' }).getByText('1,234', { exact: true })).toBeVisible();
  await expect(page.getByRole('region', { name: 'YouTube' }).getByText('987', { exact: true })).toBeVisible();
  await expect(page.getByText('Nothing here yet')).toBeHidden();
  expect(calls).toEqual(['load_demo']);
});

test('clear demo data only shows while demo data exists', async ({ page }) => {
  // Real history is already stored, so Plays exist but none of them is demo.
  const real: SourceTotals = { plays: 500, minutes: 1000, creators: 40, firstPlay: 1_700_000_000, lastPlay: 1_790_000_000 };
  let stored: Totals = { spotify: real, youtube: none };
  let demo = false;
  const calls: string[] = [];
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => demo,
    load_demo: () => {
      demo = true;
      stored = { ...stored, youtube: { ...none, plays: 90, creators: 9 } };
      return stored;
    },
    clear_demo: () => {
      calls.push('clear_demo');
      demo = false;
      stored = { spotify: real, youtube: none };
      return stored;
    },
  });

  await page.goto('/');
  const region = page.getByRole('region', { name: 'Spotify' });
  await expect(region.getByText('500', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();

  await page.getByRole('button', { name: 'Try demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeVisible();

  await page.getByRole('button', { name: 'Clear demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();
  await expect(region.getByText('500', { exact: true })).toBeVisible();
  expect(calls).toEqual(['clear_demo']);
});

test('import summary appears', async ({ page }) => {
  let stored: Totals = { spotify: none, youtube: none };
  const imported: unknown[] = [];
  const summary: ImportSummary = {
    filesRead: 3,
    added: 41_203,
    duplicates: 0,
    excludedPrivate: 12,
    excludedAds: 0,
    rejected: 0,
  };
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => false,
    // The dialog plugin's own command: the person picks one zip.
    'plugin:dialog|open': () => ['/Users/me/Downloads/my_spotify_data.zip'],
    import_path: (args) => {
      imported.push(args);
      stored = {
        spotify: { plays: 41_203, minutes: 120_000, creators: 2_100, firstPlay: 1_500_000_000, lastPlay: 1_790_000_000 },
        youtube: none,
      };
      return summary;
    },
  });

  await page.goto('/');
  await expect(page.getByText('Where to get it')).toBeVisible();
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('status')).toHaveText('Added 41,203 Plays. Skipped 12 private sessions.');
  await expect(page.getByRole('region', { name: 'Spotify' }).getByText('41,203', { exact: true })).toBeVisible();
  expect(imported).toEqual([{ path: '/Users/me/Downloads/my_spotify_data.zip' }]);
});

test('a file Rostery cannot use shows what to do', async ({ page }) => {
  await installMockIpc(page, {
    totals: () => ({ spotify: none, youtube: none }),
    has_demo_data: () => false,
    'plugin:dialog|open': () => ['/Users/me/Downloads/notes.json'],
    import_path: () => {
      throw new Error('Rostery did not find a Spotify or YouTube Export there.');
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('alert')).toContainText('Rostery did not find a Spotify or YouTube Export there.');
  await expect(page.getByRole('status')).toBeHidden();
});
