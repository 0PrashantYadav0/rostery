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
    skippedReason: null,
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
  await expect(page.getByRole('heading', { name: 'Get your Spotify Export' })).toBeVisible();
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
  skippedReason: null,
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

  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeInViewport();
});

const htmlMessage =
  'Rostery cannot read /d/watch-history.html. It is a Google Takeout in HTML, which is what Takeout gives you for ' +
  'history unless you change it. To get JSON, go to takeout.google.com, tick YouTube and YouTube Music, click ' +
  'Multiple formats and set history to JSON, then create the Export again. Google emails you a link when it is ready.';

test('html file shows json guidance', async ({ page }) => {
  const dialogs: any[] = [];
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': (args) => {
      dialogs.push(args.options);
      return ['/d/watch-history.html'];
    },
    import_paths: () => {
      throw new Error(htmlMessage);
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('alert')).toHaveText(htmlMessage);
  // The picker has to offer an HTML file, or this message could only be reached by dropping one.
  expect(dialogs[0].filters[0].extensions).toContain('html');
});

test('an html file beside a good one still explains json', async ({ page }) => {
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => ['/d/takeout'],
    import_paths: () => ({ ...nothing, filesRead: 1, filesSkipped: 1, added: 7, skippedReason: htmlMessage }),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('status')).toHaveText(
    `Added 7 Plays. Left out 1 file Rostery could not read. ${htmlMessage}`,
  );
});

test('the how-to gives the steps for both Exports', async ({ page }) => {
  await installMockIpc(page, empty);

  await page.goto('/');

  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeVisible();
  const spotify = page.locator('.howto.spotify');
  await expect(spotify).toContainText('Download your data');
  await expect(spotify).toContainText('Extended streaming history');

  const youtube = page.locator('.howto.youtube');
  await expect(youtube.getByRole('heading', { name: 'Get your YouTube Export' })).toBeVisible();
  await expect(youtube).toContainText('takeout.google.com');
  // HTML is what Takeout gives unless the person changes it, and that is the step people miss.
  await expect(youtube).toContainText('Takeout gives you history as HTML unless you change it');
  await expect(youtube).toContainText('Multiple formats');
  await expect(youtube).toContainText('JSON');
  await expect(youtube).toContainText('Google emails you a link');
  await expect(youtube).toContainText('YouTube does not let apps read your watch history');
});

test('a long path in an error wraps inside its box', async ({ page }) => {
  const path = `/Users/someone/${'very_long_folder_name_with_no_spaces_'.repeat(12)}/watch-history.html`;
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => [path],
    import_paths: () => {
      throw new Error(`Rostery could not read ${path}. Check that it is the original file from the Source.`);
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  const alert = page.getByRole('alert');
  await expect(alert).toContainText('very_long_folder_name');

  for (const width of [1200, 800, 500]) {
    await page.setViewportSize({ width, height: 800 });
    const overflow = await page.evaluate(() => {
      const main = document.querySelector('main')!;
      const drop = document.querySelector('.drop')!.getBoundingClientRect();
      const box = document.querySelector('[role="alert"]')!.getBoundingClientRect();
      return {
        sideways: main.scrollWidth - main.clientWidth,
        pastTheBox: box.right - drop.right,
      };
    });
    expect(overflow.sideways, `main scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    expect(overflow.pastTheBox, `the error leaves its box at ${width}px`).toBeLessThanOrEqual(0);
  }
});

test('focus stays on the button through an import', async ({ page }) => {
  const imported: unknown[] = [];
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: (args) => {
      imported.push(args);
      return new Promise((resolve) => (finish = () => resolve({ ...nothing, filesRead: 1, added: 5 })));
    },
  });

  await page.goto('/');
  const choose = page.getByRole('button', { name: 'Choose file' });
  await choose.focus();
  await page.keyboard.press('Enter');

  await expect(page.getByRole('status')).toContainText('Reading your history');
  await expect(choose).toBeFocused();
  await expect(choose).toBeDisabled();

  // Still in place, and still not usable twice.
  await page.keyboard.press('Enter');
  await page.getByRole('button', { name: 'Choose folder' }).dispatchEvent('click');
  await page.getByRole('button', { name: 'Try demo data' }).dispatchEvent('click');

  finish();
  await expect(page.getByRole('status')).toHaveText('Added 5 Plays.');
  await expect(choose).toBeFocused();
  await expect(choose).toBeEnabled();
  expect(imported).toHaveLength(1);
  // Try demo data has no mock here, so a click that got through would show up as an error.
  await expect(page.getByRole('alert')).toBeHidden();
});

test('clearing demo data hands focus to the button that is left', async ({ page }) => {
  let demo = true;
  await installMockIpc(page, {
    totals: () => ({ spotify: none, youtube: none }),
    has_demo_data: () => demo,
    clear_demo: () => {
      demo = false;
      return { spotify: none, youtube: none };
    },
  });

  await page.goto('/');
  const clear = page.getByRole('button', { name: 'Clear demo data' });
  await clear.focus();
  await page.keyboard.press('Enter');

  await expect(clear).toBeHidden();
  await expect(page.getByRole('button', { name: 'Try demo data' })).toBeFocused();
});
