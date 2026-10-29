import { expect, test, type Page } from '@playwright/test';
import { emitEvent, installMockIpc } from './mock-ipc';
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
    filesSkipped: 0,
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
    import_paths: (args) => {
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
  expect(imported).toEqual([{ paths: ['/Users/me/Downloads/my_spotify_data.zip'] }]);
});

test('a file Rostery cannot use shows what to do', async ({ page }) => {
  await installMockIpc(page, {
    totals: () => ({ spotify: none, youtube: none }),
    has_demo_data: () => false,
    'plugin:dialog|open': () => ['/Users/me/Downloads/notes.json'],
    import_paths: () => {
      throw new Error('Rostery did not find a Spotify or YouTube Export there.');
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('alert')).toContainText('Rostery did not find a Spotify or YouTube Export there.');
  await expect(page.getByRole('status')).toBeHidden();
});

const nothing: ImportSummary = {
  filesRead: 0,
  filesSkipped: 0,
  added: 0,
  duplicates: 0,
  excludedPrivate: 0,
  excludedAds: 0,
  rejected: 0,
};
const empty = { totals: () => ({ spotify: none, youtube: none }), has_demo_data: () => false };

test('several picked files are one import with one summary', async ({ page }) => {
  const paths = ['/d/history_0.json', '/d/Userdata.json', '/d/broken.zip', '/d/history_1.json'];
  const imported: unknown[] = [];
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => paths,
    import_paths: (args) => {
      imported.push(args);
      return { ...nothing, filesRead: 2, filesSkipped: 1, added: 12 };
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('status')).toHaveText('Added 12 Plays. Left out 1 file Rostery could not read.');
  await expect(page.getByRole('alert')).toBeHidden();
  expect(imported).toEqual([{ paths }]);
});

test('choose folder asks the dialog for a folder and imports it', async ({ page }) => {
  const dialogs: any[] = [];
  const imported: unknown[] = [];
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': (args) => {
      dialogs.push(args.options);
      return ['/d/my_spotify_data'];
    },
    import_paths: (args) => {
      imported.push(args);
      return { ...nothing, filesRead: 1, added: 3 };
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose folder' }).click();

  await expect(page.getByRole('status')).toHaveText('Added 3 Plays.');
  expect(dialogs[0].directory).toBe(true);
  expect(imported).toEqual([{ paths: ['/d/my_spotify_data'] }]);
});

/** The drag listeners attach just after mount, so keep sending until the highlight shows. */
async function dragFile(page: Page) {
  await expect(async () => {
    await emitEvent(page, 'tauri://drag-enter', { paths: ['/d/first.zip'], position: { x: 9, y: 9 } });
    await expect(page.locator('.import')).toHaveClass(/over/, { timeout: 100 });
  }).toPass();
}

test('a file dropped during an import is not silently ignored', async ({ page }) => {
  const imported: unknown[] = [];
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    import_paths: (args) => {
      imported.push(args);
      return new Promise((resolve) => (finish = () => resolve({ ...nothing, filesRead: 1, added: 5 })));
    },
  });

  await page.goto('/');
  await dragFile(page);
  await emitEvent(page, 'tauri://drag-drop', { paths: ['/d/first.zip'], position: { x: 9, y: 9 } });
  const status = page.getByRole('status');
  await expect(status).toContainText('Reading your history');

  // The zone does not offer to take a file it cannot take yet.
  await emitEvent(page, 'tauri://drag-enter', { paths: ['/d/second.zip'], position: { x: 9, y: 9 } });
  await expect(page.locator('.import')).not.toHaveClass(/over/);
  await emitEvent(page, 'tauri://drag-drop', { paths: ['/d/second.zip'], position: { x: 9, y: 9 } });
  await expect(status).toContainText('That file was not read. Drop it again when this finishes.');

  finish();
  await expect(status).toHaveText('Added 5 Plays. The file you dropped during that was not read. Drop it again.');
  expect(imported).toEqual([{ paths: ['/d/first.zip'] }]);

  // Dropping it again works, and the note goes away.
  await emitEvent(page, 'tauri://drag-drop', { paths: ['/d/second.zip'], position: { x: 9, y: 9 } });
  await expect(status).toHaveText('Reading your history. A big file takes a few seconds.');
  finish();
  await expect(status).toHaveText('Added 5 Plays.');
  expect(imported).toEqual([{ paths: ['/d/first.zip'] }, { paths: ['/d/second.zip'] }]);
});

test('progress and the result stay on screen, in one live region', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () => new Promise((resolve) => (finish = () => resolve({ ...nothing, filesRead: 1, added: 5 }))),
  });

  await page.goto('/');
  const status = page.getByRole('status');
  // A screen reader only announces a live region that was there before its text changed.
  const region = await status.elementHandle({ timeout: 2000 });

  // dispatchEvent, because click() would scroll the button into view and hide the problem.
  await page.getByRole('button', { name: 'Choose file' }).dispatchEvent('click');
  await expect(status).toContainText('Reading your history');
  await expect(status).toBeInViewport();

  finish();
  await expect(status).toHaveText('Added 5 Plays.');
  await expect(status).toBeInViewport();
  expect(await region!.evaluate((node) => node.isConnected && node.textContent)).toBe('Added 5 Plays.');
});

test('an import error stays on screen', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => ['/d/notes.json'],
    import_paths: () => {
      throw new Error('Rostery did not find a Spotify or YouTube Export there.');
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).dispatchEvent('click');

  await expect(page.getByRole('alert')).toBeInViewport();
});

test('an error at launch is on screen', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await installMockIpc(page, {
    has_demo_data: () => false,
    totals: () => {
      throw new Error('Rostery could not use its local database.');
    },
  });

  await page.goto('/');

  await expect(page.getByRole('alert')).toContainText('could not use its local database');
  await expect(page.getByRole('alert')).toBeInViewport();
});

test('dragging a file brings the drop zone on screen in a small window', async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 600 });
  await installMockIpc(page, empty);

  await page.goto('/');
  await dragFile(page);

  await expect(page.getByRole('heading', { name: 'Add your Spotify history' })).toBeInViewport();
});
