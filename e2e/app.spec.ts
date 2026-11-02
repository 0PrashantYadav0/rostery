import { readFileSync } from 'node:fs';
import { expect, test, type Locator, type Page } from '@playwright/test';
import { Color } from 'three';
import { CLAUDE_CODE, emitEvent, installMockIpc } from './mock-ipc';
import { galaxyColor } from '../web/universe/scene-data';
import type { ImportSummary, Planet, Provider, ProviderStatus, Settings, Source, SourceTotals, StoredRoast, Totals, Universe, UniverseFilter } from '../web/shared/types';

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
  const nothingStored: Totals = { spotify: none, youtube: none };
  let stored = nothingStored;
  let demo = false;
  const calls: string[] = [];
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => demo,
    load_demo: () => {
      demo = true;
      stored = { spotify: { ...none, plays: 90, creators: 9 }, youtube: none };
      return stored;
    },
    clear_demo: () => {
      calls.push('clear_demo');
      demo = false;
      stored = nothingStored;
      return stored;
    },
  });

  await page.goto('/');
  await expect(page.getByText('Nothing here yet')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();

  await page.getByRole('button', { name: 'Try demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeVisible();

  await page.getByRole('button', { name: 'Clear demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();
  await expect(page.getByRole('button', { name: 'Try demo data' })).toBeVisible();
  expect(calls).toEqual(['clear_demo']);
});

test('demo data is not offered once real history is stored', async ({ page }) => {
  const real: SourceTotals = { plays: 500, minutes: 1000, creators: 40, firstPlay: 1_700_000_000, lastPlay: 1_790_000_000 };
  await installMockIpc(page, { totals: () => ({ spotify: real, youtube: none }), has_demo_data: () => false });

  await page.goto('/');
  await expect(page.getByRole('region', { name: 'Spotify' }).getByText('500', { exact: true })).toBeVisible();

  await expect(page.getByRole('button', { name: 'Try demo data' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toHaveCount(0);
});

test('the refusal to mix demo data with real history is shown', async ({ page }) => {
  // A store from an earlier build can hold both, so the button is still there and Rust says no.
  const message = 'Your own history is already here, so Rostery keeps demo data out of it.';
  await installMockIpc(page, {
    ...withHistory,
    has_demo_data: () => true,
    load_demo: () => {
      throw new Error(message);
    },
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Try demo data' }).click();

  await expect(page.getByRole('alert')).toHaveText(message);
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
    demoPlaysRemoved: 0,
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
  demoPlaysRemoved: 0,
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

test('importing real history takes the demo data away and says so', async ({ page }) => {
  let demo = true;
  const real: SourceTotals = { ...none, plays: 5, creators: 1 };
  await installMockIpc(page, {
    totals: () => ({ spotify: demo ? { ...none, plays: 1937, creators: 60 } : real, youtube: none }),
    has_demo_data: () => demo,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () => {
      demo = false;
      return { ...nothing, filesRead: 1, added: 5, demoPlaysRemoved: 1937 };
    },
  });

  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeVisible();
  await page.getByRole('button', { name: 'Choose file' }).click();

  await expect(page.getByRole('status')).toHaveText('Added 5 Plays. Removed the demo data and any Roast of it, so only your real history is left.');
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();
  await expect(page.getByRole('button', { name: 'Try demo data' })).toHaveCount(0);
});

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
  'history unless you change it. To get JSON, go to takeout.google.com, click Deselect all, tick YouTube and YouTube ' +
  'Music, click Multiple formats and set history to JSON, then create the Export again. Google emails you a link when ' +
  'it is ready.';

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

const tauriConfig = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));

test('the demo result is on screen at the default window size', async ({ page }) => {
  const { width, height } = tauriConfig.app.windows[0];
  await page.setViewportSize({ width, height });
  let stored: Totals = { spotify: none, youtube: none };
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => false,
    load_demo: () => {
      stored = {
        spotify: { plays: 1234, minutes: 4321, creators: 30, firstPlay: 1_790_000_000, lastPlay: 1_799_000_000 },
        youtube: { plays: 987, minutes: 0, creators: 30, firstPlay: 1_790_000_000, lastPlay: 1_799_000_000 },
      };
      return stored;
    },
  });

  await page.goto('/');
  // A real click, because it scrolls to the button first the way a person has to.
  await page.getByRole('button', { name: 'Try demo data' }).click();

  await expect(page.getByRole('region', { name: 'Spotify' }).getByText('1,234', { exact: true })).toBeInViewport();
  await expect(page.getByRole('region', { name: 'YouTube' }).getByText('987', { exact: true })).toBeInViewport();
});

test('the app never scrolls sideways, down to the smallest window and past it', async ({ page }) => {
  const { minWidth, minHeight } = tauriConfig.app.windows[0];
  expect(minWidth).toBeGreaterThanOrEqual(419);
  expect(minHeight).toBeGreaterThan(0);
  await installMockIpc(page, empty);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeVisible();

  for (const width of [320, 360, 419, minWidth, 720, 721, 1200]) {
    await page.setViewportSize({ width, height: minHeight });
    const overflow = await page.evaluate(() => ({
      page: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      main: document.querySelector('main')!.scrollWidth - document.querySelector('main')!.clientWidth,
    }));
    expect(overflow.page, `the page scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    expect(overflow.main, `main scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
  }
});

test('the nav focus ring is never clipped', async ({ page }) => {
  await installMockIpc(page, empty);
  await page.goto('/');

  for (const width of [500, 1200]) {
    await page.setViewportSize({ width, height: 700 });
    const clipped = await page.getByRole('button', { name: 'Sources' }).evaluate((button: HTMLElement) => {
      button.focus();
      const { outlineWidth, outlineOffset } = getComputedStyle(button);
      const reach = parseFloat(outlineWidth) + parseFloat(outlineOffset);
      const ring = button.getBoundingClientRect();
      // The list scrolls in the narrow layout, and the window's edge clips everything.
      const box = button.closest('ul')!.getBoundingClientRect();
      return Math.max(box.left - (ring.left - reach), box.top - (ring.top - reach), ring.bottom + reach - box.bottom, 0);
    });
    expect(clipped, `the focus ring is clipped by ${clipped}px at ${width}px`).toBe(0);
  }
});

/** A small sky: two Galaxies of linked Planets, sized and placed the way Rust would. */
function universe(): Universe {
  const planet = (id: number, name: string, galaxy: number, radius: number, position: [number, number, number]): Planet => ({
    id,
    name,
    source: id % 2 ? 'youtube' : 'spotify',
    minutes: radius * 100,
    plays: id + 1,
    radius,
    position,
    galaxy,
    interests: [],
    url: null,
    topItems: [],
  });
  return {
    filter: 'both',
    planets: [
      planet(0, 'Moth Season', 0, 4, [-14, 2, 0]),
      planet(1, 'Tape Hiss TV', 0, 2.6, [-22, 7, 3]),
      planet(2, 'Velvet Anchor', 0, 1.8, [-8, 10, -4]),
      planet(3, 'Cache Miss', 1, 3.4, [14, -3, 2]),
      planet(4, 'Binary Tide', 1, 2.2, [22, 3, -3]),
      planet(5, 'Tiny Compilers', 1, 1.2, [10, -11, 5]),
    ],
    links: [
      { a: 0, b: 1, strength: 0.6, shared: [] },
      { a: 0, b: 2, strength: 0.4, shared: ['dreamy'] },
      { a: 3, b: 4, strength: 0.5, shared: [] },
      { a: 3, b: 5, strength: 0.3, shared: [] },
      { a: 2, b: 4, strength: 0.13, shared: [] },
    ],
    galaxies: [
      { id: 0, name: 'dreamy', hue: 0.11, size: 3 },
      { id: 1, name: 'tinkering', hue: 0.73, size: 3 },
    ],
  };
}

const noUniverse: Universe = { filter: 'both', planets: [], links: [], galaxies: [] };
const withUniverse = (u: Universe = universe()) => ({ ...empty, get_universe: () => u });

/** Only one Source's Planets from `from`, numbered again, the way Rust answers for that filter. */
function only(source: Source, from = universe()): Universe {
  const planets = from.planets.filter((p) => p.source === source).map((p, id) => ({ ...p, id }));
  return { ...from, filter: source, planets, links: [] };
}

/** Answers each filter the way Rust does. Any filter left out has nothing in it. */
const withUniverses = (universes: Partial<Record<UniverseFilter, Universe>>) => ({
  ...empty,
  get_universe: ({ filter }: { filter: UniverseFilter }) => universes[filter] ?? noUniverse,
});

async function openUniverse(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Universe' }).click();
}

/** The canvas's own pixels, with the words over it hidden, decoded in the page so no library is needed. */
async function canvasPng(page: Page, canvas: Locator) {
  const hide = await page.addStyleTag({ content: '.bar, .legend, .labels { visibility: hidden; }' });
  const png = (await canvas.screenshot()).toString('base64');
  await hide.evaluate((style) => (style as Element).remove());
  return png;
}

/** What the canvas drew: how many colours, and the average colour round each point. */
async function readCanvas(page: Page, canvas: Locator, points: { x: number; y: number }[] = []) {
  const png = await canvasPng(page, canvas);
  return page.evaluate(
    async ({ png, points }) => {
      const image = new Image();
      image.src = `data:image/png;base64,${png}`;
      await image.decode();
      const canvas = document.createElement('canvas');
      canvas.width = image.width;
      canvas.height = image.height;
      const context = canvas.getContext('2d', { willReadFrequently: true })!;
      context.drawImage(image, 0, 0);
      const { data } = context.getImageData(0, 0, image.width, image.height);
      const colors = new Set<number>();
      for (let i = 0; i < data.length; i += 4) colors.add((data[i] << 16) | (data[i + 1] << 8) | data[i + 2]);
      const at = ({ x, y }: { x: number; y: number }) => {
        const sum = [0, 0, 0];
        for (let dy = -3; dy <= 3; dy += 1) {
          for (let dx = -3; dx <= 3; dx += 1) {
            const i = (Math.round(y + dy) * image.width + Math.round(x + dx)) * 4;
            for (let c = 0; c < 3; c += 1) sum[c] += data[i + c];
          }
        }
        return sum.map((v) => v / 49) as [number, number, number];
      };
      return { colors: colors.size, at: points.map(at) };
    },
    { png, points },
  );
}

/** How far apart two screenshots of a canvas are in a square in the middle, as an average out of 255. */
function differenceInMiddle(page: Page, a: string, b: string, share = 0.3) {
  return page.evaluate(
    async ({ a, b, share }) => {
      const read = async (png: string) => {
        const image = new Image();
        image.src = `data:image/png;base64,${png}`;
        await image.decode();
        const canvas = document.createElement('canvas');
        canvas.width = image.width;
        canvas.height = image.height;
        const context = canvas.getContext('2d', { willReadFrequently: true })!;
        context.drawImage(image, 0, 0);
        const side = Math.round(image.height * share);
        return context.getImageData((image.width - side) / 2, (image.height - side) / 2, side, side).data;
      };
      const [left, right] = [await read(a), await read(b)];
      let sum = 0;
      for (let i = 0; i < left.length; i += 1) sum += Math.abs(left[i] - right[i]);
      return sum / left.length;
    },
    { a, b, share },
  );
}

/** Two frames, so anything a click or a tab asked for is on the canvas. */
const settled = (page: Page) =>
  page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));

const hueOf = ([r, g, b]: number[]) => new Color(r / 255, g / 255, b / 255).getHSL({ h: 0, s: 0, l: 0 }).h;
const hueGap = (a: number, b: number) => Math.min(Math.abs(a - b), 1 - Math.abs(a - b));
const brightness = ([r, g, b]: number[]) => Math.max(r, g, b);
const pigment = (hue: number) => hueOf(galaxyColor(hue).map((c) => c * 255));

test('universe draws something', async ({ page }) => {
  const asked: { filter: UniverseFilter }[] = [];
  await installMockIpc(page, {
    ...empty,
    get_universe: (args) => {
      asked.push(args);
      return universe();
    },
  });

  await openUniverse(page);

  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Universe' })).toBeVisible();
  await expect(page.getByText('6 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Universe' })).toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('button', { name: 'Sources' })).not.toHaveAttribute('aria-current', 'page');
  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeHidden();
  await expect.poll(async () => (await readCanvas(page, canvas)).colors, { timeout: 40_000 }).toBeGreaterThan(50);
  expect(asked.map((a) => a.filter).sort()).toEqual(['both', 'spotify', 'youtube']);
});

/** Two Planets in their own Galaxies, side by side, with a Link through the middle of the view. */
function twoPlanets(strength: number | null = 0.6): Universe {
  const [a, b] = universe().planets;
  return {
    filter: 'both',
    planets: [
      { ...a, radius: 3, position: [-12, 0, 0], galaxy: 0 },
      { ...b, radius: 3, position: [12, 0, 0], galaxy: 1 },
    ],
    links: strength === null ? [] : [{ a: 0, b: 1, strength, shared: [] }],
    galaxies: [
      { id: 0, name: 'dreamy', hue: 0.11, size: 1 },
      { id: 1, name: 'tinkering', hue: 0.73, size: 1 },
    ],
  };
}

/** Fails unless each of twoPlanets() shows in its own Galaxy's colour on a dark sky. */
async function expectTwoPlanetsDrawn(page: Page) {
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  const box = (await canvas.boundingBox())!;
  const [x, y] = [box.width / 2, box.height / 2];
  // Rust fits the sky to the canvas's height, which puts each Planet this far from the middle.
  const across = box.height * 0.39;
  await expect(async () => {
    const { at } = await readCanvas(page, canvas, [{ x: x - across, y }, { x: x + across, y }, { x: 8, y: 8 }]);
    const [left, right, corner] = at;
    // Not exact, because bands, craters and light all shift a colour a little.
    expect(hueGap(hueOf(left), pigment(0.11)), `left Planet is ${left}`).toBeLessThan(0.07);
    expect(hueGap(hueOf(right), pigment(0.73)), `right Planet is ${right}`).toBeLessThan(0.07);
    expect(brightness(left), `left Planet is ${left}`).toBeGreaterThan(80);
    expect(brightness(right), `right Planet is ${right}`).toBeGreaterThan(80);
    expect(brightness(corner), `the corner is ${corner}`).toBeLessThan(60);
    // Software WebGL can spend many seconds compiling the shaders on a busy machine.
  }).toPass({ timeout: 40_000 });
}

test('Planets draw in their Galaxy colours', async ({ page }) => {
  await installMockIpc(page, withUniverse(twoPlanets()));

  await openUniverse(page);

  await expectTwoPlanetsDrawn(page);
});

test('a Link shows, and a stronger one shows more', async ({ page }) => {
  // The tabs here are only a way to see the same two Planets with each Link.
  await installMockIpc(
    page,
    withUniverses({ spotify: twoPlanets(null), youtube: twoPlanets(0.15), both: twoPlanets(0.9) }),
  );
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  const look = async (tab: string) => {
    await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: tab }).click();
    await settled(page);
    return canvasPng(page, canvas);
  };
  const strong = await look('Both');
  const none = await look('Spotify');
  const weak = await look('YouTube');

  // The whole picture, since the arc is thin and a Planet at each end of it is the same either way.
  const [toWeak, toStrong] = [await differenceInMiddle(page, none, weak, 1), await differenceInMiddle(page, none, strong, 1)];
  expect(toWeak).toBeGreaterThan(0);
  expect(toStrong).toBeGreaterThan(toWeak * 2);
});

test('Spotify and YouTube Planets look different, and no two look identical', async ({ page }) => {
  const named = (name: string, source: Source): Universe => {
    const [a] = universe().planets;
    return {
      filter: source,
      planets: [{ ...a, name, source, radius: 4, position: [0, 0, 0], galaxy: 0 }],
      links: [],
      galaxies: [{ id: 0, name: 'dreamy', hue: 0.11, size: 1 }],
    };
  };
  // Same place, size and Galaxy each time, so only what makes the Planet itself can differ.
  await installMockIpc(
    page,
    withUniverses({ spotify: named('Moth Season', 'spotify'), youtube: named('Moth Season', 'youtube'), both: named('Velvet Anchor', 'spotify') }),
  );
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  const look = async (tab: string) => {
    await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: tab }).click();
    await settled(page);
    return canvasPng(page, canvas);
  };
  const artist = await look('Spotify');
  const channel = await look('YouTube');
  const otherArtist = await look('Both');
  const again = await look('Spotify');

  expect(await differenceInMiddle(page, artist, again)).toBeLessThan(1);
  expect(await differenceInMiddle(page, artist, channel)).toBeGreaterThan(8);
  expect(await differenceInMiddle(page, artist, otherArtist)).toBeGreaterThan(4);
});

test('three.js loads only on the Universe screen', async ({ page }) => {
  const scripts: string[] = [];
  page.on('request', (request) => {
    if (request.resourceType() === 'script') scripts.push(request.url());
  });
  await installMockIpc(page, withUniverse());

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeVisible();
  const onSources = scripts.length;
  const hasThree = (url: string) => fetch(url).then(async (r) => (await r.text()).includes('WebGLRenderer'));
  for (const url of scripts) {
    expect(await page.evaluate(hasThree, url), `${url} holds three.js`).toBe(false);
  }

  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  expect(scripts.length).toBeGreaterThan(onSources);
});

test('the Universe draws under the app CSP', async ({ page }) => {
  const problems: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') problems.push(message.text());
  });
  page.on('pageerror', (error) => problems.push(error.message));
  const csp: string = tauriConfig.app.security.csp;
  await page.route('**/*', async (route) => {
    const response = await route.fetch();
    await route.fulfill({ response, headers: { ...response.headers(), 'content-security-policy': csp } });
  });
  await installMockIpc(page, withUniverse(twoPlanets()));

  await openUniverse(page);

  await expectTwoPlanetsDrawn(page);
  expect(problems).toEqual([]);
});

/** One Planet, which fills the middle of the view. The corners hold nothing but stars. */
function onePlanet(): Universe {
  return {
    filter: 'spotify',
    planets: [{ ...universe().planets[0], radius: 4, position: [0, 0, 0] }],
    links: [],
    galaxies: [{ id: 0, name: 'dreamy', hue: 0.11, size: 1 }],
  };
}

/** Moth Season and friends, with everything a panel can show. */
function detailed(): Universe {
  const sky = universe();
  sky.planets[0] = {
    ...sky.planets[0],
    interests: ['dream pop', 'shoegaze'],
    url: 'https://open.spotify.com/artist/abc',
    topItems: [
      { title: 'Low Tide', url: 'https://open.spotify.com/track/def', plays: 9 },
      { title: 'Paper Signal', url: null, plays: 4 },
    ],
  };
  return sky;
}

async function clickMiddleOfSky(page: Page) {
  const box = (await page.locator('canvas').boundingBox())!;
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
}

const panel = (page: Page) => page.getByRole('complementary');

type Box = { x: number; y: number; width: number; height: number };
const overlaps = (a: Box, b: Box) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

test('clicking a Planet opens its panel, and clicking empty sky does not', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await installMockIpc(page, withUniverse(onePlanet()));

  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);

  const box = (await canvas.boundingBox())!;
  await page.mouse.click(box.x + 8, box.y + 8);
  await expect(panel(page)).toHaveCount(0);
  await clickMiddleOfSky(page);
  await expect(panel(page).getByRole('heading', { name: 'Moth Season' })).toBeVisible();
  await expect(panel(page)).toContainText('7 hours on Spotify');
  await expect(panel(page)).toContainText('dreamy Galaxy');
});

test('hovering a Planet shows a tooltip, and leaving it takes it away', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await installMockIpc(page, withUniverse(onePlanet()));
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  const box = (await canvas.boundingBox())!;
  const tooltip = page.locator('.label.tooltip');

  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect(tooltip).toBeVisible();
  await expect(tooltip).toContainText('Moth Season');
  await expect(tooltip).toContainText('7 hours, dreamy Galaxy');

  await page.mouse.move(box.x + 8, box.y + 8);
  await expect(tooltip).toBeHidden();
});

/** Moves over the whole sky at once. Every spot a hover lands on, what was wrong with its tooltip. */
const hoverEverywhere = (page: Page) =>
  page.evaluate(() => {
    const canvas = document.querySelector('canvas')!;
    const sky = canvas.getBoundingClientRect();
    const covers = [...document.querySelectorAll('.title, .controls, .notice, .legend')].map((node) => node.getBoundingClientRect());
    const tip = document.querySelector<HTMLElement>('.label.tooltip')!;
    const problems: string[] = [];
    let hovers = 0;
    for (let y = sky.top + 1; y < sky.bottom; y += 12) {
      for (let x = sky.left + 1; x < sky.right; x += 12) {
        canvas.dispatchEvent(new PointerEvent('pointermove', { clientX: x, clientY: y, pointerType: 'mouse' }));
        if (canvas.style.cursor !== 'pointer') continue;
        hovers += 1;
        const at = tip.getBoundingClientRect();
        if (tip.hidden) problems.push(`hidden at ${x},${y}`);
        else if (at.left < sky.left || at.top < sky.top || at.right > sky.right || at.bottom > sky.bottom) problems.push(`off the sky at ${x},${y}`);
        else if (covers.some((c) => at.left < c.right && c.left < at.right && at.top < c.bottom && c.top < at.bottom)) problems.push(`under a box at ${x},${y}`);
      }
    }
    canvas.dispatchEvent(new PointerEvent('pointerleave'));
    return { hovers, problems };
  });

/** Planets all the way round the edge of the view, so some are behind each box over the sky. */
function rim(count: number): Universe {
  const sky = universe();
  const planets = Array.from({ length: count }, (_, i): Planet => {
    const turn = (i / count) * 2 * Math.PI;
    return { ...sky.planets[i % 2], id: i, name: `Creator ${i}`, radius: 5, position: [100 * Math.cos(turn), 100 * Math.sin(turn), 0], galaxy: i % 2 };
  });
  return { ...sky, planets, links: [] };
}

test('wherever a Planet is hovered, its tooltip is in the sky and clear of the boxes', async ({ page }) => {
  await installMockIpc(page, withUniverse(rim(60)));
  await page.setViewportSize({ width: 1200, height: 800 });
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);

  for (const [width, height] of [[1200, 800], [800, 600], [640, 480]]) {
    await page.setViewportSize({ width, height });
    await settled(page);
    await settled(page);
    const { hovers, problems } = await hoverEverywhere(page);
    expect(hovers, `nothing was hovered at ${width}x${height}`).toBeGreaterThan(20);
    expect(problems, `at ${width}x${height}`).toEqual([]);
  }
});

test('a Planet hovered from close up still gets its tooltip', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await installMockIpc(page, withUniverse(crowd(60)));
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Creator 0\b/ }).click();
  await page.getByRole('button', { name: 'Show 3D view' }).click();
  await settled(page);
  await settled(page);

  const box = (await canvas.boundingBox())!;
  const tooltip = page.locator('.label.tooltip');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect(tooltip).toContainText('Creator 0');
  for (let step = 0; step < 4; step += 1) {
    await page.mouse.wheel(0, -240);
    await settled(page);
    await settled(page);
    await page.mouse.move(box.x + box.width / 2 + step, box.y + box.height / 2);
    await expect(canvas).toHaveCSS('cursor', 'pointer');
    await expect(tooltip, `after ${step + 1} wheel steps`).toBeVisible();
    const at = (await tooltip.boundingBox())!;
    expect(at.x, 'left edge').toBeGreaterThanOrEqual(box.x);
    expect(at.y, 'top edge').toBeGreaterThanOrEqual(box.y);
    expect(at.x + at.width, 'right edge').toBeLessThanOrEqual(box.x + box.width);
    expect(at.y + at.height, 'bottom edge').toBeLessThanOrEqual(box.y + box.height);
  }
});

test('dragging turns the Universe and does not pick a Planet', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await installMockIpc(page, withUniverse(onePlanet()));
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  const box = (await canvas.boundingBox())!;
  const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  const before = await canvasPng(page, canvas);

  // It ends on the Planet, where a click would pick it. The stars show the turn.
  await page.mouse.move(middle.x + 150, middle.y + 40);
  await page.mouse.down();
  await page.mouse.move(middle.x, middle.y, { steps: 5 });
  await page.mouse.up();

  await expect.poll(async () => (await canvasPng(page, canvas)) === before).toBe(false);
  await expect(panel(page)).toHaveCount(0);
});

test('a hostile Planet name is only text, and it cannot take over the screen', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  const hostile = onePlanet();
  hostile.planets[0].name = `</script><img src=x onerror="window.ran = 1">${'long '.repeat(2000)}`;
  hostile.galaxies[0].name = hostile.planets[0].name;
  await installMockIpc(page, withUniverse(hostile));

  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  await clickMiddleOfSky(page);

  const heading = panel(page).getByRole('heading', { level: 2 });
  await expect(heading).toContainText('</script><img src=x');
  await expect(page.locator('main img')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).ran)).toBeUndefined();
  expect((await heading.boundingBox())!.height).toBeLessThan(200);
  const sideways = await page.evaluate(() => document.querySelector('main')!.scrollWidth - document.querySelector('main')!.clientWidth);
  expect(sideways).toBeLessThanOrEqual(0);
});

test('empty universe shows guidance', async ({ page }) => {
  await installMockIpc(page, withUniverse(noUniverse));

  await openUniverse(page);

  await expect(page.getByText('Nothing to draw yet.')).toBeVisible();
  await expect(page.getByText('Add your history on the Sources screen, or try the demo data there.')).toBeVisible();
  await expect(page.locator('canvas')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Show list' })).toHaveCount(0);
  await expect(page.getByRole('group', { name: 'Which Source to show' })).toHaveCount(0);
});

test('a Universe that cannot load shows why', async ({ page }) => {
  await installMockIpc(page, {
    ...empty,
    get_universe: () => {
      throw new Error('Rostery could not use its local database.');
    },
  });

  await openUniverse(page);

  await expect(page.getByRole('alert')).toContainText('could not use its local database');
});

/** Makes every request for a WebGL context fail, the way a computer with no graphics driver does. */
const noWebGL = (page: Page) =>
  page.addInitScript(() => {
    const real = HTMLCanvasElement.prototype.getContext as any;
    HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...rest: unknown[]) {
      return kind === 'webgl2' || kind === 'webgl' ? null : real.call(this, kind, ...rest);
    } as any;
  });

test('falls back to list without webgl', async ({ page }) => {
  await noWebGL(page);
  await installMockIpc(page, withUniverse(detailed()));

  await openUniverse(page);

  await expect(page.getByRole('alert')).toContainText('could not start 3D drawing');
  await expect(page.getByRole('alert')).toContainText('here is the list of Planets');
  await expect(page.getByText('6 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  await expect(page.getByRole('list', { name: 'Planets, largest first' }).getByRole('listitem')).toHaveCount(6);
  await expect(page.locator('canvas')).toHaveCount(0);
  // There is nothing to go back to, so no button offers it.
  await expect(page.getByRole('button', { name: 'Show 3D view' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Show list' })).toHaveCount(0);

  await page.getByRole('button', { name: /Moth Season/ }).click();
  await expect(panel(page).getByRole('heading', { name: 'Moth Season' })).toBeVisible();
});

test('a failure while drawing is not blamed on the computer', async ({ page }) => {
  const broken = universe();
  broken.links.push({ a: 0, b: 99, strength: 0.5, shared: [] });
  await installMockIpc(page, withUniverse(broken));

  await openUniverse(page);

  await expect(page.getByRole('alert')).toContainText('could not draw this Universe');
  await expect(page.getByRole('alert')).not.toContainText('3D drawing');
  // Its drawing context goes too, since a browser only allows so many.
  await expect(page.locator('canvas')).toHaveCount(0);
  // A Link to nothing still leaves the rest usable.
  await page.getByRole('button', { name: /Moth Season/ }).click();
  await expect(panel(page).getByRole('heading', { name: 'Moth Season' })).toBeVisible();
});

test('a renderer that cannot load shows the list and a plain message', async ({ page }) => {
  await page.route(/render-[^/]*\.js$/, (route) => route.abort());
  await installMockIpc(page, withUniverse());

  await openUniverse(page);

  const alert = page.getByRole('alert');
  await expect(alert).toContainText('The 3D view could not load, so here is the list of Planets');
  await expect(alert).not.toContainText(/import|fetch|module|load failed/i);
  await expect(page.getByRole('list', { name: 'Planets, largest first' }).getByRole('listitem')).toHaveCount(6);
  await expect(page.getByRole('button', { name: 'Show 3D view' })).toHaveCount(0);
});

test('when the drawing context is lost the screen says so, and it draws again when it comes back', async ({ page }) => {
  await installMockIpc(page, withUniverse(twoPlanets()));
  await openUniverse(page);
  await expectTwoPlanetsDrawn(page);
  const lose = await page
    .locator('canvas')
    .evaluateHandle((canvas: HTMLCanvasElement) => canvas.getContext('webgl2')!.getExtension('WEBGL_lose_context')!);

  await lose.evaluate((extension) => extension.loseContext());
  // If it never comes back, this is what stays on screen.
  await expect(page.getByRole('alert')).toContainText('lost its 3D drawing');
  await expect(page.getByRole('alert')).toContainText('use the list, or leave this page and come back');
  // The list still works while the picture is gone.
  await page.getByRole('button', { name: 'Show list' }).click();
  await expect(page.getByRole('list', { name: 'Planets, largest first' })).toBeVisible();
  await page.getByRole('button', { name: 'Show 3D view' }).click();

  await lose.evaluate((extension) => extension.restoreContext());
  await expect(page.getByRole('alert')).toBeHidden();
  await expectTwoPlanetsDrawn(page);
});

test('the words stay readable with a Planet behind them', async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 480 });
  await installMockIpc(page, withUniverse(onePlanet()));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();

  const luminance = (css: string) => {
    const { r, g, b } = new Color(css);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  for (const text of [
    page.getByRole('heading', { name: 'Universe' }),
    page.getByText('1 Planet, 0 Links, 1 Galaxy'),
    page.getByRole('button', { name: 'Show list' }),
    page.getByRole('button', { name: 'Both' }),
    page.getByText('Galaxies', { exact: true }),
  ]) {
    const { color, backing } = await text.evaluate((node) => ({
      color: getComputedStyle(node).color,
      backing: getComputedStyle(node.closest('.title, .controls, .legend')!).backgroundColor,
    }));
    // Solid, because anything the picture shows through would change the contrast from one frame to the next.
    expect(backing).toMatch(/^rgb\(/);
    const [light, dark] = [luminance(color), luminance(backing)].sort((a, b) => b - a);
    expect((light + 0.05) / (dark + 0.05)).toBeGreaterThanOrEqual(4.5);
  }
});

test('leaving the Universe frees its drawing context and coming back asks for fresh data', async ({ page }) => {
  let asked = 0;
  await installMockIpc(page, {
    ...empty,
    get_universe: () => {
      asked += 1;
      return universe();
    },
  });

  await page.goto('/');
  for (let visit = 0; visit < 3; visit += 1) {
    await page.getByRole('button', { name: 'Universe' }).click();
    await expect(page.locator('canvas')).toBeVisible();
    const canvas = await page.locator('canvas').elementHandle();
    await page.getByRole('button', { name: 'Sources' }).click();
    await expect(page.locator('canvas')).toHaveCount(0);
    // A browser only allows about 16 live contexts, so one left behind would cost the 17th visit.
    const lost = await canvas!.evaluate((c: HTMLCanvasElement) => c.getContext('webgl2')!.isContextLost());
    expect(lost).toBe(true);
  }

  await expect(page.getByRole('heading', { name: 'Add your history' })).toBeVisible();
  // One for each of the 3 filters, on each of the 3 visits.
  expect(asked).toBe(9);
});

test('an import keeps going while you look at the Universe', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 600 });
  let finish = () => {};
  await installMockIpc(page, {
    ...withUniverse(),
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () => new Promise((resolve) => (finish = () => resolve({ ...nothing, filesRead: 1, added: 5 }))),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');

  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  finish();
  await page.getByRole('button', { name: 'Sources' }).click();

  const status = page.getByRole('status');
  await expect(status).toHaveText('Added 5 Plays.');
  await expect(status).toBeInViewport();
});

test('the Universe says an import is running and draws the result when it ends', async ({ page }) => {
  let stored = noUniverse;
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    get_universe: () => stored,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () =>
      new Promise((resolve) => {
        finish = () => {
          stored = universe();
          resolve({ ...nothing, filesRead: 1, added: 5 });
        };
      }),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');
  await page.getByRole('button', { name: 'Universe' }).click();

  await expect(page.getByRole('status')).toContainText('Reading your history');
  await expect(page.getByText('Nothing to draw yet')).toBeHidden();

  finish();
  await expect(page.locator('canvas')).toBeVisible();
  await expect(page.getByText('6 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  await expect(page.getByText('Reading your history')).toBeHidden();
});

test('an import that fails while you look at the Universe says so there', async ({ page }) => {
  let fail = () => {};
  await installMockIpc(page, {
    ...empty,
    get_universe: () => noUniverse,
    'plugin:dialog|open': () => ['/d/notes.json'],
    import_paths: () =>
      new Promise((_, reject) => (fail = () => reject(new Error('Rostery did not find a Spotify or YouTube Export there.')))),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');

  fail();
  await expect(page.getByRole('alert')).toContainText('did not find a Spotify or YouTube Export');
  await expect(page.getByText('Nothing to draw yet')).toBeVisible();

  // Sources has it too, so it does not follow the person around after they have been there.
  await page.getByRole('button', { name: 'Sources' }).click();
  await expect(page.getByRole('alert')).toContainText('did not find a Spotify or YouTube Export');
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.getByText('Nothing to draw yet')).toBeVisible();
  await expect(page.getByRole('alert')).toBeHidden();
});

test('finishing an import keeps the canvas and the camera, and swaps nothing else out', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  let stored = universe();
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    get_universe: () => stored,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () =>
      new Promise((resolve) => {
        finish = () => {
          stored = universe();
          stored.planets.push({ ...stored.planets[5], id: 6, name: 'Late Arrival', radius: 0.6, position: [0, 0, 20] });
          stored.galaxies[1].size = 4;
          resolve({ ...nothing, filesRead: 1, added: 5 });
        };
      }),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');
  await page.getByRole('button', { name: 'Universe' }).click();
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);

  // Someone has turned the sky to look at something.
  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + 500, box.y + 300);
  await page.mouse.down();
  await page.mouse.move(box.x + 420, box.y + 260, { steps: 4 });
  await page.mouse.up();

  // A sky that has been let go drifts on for a moment, and that is not what this test is about. It slows by
  // the frame, not by the clock, and a busy machine can go 400 ms without one, so this counts frames.
  const label = page.locator('.label', { hasText: 'Moth Season' });
  const where = () => label.evaluate((node) => (node as HTMLElement).style.transform);
  await expect(async () => {
    const start = await where();
    await page.evaluate(() => new Promise((done) => {
      let left = 30;
      const step = () => (left-- ? requestAnimationFrame(step) : done(null));
      step();
    }));
    expect(await where()).toBe(start);
  }).toPass();
  const before = await where();
  const node = await canvas.elementHandle();
  await node!.evaluate((c) => ((c as any).marked = true));

  finish();
  await expect(page.getByText('7 Planets, 5 Links, 2 Galaxies')).toBeVisible();

  await settled(page);
  expect(await canvas.evaluate((c) => (c as any).marked)).toBe(true);
  // Rust moved nothing that was already there, so its name should not have moved on screen either.
  expect(await where()).toBe(before);
});

test('a sky nobody has touched fits what an import adds', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  let stored = universe();
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    get_universe: () => stored,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () =>
      new Promise((resolve) => {
        finish = () => {
          stored = universe();
          stored.planets.unshift({ ...stored.planets[0], id: 0, name: 'Far Favourite', radius: 4, position: [400, 0, 0] });
          stored.planets.forEach((p, id) => (p.id = id));
          stored.links = [];
          resolve({ ...nothing, filesRead: 1, added: 5 });
        };
      }),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);

  finish();
  await expect(page.getByText('7 Planets, 0 Links, 2 Galaxies')).toBeVisible();
  await settled(page);
  await expect(page.locator('.label', { hasText: 'Far Favourite' })).toBeInViewport();
});

test('a Planet stays open through an import, under whatever id it has now', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  let stored = detailed();
  let finish = () => {};
  await installMockIpc(page, {
    ...empty,
    get_universe: () => stored,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () =>
      new Promise((resolve) => {
        finish = () => {
          // Someone heavier arrives, so everyone's id moves down one.
          const older = detailed();
          stored = {
            ...older,
            planets: [
              { ...older.planets[0], id: 0, name: 'New Favourite', interests: [], url: null, topItems: [] },
              ...older.planets.map((p) => ({ ...p, id: p.id + 1 })),
            ],
            links: older.links.map((l) => ({ ...l, a: l.a + 1, b: l.b + 1 })),
          };
          resolve({ ...nothing, filesRead: 1, added: 5 });
        };
      }),
  });

  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();
  const heading = panel(page).getByRole('heading', { level: 2 });
  await expect(heading).toHaveText('Moth Season');

  finish();
  await expect(page.getByText('7 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  await expect(heading).toHaveText('Moth Season');
  await expect(panel(page)).toContainText('dream pop');
});

test('filter tabs change the planet count', async ({ page }) => {
  await installMockIpc(page, withUniverses({ both: universe(), spotify: only('spotify'), youtube: only('youtube') }));
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await expect(page.getByText('6 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  const tabs = page.getByRole('group', { name: 'Which Source to show' });
  await expect(tabs.getByRole('button', { name: 'Both' })).toHaveAttribute('aria-pressed', 'true');

  await canvas.evaluate((c) => ((c as any).marked = true));
  await tabs.getByRole('button', { name: 'YouTube' }).click();
  await expect(page.getByText('3 Planets, 0 Links, 2 Galaxies')).toBeVisible();
  await expect(tabs.getByRole('button', { name: 'YouTube' })).toHaveAttribute('aria-pressed', 'true');
  await expect(tabs.getByRole('button', { name: 'Both' })).toHaveAttribute('aria-pressed', 'false');
  await expect(page.locator('.label', { hasText: 'Tape Hiss TV' })).toBeVisible();
  await expect(page.locator('.label', { hasText: 'Moth Season' })).toHaveCount(0);

  await tabs.getByRole('button', { name: 'Spotify' }).click();
  await expect(page.locator('.label', { hasText: 'Moth Season' })).toBeVisible();

  await tabs.getByRole('button', { name: 'Both' }).click();
  await expect(page.getByText('6 Planets, 5 Links, 2 Galaxies')).toBeVisible();
  // The same canvas all along, so a change of filter cannot flash an empty one.
  expect(await canvas.evaluate((c) => (c as any).marked)).toBe(true);
});

test('a filter change frames the new Planets and a click still picks the right one', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  // Far outside anything the first sky drew, which a stale bounding sphere would miss.
  const far: Universe = {
    ...onePlanet(),
    filter: 'youtube',
    planets: [{ ...onePlanet().planets[0], name: 'Far Away', source: 'youtube', position: [900, 200, -300] }],
  };
  await installMockIpc(page, withUniverses({ both: universe(), spotify: only('spotify'), youtube: far }));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);

  await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: 'YouTube' }).click();
  await settled(page);
  await expect(page.locator('.label', { hasText: 'Far Away' })).toBeInViewport();

  await clickMiddleOfSky(page);
  await expect(panel(page).getByRole('heading', { name: 'Far Away' })).toBeVisible();
});

test('a filter change frames the new Planets even after someone has flown in close', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await installMockIpc(page, withUniverses({ both: universe(), spotify: only('spotify'), youtube: only('youtube') }));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();
  await page.getByRole('button', { name: 'Show 3D view' }).click();
  await settled(page);
  // Close up on one Planet, this one is far out of view.
  await expect(page.locator('.label:visible', { hasText: 'Binary Tide' })).toHaveCount(0);

  await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: 'YouTube' }).click();
  await settled(page);
  await expect(page.getByRole('complementary')).toHaveCount(0);
  for (const name of ['Tape Hiss TV', 'Cache Miss', 'Tiny Compilers']) {
    await expect(page.locator('.label:visible', { hasText: name })).toBeInViewport();
  }
});

test('a filter change closes the panel, since that Planet may not be in the new set', async ({ page }) => {
  await installMockIpc(page, withUniverses({ both: detailed(), spotify: only('spotify', detailed()), youtube: only('youtube') }));
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();
  await expect(panel(page)).toBeVisible();

  await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: 'Spotify' }).click();
  await expect(panel(page)).toHaveCount(0);
});

test('one source hides the other tab', async ({ page }) => {
  const spotifyOnly = only('spotify');
  await installMockIpc(page, withUniverses({ both: { ...spotifyOnly, filter: 'both' }, spotify: spotifyOnly, youtube: noUniverse }));
  await openUniverse(page);

  const tabs = page.getByRole('group', { name: 'Which Source to show' });
  await expect(tabs.getByRole('button', { name: 'Spotify' })).toBeVisible();
  await expect(tabs.getByRole('button', { name: 'Spotify' })).toHaveAttribute('aria-pressed', 'true');
  await expect(tabs.getByRole('button', { name: 'YouTube' })).toHaveCount(0);
  // Both would be the same picture again.
  await expect(tabs.getByRole('button', { name: 'Both' })).toHaveCount(0);
  await expect(page.getByText('3 Planets, 0 Links, 2 Galaxies')).toBeVisible();
});

test('list item opens the planet panel', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();

  await page.getByRole('button', { name: 'Show list' }).click();
  const list = page.getByRole('list', { name: 'Planets, largest first' });
  await expect(page.locator('canvas')).toBeHidden();
  const items = list.getByRole('listitem');
  await expect(items).toHaveCount(6);
  // Sorted by size, with what a person needs to tell them apart.
  await expect(items.first()).toContainText('Moth Season');
  await expect(items.first()).toContainText('7 hours');
  await expect(items.first()).toContainText('dreamy');
  await expect(items.nth(1)).toContainText('Cache Miss');
  await expect(items.nth(1)).toContainText('4 watches');

  await list.getByRole('button', { name: /Moth Season/ }).click();
  const details = panel(page);
  await expect(details.getByRole('heading', { name: 'Moth Season' })).toBeVisible();
  await expect(details).toContainText('7 hours on Spotify');
  await expect(details).toContainText('dreamy Galaxy');
  await expect(details.getByRole('listitem').filter({ hasText: 'dream pop' })).toBeVisible();
  await expect(details.getByRole('listitem').filter({ hasText: 'shoegaze' })).toBeVisible();
  // Item titles are on in the app, with a link for the ones that have one.
  await expect(details.getByRole('link', { name: 'Low Tide' })).toBeVisible();
  await expect(details.getByText('Paper Signal')).toBeVisible();
  await expect(list.getByRole('button', { name: /Moth Season/ })).toHaveAttribute('aria-current', 'true');

  await page.getByRole('button', { name: 'Show 3D view' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  await expect(details.getByRole('heading', { name: 'Moth Season' })).toBeVisible();
});

test('the panel says why Planets are linked, and opens a neighbour from there', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();

  const linked = panel(page).getByRole('button', { name: /Tape Hiss TV/ });
  await expect(linked).toContainText('shared Sessions');
  await expect(panel(page).getByRole('button', { name: /Velvet Anchor/ })).toContainText('shared Interests: dreamy');
  // Strongest first.
  await expect(panel(page).locator('.linked li').first()).toContainText('Tape Hiss TV');

  await linked.click();
  const heading = panel(page).getByRole('heading', { level: 2 });
  await expect(heading).toHaveText('Tape Hiss TV');
  await expect(heading).toBeFocused();
  await expect(panel(page)).toContainText('2 watches on YouTube');
});

test('following a Linked Planet from down the panel shows its name, with the focus on it', async ({ page }) => {
  const sky = detailed();
  // Both tall enough to scroll, so the panel's new content cannot pull the scroll back by being short.
  for (const planet of [sky.planets[0], sky.planets[2]]) {
    planet.topItems = Array.from({ length: 14 }, (_, i) => ({ title: `Track ${i}`, url: null, plays: 20 - i }));
  }
  await installMockIpc(page, withUniverse(sky));

  for (const [width, height] of [[640, 480], [1200, 500]]) {
    await page.setViewportSize({ width, height });
    await openUniverse(page);
    await page.getByRole('button', { name: 'Show list' }).click();
    await page.getByRole('button', { name: /Moth Season/ }).click();

    const aside = panel(page);
    const link = aside.getByRole('button', { name: /Velvet Anchor/ });
    await aside.evaluate((node) => (node.scrollTop = node.scrollHeight));
    await link.scrollIntoViewIfNeeded();
    expect(await aside.evaluate((node) => node.scrollTop), 'the panel has scrolled').toBeGreaterThan(0);
    await link.click();

    const heading = aside.getByRole('heading', { level: 2 });
    await expect(heading).toHaveText('Velvet Anchor');
    await expect(heading).toBeFocused();
    const [name, box] = [(await heading.boundingBox())!, (await aside.boundingBox())!];
    expect(name.y, `the name is above the panel at ${width}x${height}`).toBeGreaterThanOrEqual(box.y);
    expect(name.y + name.height, `the name is below the panel at ${width}x${height}`).toBeLessThanOrEqual(box.y + box.height);
  }
});

test('a right to left name stays clear of the close button', async ({ page }) => {
  const sky = detailed();
  sky.planets[0].name = 'موسيقى الليل الطويل جدا '.repeat(3);
  await installMockIpc(page, withUniverse(sky));

  for (const [width, height] of [[1200, 800], [640, 480]]) {
    await page.setViewportSize({ width, height });
    await openUniverse(page);
    await page.getByRole('button', { name: 'Show list' }).click();
    await page.getByRole('button', { name: /موسيقى/ }).click();

    const hits = await panel(page).evaluate((aside) => {
      const close = aside.querySelector('.close')!.getBoundingClientRect();
      const words = document.createRange();
      words.selectNodeContents(aside.querySelector('h2')!);
      return [...words.getClientRects()].filter((r) => r.left < close.right && close.left < r.right && r.top < close.bottom && close.top < r.bottom).length;
    });
    expect(hits, `at ${width}x${height}`).toBe(0);
  }
});

test('Escape closes the panel and returns focus to the list item', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  const item = page.getByRole('button', { name: /Velvet Anchor/ });
  await item.focus();
  await page.keyboard.press('Enter');
  const heading = panel(page).getByRole('heading', { level: 2 });
  await expect(heading).toHaveText('Velvet Anchor');
  await expect(heading).toBeFocused();

  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await expect(item).toBeFocused();
});

test('Escape closes the panel and returns focus to the canvas', async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 700 });
  await installMockIpc(page, withUniverse(onePlanet()));
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  await clickMiddleOfSky(page);
  await expect(panel(page)).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await expect(canvas).toBeFocused();

  // Escape with nothing open does nothing, and the close button does the same as Escape.
  await page.keyboard.press('Escape');
  await expect(canvas).toBeFocused();
  await settled(page);
  await clickMiddleOfSky(page);
  await panel(page).getByRole('button', { name: 'Close Planet details' }).click();
  await expect(panel(page)).toHaveCount(0);
  await expect(canvas).toBeFocused();
});

/** Where each top label is. The same camera puts every one of them in the same place. */
const topLabels = (page: Page) =>
  page.locator('.label:not(.tooltip, .chosen):visible').evaluateAll((nodes) =>
    Object.fromEntries(nodes.map((n) => [n.textContent, (n as HTMLElement).style.transform.match(/-?[\d.]+/g)!.map(Number)])),
  );

function expectSameView(after: Record<string, number[]>, before: Record<string, number[]>, what: string) {
  expect(Object.keys(after).sort(), what).toEqual(Object.keys(before).sort());
  for (const [name, [x, y]] of Object.entries(before)) {
    expect(Math.abs(after[name][0] - x), `${name} moved sideways ${what}`).toBeLessThanOrEqual(2);
    expect(Math.abs(after[name][1] - y), `${name} moved up or down ${what}`).toBeLessThanOrEqual(2);
  }
}

test('closing a panel goes back to the same view of the whole sky', async ({ page }) => {
  await installMockIpc(page, withUniverse());

  for (const [width, height] of [[640, 900], [500, 900], [1200, 800]]) {
    await page.setViewportSize({ width, height });
    await openUniverse(page);
    await expect(page.locator('canvas')).toBeVisible();
    await settled(page);
    await settled(page);
    const before = await topLabels(page);
    expect(Object.keys(before).length).toBeGreaterThan(0);

    await page.getByRole('button', { name: 'Show list' }).click();
    await page.getByRole('button', { name: /Moth Season/ }).click();
    await page.getByRole('button', { name: 'Show 3D view' }).click();
    await settled(page);
    // In flight, as most people see it, because a resize during a flight does not correct it.
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.keyboard.press('Escape');
    await expect(panel(page)).toHaveCount(0);
    await expect(async () => expectSameView(await topLabels(page), before, `after closing at ${width}x${height}`)).toPass({ timeout: 15_000 });
    await page.emulateMedia({ reducedMotion: 'reduce' });

    // The same when the sky was out of sight as the panel closed.
    await page.getByRole('button', { name: 'Show list' }).click();
    await page.getByRole('button', { name: /Moth Season/ }).click();
    await page.getByRole('button', { name: 'Show 3D view' }).click();
    await settled(page);
    await page.getByRole('button', { name: 'Show list' }).click();
    await page.keyboard.press('Escape');
    await expect(panel(page)).toHaveCount(0);
    await settled(page);
    await page.getByRole('button', { name: 'Show 3D view' }).click();
    await settled(page);
    await settled(page);
    expectSameView(await topLabels(page), before, `after closing in the list at ${width}x${height}`);
  }
});

test('a view of the whole sky fits again when the window changes after a panel closes', async ({ page }) => {
  await installMockIpc(page, withUniverse());
  await page.setViewportSize({ width: 1200, height: 800 });
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();
  await page.getByRole('button', { name: 'Show 3D view' }).click();
  await settled(page);
  await page.keyboard.press('Escape');
  await expect(panel(page)).toHaveCount(0);
  await settled(page);

  await page.setViewportSize({ width: 500, height: 900 });
  await settled(page);
  await settled(page);
  const after = await topLabels(page);
  // A sky nobody has moved, opened fresh in this window.
  await page.reload();
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);
  await settled(page);
  expectSameView(after, await topLabels(page), 'against a fresh sky in the new window');
});

test('the canvas says a list view is available', async ({ page }) => {
  await installMockIpc(page, withUniverse());
  await openUniverse(page);

  const canvas = page.getByRole('img', { name: /list view/ });
  await expect(canvas).toBeVisible();
  await expect(canvas).toHaveAttribute('tabindex', '0');
});

test('every control can be reached and used from the keyboard', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();

  const reached = new Set<string>();
  for (let press = 0; press < 40; press += 1) {
    await page.keyboard.press('Tab');
    const name = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement;
      return el === document.body ? '' : el.tagName === 'CANVAS' ? 'canvas' : (el.innerText || el.getAttribute('aria-label') || el.tagName).trim();
    });
    if (name) reached.add(name);
  }
  for (const name of ['Spotify', 'YouTube', 'Both', 'Show list', 'canvas', 'Galaxies']) {
    expect([...reached], `${name} was never reached`).toContain(name);
  }

  // The toggle works from the keyboard, and the list it opens is all buttons in reach.
  await page.getByRole('button', { name: 'Show list' }).focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('button', { name: 'Show 3D view' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(page.getByRole('button', { name: /Moth Season/ })).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(panel(page).getByRole('heading', { level: 2 })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(panel(page).getByRole('button', { name: 'Close Planet details' })).toBeFocused();
});

test('the arrow keys move the sky once the canvas has focus', async ({ page }) => {
  await installMockIpc(page, withUniverse());
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);
  const label = page.locator('.label', { hasText: 'Moth Season' });
  const before = await label.evaluate((node) => (node as HTMLElement).style.transform);

  await canvas.focus();
  await page.keyboard.press('ArrowLeft');
  await page.keyboard.press('ArrowLeft');

  await expect.poll(() => label.evaluate((node) => (node as HTMLElement).style.transform)).not.toBe(before);
});

test('a Source link opens in the system browser and leaves the window alone', async ({ page }) => {
  const opened: string[] = [];
  await installMockIpc(page, {
    ...withUniverse(detailed()),
    'plugin:opener|open_url': (args) => {
      opened.push(args.url);
      return null;
    },
  });
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();
  const url = page.url();
  // A popup can arrive after any check for one, so look at the thing that would have let it open.
  await page.evaluate(() => {
    window.addEventListener('click', (e) => ((window as any).kept = !e.defaultPrevented));
  });

  await panel(page).getByRole('link', { name: 'Open in Spotify' }).click();
  await expect.poll(() => opened).toEqual(['https://open.spotify.com/artist/abc']);
  expect(await page.evaluate(() => (window as any).kept)).toBe(false);
  await panel(page).getByRole('link', { name: 'Low Tide' }).click();
  await expect.poll(() => opened).toEqual(['https://open.spotify.com/artist/abc', 'https://open.spotify.com/track/def']);

  expect(page.url()).toBe(url);
  expect(page.context().pages()).toHaveLength(1);
  await expect(panel(page)).toBeVisible();
});

test('a link the opener refuses says what to do, not what the plugin said', async ({ page }) => {
  // tauri-plugin-opener's own words for a URL outside the capability's scope.
  const refusal = 'Not allowed to open url https://open.spotify.com/artist/abc';
  const logged: string[] = [];
  page.on('console', (message) => logged.push(message.text()));
  await installMockIpc(page, {
    ...withUniverse(detailed()),
    'plugin:opener|open_url': () => {
      throw new Error(refusal);
    },
  });
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();

  await panel(page).getByRole('link', { name: 'Open in Spotify' }).click();
  await expect(page.getByRole('alert')).toHaveText('Rostery could not open your browser. Check that you have a default browser set, then try again.');
  await expect(page.getByText('Not allowed to open url')).toHaveCount(0);
  // Whoever reads the log still gets the reason.
  expect(logged.join('\n')).toContain(refusal);
});

test('a Planet with no way back has no link, and says the right words when it has one', async ({ page }) => {
  const sky = detailed();
  sky.planets[1].url = 'https://www.youtube.com/channel/UCabc';
  await installMockIpc(page, withUniverse(sky));
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();

  await page.getByRole('button', { name: /Tape Hiss TV/ }).click();
  await expect(panel(page).getByRole('link', { name: 'Open on YouTube' })).toHaveAttribute('href', 'https://www.youtube.com/channel/UCabc');

  await page.getByRole('button', { name: /Cache Miss/ }).click();
  await expect(panel(page).getByRole('heading', { level: 2 })).toHaveText('Cache Miss');
  await expect(panel(page).getByRole('link')).toHaveCount(0);
});

test('the legend names the Galaxies and which Planet is which Source', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await installMockIpc(page, withUniverses({ both: universe(), spotify: only('spotify'), youtube: only('youtube') }));
  await openUniverse(page);

  const legend = page.locator('.legend');
  await expect(legend.getByRole('listitem')).toHaveCount(2);
  await expect(legend).toContainText('dreamy');
  await expect(legend).toContainText('tinkering');
  await expect(legend).toContainText('Banded is Spotify');
  await expect(legend).toContainText('Ringed is YouTube');

  // With one Source on screen there is nothing to tell apart.
  await page.getByRole('group', { name: 'Which Source to show' }).getByRole('button', { name: 'Spotify' }).click();
  await expect(legend).not.toContainText('Banded is Spotify');
});

/** Many Planets on a grid, with sizes that fall away, so the largest are known. */
function crowd(count: number): Universe {
  const sky = universe();
  const planets = Array.from({ length: count }, (_, i): Planet => ({
    ...sky.planets[i % 2],
    id: i,
    name: `Creator ${i}`,
    radius: 4 - (3.4 * i) / count,
    position: [(i % 15) * 16 - 112, (Math.floor(i / 15) % 10) * 16 - 72, (i % 7) * 6 - 18],
    galaxy: i % 2,
  }));
  return { ...sky, planets, links: [] };
}

test('150 Planets still carry at most 14 labels, and only the largest', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 800 });
  await installMockIpc(page, withUniverse(crowd(150)));
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);

  await expect(page.locator('.label')).toHaveCount(14);
  const named = await page.locator('.label:visible').allInnerTexts();
  expect(named.length).toBeGreaterThan(0);
  expect(named.length).toBeLessThanOrEqual(12);
  for (const name of named) expect(Number(name.split(' ')[1]), name).toBeLessThan(12);

  // Labels never sit on top of one another.
  const boxes = await page.locator('.label:visible').evaluateAll((nodes) => nodes.map((n) => n.getBoundingClientRect().toJSON()));
  for (const [i, a] of boxes.entries()) {
    for (const b of boxes.slice(i + 1)) {
      expect(a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom).toBe(false);
    }
  }
});

const oddNames = [
  'x'.repeat(10_000),
  'موسيقى الليل الطويل جدا '.repeat(40),
  '🪐🌙✨👩🏽‍🚀'.repeat(200),
  `Ends with an override ‮evil‬ ${'word '.repeat(300)}`,
];

function oddSky(): Universe {
  const sky = detailed();
  sky.planets.slice(0, oddNames.length).forEach((p, i) => (p.name = oddNames[i]));
  sky.galaxies[0].name = oddNames[0];
  sky.planets[0].interests = [oddNames[1], oddNames[2]];
  sky.planets[0].topItems = [{ title: oddNames[0], url: 'https://open.spotify.com/track/def', plays: 2 }];
  sky.links[0].shared = [oddNames[1]];
  return sky;
}

test('long, right to left and emoji names do not break the labels, panel or list', async ({ page }) => {
  await installMockIpc(page, withUniverse(oddSky()));
  await page.setViewportSize({ width: 1000, height: 700 });
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);

  for (const width of [1000, 640]) {
    await page.setViewportSize({ width, height: 700 });
    await settled(page);
    for (const label of await page.locator('.label:visible').all()) {
      const box = (await label.boundingBox())!;
      expect(box.width, 'a label stays narrow').toBeLessThanOrEqual(230);
      expect(box.height, 'a label stays on one line').toBeLessThanOrEqual(60);
    }

    await page.getByRole('button', { name: 'Show list' }).click();
    for (const row of await page.getByRole('list', { name: 'Planets, largest first' }).getByRole('listitem').all()) {
      expect((await row.boundingBox())!.height, 'a row keeps to a few lines').toBeLessThan(160);
    }
    await page.getByRole('button', { name: new RegExp(oddNames[0].slice(0, 50)) }).first().click();
    const details = panel(page);
    await expect(details).toBeVisible();
    expect((await details.getByRole('heading', { level: 2 }).boundingBox())!.height).toBeLessThan(200);
    const overflow = await page.evaluate(() => {
      const main = document.querySelector('main')!;
      const aside = document.querySelector('aside')!;
      return { main: main.scrollWidth - main.clientWidth, aside: aside.scrollWidth - aside.clientWidth, tall: aside.scrollHeight };
    });
    expect(overflow.main, `main scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    expect(overflow.aside, `the panel scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    // Long words in every field still leave the link at the end a short scroll away.
    expect(overflow.tall, `the panel is too tall at ${width}px`).toBeLessThan(1300);
    await details.getByRole('button', { name: 'Close Planet details' }).click();
    await page.getByRole('button', { name: 'Show 3D view' }).click();
  }

  // Right to left text reads right to left, whichever way the page runs.
  await page.getByRole('button', { name: 'Show list' }).click();
  const rtl = page.getByRole('button', { name: /موسيقى/ }).first();
  expect(await rtl.locator('.name').evaluate((n) => getComputedStyle(n).direction)).toBe('rtl');
  await rtl.click();
  expect(await panel(page).getByRole('heading', { level: 2 }).evaluate((n) => getComputedStyle(n).direction)).toBe('rtl');
});

test('labels never overlap the panel on small windows', async ({ page }) => {
  await installMockIpc(page, withUniverse(crowd(60)));
  await page.setViewportSize({ width: 1200, height: 800 });
  await openUniverse(page);
  await expect(page.locator('canvas')).toBeVisible();
  await settled(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: 'Creator 0', exact: false }).first().click();
  await page.getByRole('button', { name: 'Show 3D view' }).click();

  for (const [width, height] of [[1200, 800], [800, 600], [640, 480], [500, 500], [420, 480]]) {
    await page.setViewportSize({ width, height });
    await settled(page);
    await settled(page);
    const side = (await page.getByRole('complementary').boundingBox())!;
    expect(side.height).toBeGreaterThan(0);
    const labels = await page.locator('.label:visible').evaluateAll((nodes) => nodes.map((n) => n.getBoundingClientRect().toJSON()));
    for (const box of labels) {
      expect(overlaps(box, side), `a label at ${box.left},${box.top} is on the panel at ${width}x${height}`).toBe(false);
    }
    expect(overlaps((await page.locator('.bar').boundingBox())!, side), `the words are on the panel at ${width}x${height}`).toBe(false);
    // And there is no way for the page itself to scroll sideways.
    const sideways = await page.evaluate(() => document.querySelector('main')!.scrollWidth - document.querySelector('main')!.clientWidth);
    expect(sideways).toBeLessThanOrEqual(0);
  }
});

test('the words over the sky stay off the panel on short windows, however much they have to say', async ({ page }) => {
  await installMockIpc(page, {
    ...withUniverse(detailed()),
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () => new Promise(() => {}),
    'plugin:opener|open_url': () => {
      throw new Error('No application knows how to open this URL');
    },
  });

  for (const [width, height] of [[640, 480], [720, 520], [640, 560], [800, 600]]) {
    await page.setViewportSize({ width, height });
    await page.goto('/');
    await page.getByRole('button', { name: 'Choose file' }).click();
    await page.getByRole('button', { name: 'Universe' }).click();
    await expect(page.getByRole('status')).toContainText('Reading your history');
    await page.getByRole('button', { name: 'Show list' }).click();
    await page.getByRole('button', { name: /Moth Season/ }).click();
    await page.getByRole('button', { name: 'Show 3D view' }).click();
    await settled(page);

    const bar = page.locator('.bar');
    const aside = panel(page);
    const expectRoomForAll = async (when: string) => {
      const [words, side, sky] = [(await bar.boundingBox())!, (await aside.boundingBox())!, (await page.locator('.sky').boundingBox())!];
      expect(overlaps(words, side), `the words are on the panel at ${width}x${height} ${when}`).toBe(false);
      expect(overlaps(words, sky), `the words are on the sky at ${width}x${height} ${when}`).toBe(false);
      expect(sky.height, `the sky has no room at ${width}x${height} ${when}`).toBeGreaterThanOrEqual(80);
      expect(side.height, `the panel has no room at ${width}x${height} ${when}`).toBeGreaterThanOrEqual(100);
    };
    await expectRoomForAll('during an import');
    const name = await aside.getByRole('heading', { level: 2 }).evaluate((h2) => {
      const { left, top, width, height } = h2.getBoundingClientRect();
      return document.elementFromPoint(left + width / 2, top + height / 2) === h2;
    });
    expect(name, `the name is covered at ${width}x${height}`).toBe(true);

    await aside.getByRole('link', { name: 'Open in Spotify' }).click();
    const alert = page.getByRole('alert');
    await expect(alert).toContainText('could not open your browser');
    await expectRoomForAll('after a link failed');
    // The failure is on screen without scrolling the words.
    const [said, words] = [(await alert.boundingBox())!, (await bar.boundingBox())!];
    expect(said.y + said.height, `the failure is cut off at ${width}x${height}`).toBeLessThanOrEqual(words.y + words.height);
  }
});

/** With a Planet open on the app's smallest window, the list still shows whole rows and the row it opened from. */
async function expectListBesidePanel(page: Page) {
  const list = page.getByRole('list', { name: 'Planets, largest first' });
  await list.getByRole('button', { name: /Binary Tide/ }).click();
  const aside = panel(page);
  const heading = aside.getByRole('heading', { level: 2 });
  await expect(heading).toBeFocused();

  const scroll = (await page.locator('.scroll').boundingBox())!;
  const chosen = (await list.locator('[aria-current="true"]').boundingBox())!;
  expect(scroll.height, 'not one whole row shows').toBeGreaterThanOrEqual(chosen.height);
  expect(chosen.y, 'the chosen row is above the list').toBeGreaterThanOrEqual(scroll.y - 1);
  expect(chosen.y + chosen.height, 'the chosen row is below the list').toBeLessThanOrEqual(scroll.y + scroll.height + 1);
  expect(overlaps((await page.locator('.bar').boundingBox())!, (await aside.boundingBox())!), 'the words are on the panel').toBe(false);

  // Back up into the list by keyboard, and the row that gets the focus can be seen.
  await page.keyboard.press('Shift+Tab');
  const focused = (await page.locator('[data-planet]:focus').boundingBox())!;
  expect(focused.y, 'the focused row is above the list').toBeGreaterThanOrEqual(scroll.y - 1);
  expect(focused.y + focused.height, 'the focused row is below the list').toBeLessThanOrEqual(scroll.y + scroll.height + 1);
}

test('without WebGL on a small window, the list stays beside an open Planet', async ({ page }) => {
  await noWebGL(page);
  await installMockIpc(page, withUniverse(detailed()));
  await page.setViewportSize({ width: 640, height: 480 });
  await openUniverse(page);
  await expect(page.getByRole('alert')).toContainText('could not start 3D drawing');
  await expectListBesidePanel(page);
});

test('the list view on a small window stays beside an open Planet', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await page.setViewportSize({ width: 640, height: 480 });
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await expectListBesidePanel(page);
});

test('names in the list keep their room beside the panel', async ({ page }) => {
  await installMockIpc(page, withUniverse(detailed()));
  await page.setViewportSize({ width: 1200, height: 700 });
  await openUniverse(page);
  await page.getByRole('button', { name: 'Show list' }).click();
  await page.getByRole('button', { name: /Moth Season/ }).click();

  for (const width of [850, 900, 950, 1000, 1100, 1200]) {
    await page.setViewportSize({ width, height: 700 });
    await settled(page);
    const name = await page.locator('.list .name').first().evaluate((node) => node.getBoundingClientRect().width);
    expect(name, `at a window ${width}px wide`).toBeGreaterThanOrEqual(150);
  }
});

const labelPlace = (page: Page) =>
  page.locator('.label:visible').first().evaluate((node) => (node as HTMLElement).style.transform);

test('auto-rotation turns the sky until someone touches it', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.setViewportSize({ width: 800, height: 600 });
  await installMockIpc(page, withUniverse());
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  // It opens with one long approach, and turns once that is over.
  await page.waitForTimeout(2500);

  const first = await labelPlace(page);
  await expect.poll(() => labelPlace(page), { timeout: 15_000 }).not.toBe(first);

  const box = (await canvas.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 121, box.y + box.height / 2);
  await page.mouse.up();
  // A sky that has been let go drifts on for a moment. Turning by itself would never stop.
  await expect(async () => {
    const stopped = await labelPlace(page);
    await page.waitForTimeout(1500);
    expect(await labelPlace(page)).toBe(stopped);
  }).toPass({ timeout: 15_000 });
});

test('auto-rotation is off under prefers-reduced-motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 800, height: 600 });
  await installMockIpc(page, withUniverse());
  await openUniverse(page);
  const canvas = page.locator('canvas');
  await expect(canvas).toBeVisible();
  await settled(page);

  const [place, picture] = [await labelPlace(page), await canvasPng(page, canvas)];
  await page.waitForTimeout(1500);
  expect(await labelPlace(page)).toBe(place);
  expect(await canvasPng(page, canvas)).toBe(picture);
});

test('a file dropped on another screen opens Sources and imports there', async ({ page }) => {
  const imported: unknown[] = [];
  await installMockIpc(page, {
    ...withUniverse(),
    import_paths: (args) => {
      imported.push(args);
      return { ...nothing, filesRead: 1, added: 5 };
    },
  });

  await page.goto('/');
  await dragFile(page);
  await page.getByRole('button', { name: 'Universe' }).click();
  await expect(page.locator('canvas')).toBeVisible();

  await emitEvent(page, 'tauri://drag-drop', { paths: ['/d/first.zip'], position: { x: 9, y: 9 } });

  await expect(page.getByRole('button', { name: 'Sources' })).toHaveAttribute('aria-current', 'page');
  const status = page.getByRole('status');
  await expect(status).toHaveText('Added 5 Plays.');
  await expect(status).toBeInViewport();
  await expect(page.locator('.import')).not.toHaveClass(/over/);
  expect(imported).toEqual([{ paths: ['/d/first.zip'] }]);
});

/** A finished Roast, the way Rust stores it. */
const aRoast: StoredRoast = {
  id: 7,
  createdAt: 1_790_000_000,
  providerLabel: 'Claude Code',
  roast: {
    headline: 'Forty-One Slow Thursdays',
    opening: 'You played Slow Thursday 41 times in a row. Sit down.',
    burns: [
      { title: 'The Monday Problem', body: 'You played it on a Monday. It did not help.' },
      { title: 'Through the Wall', body: 'Someone next door hates Thursdays now.' },
      { title: 'Recall Notice', body: 'Slow Thursday is being withdrawn.\n\nStay where you are.' },
    ],
    verdict: {
      archetype: 'The Night Clerk',
      score: 34,
      sentence: 'You are a menace, and I would sit with you again.',
      evidence: ['41 plays in a row', '31 percent after midnight'],
    },
    galaxyNames: [{ id: 0, name: 'The Quiet Room' }],
  },
};
const someHistory: Totals = {
  spotify: { plays: 944, minutes: 3000, creators: 30, firstPlay: 1_790_000_000, lastPlay: 1_799_000_000 },
  youtube: none,
};
const withHistory = { totals: () => someHistory, has_demo_data: () => false, latest_roast: () => null };
const DISCLOSURE = 'Your Digest goes to Anthropic through Claude Code.';
const roastButton = (page: Page) => page.getByRole('button', { name: /^Roast me/ });

async function openRoast(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Roast' })).toBeVisible();
}

/** A `generate_roast` that stays running until the test says how it ends. */
function controlledRoast() {
  const run = {
    calls: 0,
    /** What `latest_roast` answers, which only changes when a Roast finishes, as in Rust. */
    saved: null as StoredRoast | null,
    finish: (_result: StoredRoast = aRoast) => {},
    fail: (_message: string) => {},
    handler: () => {
      run.calls += 1;
      return new Promise((resolve, reject) => {
        run.finish = (result = aRoast) => {
          run.saved = result;
          resolve({ roast: result, notSaved: null });
        };
        run.fail = (message) => reject(new Error(message));
      });
    },
  };
  return run;
}

test('roast renders', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await openRoast(page);

  // The person reads where their Digest goes before they can send it.
  const line = page.getByText(DISCLOSURE);
  await expect(line).toBeVisible();
  const button = roastButton(page);
  await expect(button).toHaveText('Roast me');
  expect((await line.boundingBox())!.y).toBeLessThan((await button.boundingBox())!.y);
  expect(run.calls).toBe(0);

  await button.click();
  const status = page.getByRole('status');
  await expect(status).toHaveText('Reading your history');
  await expect(page.getByText('This can take a minute or two.')).toBeVisible();
  await expect.poll(() => run.calls).toBe(1);
  await emitEvent(page, 'roast-progress', 'Writing the Burns');
  await expect(status).toHaveText('Writing the Burns');

  run.finish();
  const { roast } = aRoast;
  await expect(page.getByRole('heading', { level: 2, name: roast.headline })).toBeVisible();
  await expect(page.getByText(roast.opening)).toBeVisible();
  await expect(page.getByText(/^Written by Claude Code on /)).toBeVisible();
  for (const burn of roast.burns) {
    await expect(page.getByRole('heading', { level: 3, name: burn.title })).toBeVisible();
  }
  await expect(page.getByText('It did not help.')).toBeVisible();
  const verdict = page.getByRole('region', { name: 'Verdict' });
  await expect(verdict.getByText(roast.verdict.archetype)).toBeVisible();
  await expect(verdict.getByText('34', { exact: true })).toBeVisible();
  await expect(verdict.getByText('out of 100')).toBeVisible();
  await expect(verdict.getByText(roast.verdict.sentence)).toBeVisible();
  await expect(verdict.getByRole('listitem')).toHaveText(roast.verdict.evidence);

  // Focus stays on the button and nothing else speaks up, so this is how a screen reader hears of it.
  await expect(status).toHaveText('Your Roast is ready.');
  await expect(page.getByText('This can take a minute or two.')).toBeHidden();
  await expect(button).toHaveText('Roast me again');
  await expect(line).toBeVisible();
});

test('the last Roast is there when the screen opens', async ({ page }) => {
  await installMockIpc(page, { ...withHistory, latest_roast: () => aRoast });
  await openRoast(page);

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(roastButton(page)).toHaveText('Roast me again');
});

test('roast error keeps previous', async ({ page }) => {
  const message = 'Claude Code is not logged in. Run claude in a terminal and log in with /login, then try again.';
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, latest_roast: () => aRoast, generate_roast: run.handler });
  await openRoast(page);
  const headline = page.getByRole('heading', { level: 2, name: aRoast.roast.headline });
  await expect(headline).toBeVisible();

  await roastButton(page).click();
  // The last Roast stays readable while a new one runs.
  await expect(page.getByRole('status')).toHaveText('Reading your history');
  await expect(headline).toBeVisible();
  await expect.poll(() => run.calls).toBe(1);
  run.fail(message);

  await expect(page.getByRole('alert')).toHaveText(message);
  await expect(headline).toBeVisible();
  await expect(page.getByRole('region', { name: 'Verdict' }).getByText('34', { exact: true })).toBeVisible();
  await expect(page.getByRole('status')).toBeEmpty();
  await expect(roastButton(page)).not.toHaveAttribute('aria-disabled', 'true');

  // Trying again clears the message, and a good run replaces the old Roast.
  await roastButton(page).click();
  await expect(page.getByRole('alert')).toBeHidden();
  await expect.poll(() => run.calls).toBe(2);
  run.finish({ ...aRoast, id: 8, roast: { ...aRoast.roast, headline: 'A Newer One' } });
  await expect(page.getByRole('heading', { level: 2, name: 'A Newer One' })).toBeVisible();
  await expect(headline).toBeHidden();
});

test('an error from before a Roast is still there on coming back', async ({ page }) => {
  await installMockIpc(page, {
    ...withHistory,
    generate_roast: () => {
      throw new Error('Claude Code did not finish within 5 minutes, so Rostery stopped it.');
    },
  });
  await openRoast(page);
  await roastButton(page).click();
  await expect(page.getByRole('alert')).toContainText('did not finish');

  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect(page.getByRole('alert')).toContainText('did not finish');
});

test('with no history the Roast screen asks for some in place of offering to call a Provider', async ({ page }) => {
  let calls = 0;
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    generate_roast: () => (calls += 1),
  });
  await openRoast(page);

  await expect(page.getByText('There is nothing to roast yet.')).toBeVisible();
  await expect(roastButton(page)).toHaveCount(0);
  await expect(page.getByText(DISCLOSURE)).toHaveCount(0);

  await page.getByRole('button', { name: 'Go to Sources' }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Sources' })).toBeVisible();
  expect(calls).toBe(0);
});

test('clearing demo data takes its Roast off the Roast screen', async ({ page }) => {
  let demo = true;
  const nothingStored: Totals = { spotify: none, youtube: none };
  await installMockIpc(page, {
    totals: () => (demo ? someHistory : nothingStored),
    has_demo_data: () => demo,
    latest_roast: () => (demo ? aRoast : null),
    clear_demo: () => {
      demo = false;
      return nothingStored;
    },
  });
  await openRoast(page);
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();

  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Clear demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect(page.getByText('There is nothing to roast yet.')).toBeVisible();
  await expect(page.locator('article')).toHaveCount(0);
  await expect(roastButton(page)).toHaveCount(0);
});

test('a Roast screen that cannot read what is stored says why, then recovers', async ({ page }) => {
  let broken = true;
  await installMockIpc(page, {
    has_demo_data: () => false,
    latest_roast: () => {
      if (broken) throw new Error('Rostery could not use its local database.');
      return null;
    },
    totals: () => {
      if (broken) throw new Error('Rostery could not use its local database.');
      return someHistory;
    },
    load_demo: () => {
      broken = false;
      return someHistory;
    },
  });
  await openRoast(page);
  await expect(page.locator('main [role="alert"]:visible')).toContainText('could not use its local database');
  await expect(roastButton(page)).toHaveCount(0);

  // Sources finishing a change asks again, and the old message goes with the failure it was about.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Try demo data' }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(roastButton(page)).toBeVisible();
  await expect(page.locator('main [role="alert"]:visible')).toHaveCount(0);
});

const UNREADABLE = 'Rostery could not read your last Roast. You can write a new one, and it takes the old one\'s place.';

test('a stored Roast that cannot be read keeps the button, says so and can be retried', async ({ page }) => {
  let broken = true;
  await installMockIpc(page, {
    ...withHistory,
    latest_roast: () => {
      if (broken) throw new Error(UNREADABLE);
      return aRoast;
    },
  });
  await openRoast(page);

  await expect(page.getByRole('alert')).toHaveText(UNREADABLE);
  await expect(roastButton(page)).toHaveText('Roast me');
  await expect(page.getByText(DISCLOSURE)).toBeVisible();

  const retry = page.getByRole('button', { name: 'Try again' });
  await retry.click();
  await settled(page);
  await expect(page.getByRole('alert')).toHaveText(UNREADABLE);

  broken = false;
  await retry.focus();
  await page.keyboard.press('Enter');
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(retry).toHaveCount(0);
  // Its button is gone, so focus goes to the one that is left and not to the page.
  await expect(roastButton(page)).toBeFocused();
});

test('a new Roast takes the place of one that could not be read', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, {
    ...withHistory,
    generate_roast: run.handler,
    latest_roast: () => {
      throw new Error(UNREADABLE);
    },
  });
  await openRoast(page);
  await expect(page.getByRole('alert')).toHaveText(UNREADABLE);

  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  run.finish();

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Try again' })).toHaveCount(0);
});

test('history that cannot be read can be retried on the Roast screen', async ({ page }) => {
  let broken = true;
  await installMockIpc(page, {
    ...withHistory,
    totals: () => {
      if (broken) throw new Error('Rostery could not use its local database.');
      return someHistory;
    },
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.locator('main [role="alert"]:visible')).toContainText('could not use its local database');
  await expect(roastButton(page)).toHaveCount(0);

  broken = false;
  await page.getByRole('button', { name: 'Try again' }).click();

  await expect(roastButton(page)).toBeVisible();
  await expect(page.locator('main [role="alert"]:visible')).toHaveCount(0);
});

test('history that arrives while the Roast screen is open makes the button appear', async ({ page }) => {
  let stored: Totals = { spotify: none, youtube: none };
  let finish = () => {};
  await installMockIpc(page, {
    totals: () => stored,
    has_demo_data: () => false,
    latest_roast: () => null,
    'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
    import_paths: () =>
      new Promise((resolve) => {
        finish = () => {
          stored = someHistory;
          resolve({ ...nothing, filesRead: 1, added: 944 });
        };
      }),
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('status')).toContainText('Reading your history');

  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  // An import is running, so asking for history would be wrong.
  await expect(page.getByRole('status')).toHaveText('Reading your history. This screen will update when that finishes.');
  await expect(page.getByText('There is nothing to roast yet.')).toHaveCount(0);
  finish();

  await expect(roastButton(page)).toBeVisible();
  await expect(page.getByText('There is nothing to roast yet.')).toHaveCount(0);
  await expect(page.getByRole('status')).toBeEmpty();
});

for (const [name, stored] of [
  ['no', { spotify: none, youtube: none }],
  ['older', someHistory],
] as const) {
  test(`an import that fails while the Roast screen is open says so, with ${name} history`, async ({ page }) => {
    let fail = (_: Error) => {};
    await installMockIpc(page, {
      totals: () => stored,
      has_demo_data: () => false,
      latest_roast: () => null,
      'plugin:dialog|open': () => ['/d/my_spotify_data.zip'],
      import_paths: () => new Promise((_, reject) => (fail = reject)),
    });
    await page.goto('/');
    await page.getByRole('button', { name: 'Choose file' }).click();
    await page.getByRole('button', { name: 'Roast', exact: true }).click();
    await expect(page.getByRole('status')).toContainText('Reading your history');

    fail(new Error('Rostery cannot read that file.'));

    await expect(page.locator('main [role="alert"]:visible')).toHaveText('Rostery cannot read that file.');
    await expect(page.getByRole('status')).toBeEmpty();
    if (stored === someHistory) await expect(roastButton(page)).toBeVisible();
    else await expect(page.getByText('There is nothing to roast yet.')).toBeVisible();
  });
}

test('a running Roast survives moving between screens and its result appears on coming back', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler, latest_roast: () => run.saved });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  await emitEvent(page, 'roast-progress', 'Going through your late nights');

  // Away and back: still running, with nothing started twice.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Sources' })).toBeVisible();
  await emitEvent(page, 'roast-progress', 'Writing the Burns');
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect(page.locator('main').getByRole('status').filter({ hasText: 'Writing the Burns' })).toBeVisible();
  await expect(roastButton(page)).toHaveAttribute('aria-disabled', 'true');
  expect(run.calls).toBe(1);

  // It lands while the person is away, and is there when they return.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  run.finish();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(roastButton(page)).not.toHaveAttribute('aria-disabled', 'true');
  expect(run.calls).toBe(1);
});

test('a Roast that lands while the screen is open appears without leaving', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(roastButton(page)).toHaveAttribute('aria-disabled', 'true');

  run.finish();

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
});

test('an old answer about the last Roast cannot replace one that just finished', async ({ page }) => {
  const run = controlledRoast();
  let answer = (_: StoredRoast | null) => {};
  let asked = 0;
  const older: StoredRoast = { ...aRoast, id: 1, roast: { ...aRoast.roast, headline: 'An Older One' } };
  await installMockIpc(page, {
    ...withHistory,
    generate_roast: run.handler,
    latest_roast: () => {
      asked += 1;
      return asked === 1 ? null : new Promise((resolve) => (answer = resolve));
    },
  });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);

  // The person comes back and the screen asks, then the Roast lands, then the slow answer arrives.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect.poll(() => asked).toBe(2);
  run.finish();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  answer(older);
  await settled(page);

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByText('An Older One')).toHaveCount(0);
});

test('a Roast cannot be started twice, and focus stays on its button', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await openRoast(page);
  const button = roastButton(page);
  await button.focus();
  await page.keyboard.press('Enter');
  await expect.poll(() => run.calls).toBe(1);

  await expect(button).toBeFocused();
  await expect(button).toHaveAttribute('aria-disabled', 'true');
  await page.keyboard.press('Enter');
  await button.dispatchEvent('click');
  await settled(page);
  expect(run.calls).toBe(1);

  run.finish();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(button).toBeFocused();
  await expect(button).not.toHaveAttribute('aria-disabled', 'true');
});

test('the message for a second Roast from the app itself is shown', async ({ page }) => {
  // The window was reloaded while a Roast ran, so only Rust still knows about it.
  const message = 'A Roast is already running. Its result will show on the Roast screen when it is done.';
  await installMockIpc(page, {
    ...withHistory,
    latest_roast: () => aRoast,
    generate_roast: () => {
      throw new Error(message);
    },
  });
  await openRoast(page);
  await roastButton(page).click();

  await expect(page.getByRole('alert')).toHaveText(message);
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
});

test('a progress phrase that arrives after the Roast finished is ignored', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  run.finish();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();

  await emitEvent(page, 'roast-progress', 'Working out your score');
  await settled(page);

  await expect(page.getByRole('status')).toHaveText('Your Roast is ready.');
});

test('saying a Roast is ready stops once it is old news', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler, latest_roast: () => run.saved });
  await openRoast(page);
  const status = page.getByRole('status');

  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  run.finish();
  await expect(status).toHaveText('Your Roast is ready.');

  // The next run takes over the region, and a failure leaves it empty.
  await roastButton(page).click();
  await expect(status).toHaveText('Reading your history');
  await expect.poll(() => run.calls).toBe(2);
  run.fail('Claude Code did not finish.');
  await expect(page.getByRole('alert')).toHaveText('Claude Code did not finish.');
  await expect(status).toBeEmpty();

  // Coming back to a Roast that landed earlier is not news either.
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(3);
  run.finish();
  await expect(status).toHaveText('Your Roast is ready.');
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(status).toBeEmpty();
});

test('a Roast is only ever text, however hostile', async ({ page }) => {
  const attack = '</script><img src=x onerror="window.ran = 1"><b>bold</b>';
  const hostile: StoredRoast = {
    ...aRoast,
    providerLabel: attack,
    roast: {
      headline: attack,
      opening: attack,
      burns: [{ title: attack, body: attack }],
      verdict: { archetype: attack, score: 34, sentence: attack, evidence: [attack] },
      galaxyNames: [],
    },
  };
  await installMockIpc(page, { ...withHistory, latest_roast: () => hostile });
  await openRoast(page);

  await expect(page.getByRole('heading', { level: 2 }).first()).toHaveText(attack);
  await expect(page.locator('main img')).toHaveCount(0);
  await expect(page.locator('article b')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).ran)).toBeUndefined();
});

test('a reply as long as validation allows stays inside the screen at every width', async ({ page }) => {
  // The caps in roast.rs, with no spaces to wrap at, plus text that runs right to left.
  const word = (n: number) => 'w'.repeat(n);
  const burn = { title: word(160), body: word(1500) };
  const long: StoredRoast = {
    ...aRoast,
    roast: {
      headline: word(160),
      opening: word(1500),
      burns: [...Array.from({ length: 11 }, () => burn), { title: 'مرحبا بالعالم', body: 'مرحبا 日本語 👩‍🚀 '.repeat(90) }],
      verdict: { archetype: word(160), score: 100, sentence: word(400), evidence: Array.from({ length: 8 }, () => word(400)) },
      galaxyNames: [],
    },
  };
  await installMockIpc(page, { ...withHistory, latest_roast: () => long });
  await openRoast(page);
  await expect(page.getByRole('region', { name: 'Verdict' })).toBeVisible();

  for (const width of [1200, 800, 721, 720, 500, 320]) {
    await page.setViewportSize({ width, height: 800 });
    const problems = await page.evaluate(() => {
      const main = document.querySelector('main')!;
      const edge = main.getBoundingClientRect().right;
      const past = [...main.querySelectorAll('h1, h2, h3, p, li')].filter((node) => node.getBoundingClientRect().right > edge + 0.5);
      return { sideways: main.scrollWidth - main.clientWidth, past: past.length };
    });
    expect(problems.sideways, `main scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    expect(problems.past, `text leaves the screen at ${width}px`).toBe(0);
  }
});

test('the score dial draws the share the Verdict gave', async ({ page }) => {
  await installMockIpc(page, { ...withHistory, latest_roast: () => aRoast });
  await openRoast(page);

  const arc = page.getByRole('region', { name: 'Verdict' }).locator('.arc');
  await expect(arc).toHaveAttribute('stroke-dasharray', '34 100');
});

test('the dial moves while a Roast runs and when a Verdict lands, unless motion is turned down', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);

  const sweep = page.locator('.progress .arc');
  const offset = () => sweep.evaluate((arc) => getComputedStyle(arc).strokeDashoffset);
  const first = await offset();
  await expect.poll(offset).not.toBe(first);

  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(sweep).toHaveCSS('animation-name', 'none');

  await page.emulateMedia({ reducedMotion: 'no-preference' });
  run.finish();
  const reading = page.getByRole('region', { name: 'Verdict' }).locator('.arc');
  await expect(reading).not.toHaveCSS('animation-name', 'none');
  await expect(reading).toHaveAttribute('stroke-dasharray', '34 100');
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await expect(reading).toHaveCSS('animation-name', 'none');
});

test('every screen opens at its top, wherever the last one was scrolled to', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 500 });
  await installMockIpc(page, { ...withHistory, latest_roast: () => aRoast });
  await openRoast(page);
  await expect(page.getByRole('region', { name: 'Verdict' })).toBeVisible();
  const main = page.locator('main');
  const scrollTo = (y: number) => main.evaluate((el, y) => (el.scrollTop = y), y);
  const scrolled = () => main.evaluate((el) => el.scrollTop);

  // Read a Roast to the end, then move on.
  await scrollTo(100_000);
  expect(await scrolled()).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Sources' })).toBeInViewport();
  expect(await scrolled()).toBe(0);

  // And the other way, to a screen seen before.
  await scrollTo(300);
  expect(await scrolled()).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Roast' })).toBeInViewport();
  await expect(roastButton(page)).toBeInViewport();
  expect(await scrolled()).toBe(0);
});

test('with no history but a stored Roast, the screen does not say there is nothing to roast', async ({ page }) => {
  await installMockIpc(page, { ...empty, latest_roast: () => aRoast });
  await openRoast(page);

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByText('There is nothing to roast yet.')).toHaveCount(0);
  await expect(page.getByText('Add your history on the Sources screen to write a new Roast.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Go to Sources' })).toBeVisible();
});

test('an old error goes when the history it was about does', async ({ page }) => {
  let demo = true;
  const nothingStored: Totals = { spotify: none, youtube: none };
  await installMockIpc(page, {
    totals: () => (demo ? someHistory : nothingStored),
    has_demo_data: () => demo,
    latest_roast: () => null,
    generate_roast: () => {
      throw new Error('Claude Code is not logged in.');
    },
    clear_demo: () => {
      demo = false;
      return nothingStored;
    },
  });
  await openRoast(page);
  await roastButton(page).click();
  await expect(page.getByRole('alert')).toHaveText('Claude Code is not logged in.');

  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Clear demo data' }).click();
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeHidden();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect(page.getByText('There is nothing to roast yet.')).toBeVisible();
  await expect(page.locator('main [role="alert"]:visible')).toHaveCount(0);
});

test('Go to Sources keeps keyboard focus', async ({ page }) => {
  await installMockIpc(page, { ...empty, latest_roast: () => null });
  await openRoast(page);
  await page.getByRole('button', { name: 'Go to Sources' }).focus();
  await page.keyboard.press('Enter');

  await expect(page.getByRole('heading', { level: 1, name: 'Sources' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sources', exact: true })).toBeFocused();
});

test('right to left Burns and evidence start at the right', async ({ page }) => {
  const arabic = 'مرحبا بالعالم';
  const roast = {
    ...aRoast,
    roast: {
      ...aRoast.roast,
      burns: [{ title: arabic, body: arabic }],
      verdict: { ...aRoast.roast.verdict, evidence: [arabic] },
    },
  };
  await installMockIpc(page, { ...withHistory, latest_roast: () => roast });
  await openRoast(page);
  await expect(page.getByRole('region', { name: 'Verdict' })).toBeVisible();

  for (const selector of ['.burns h3', '.burns p', '.verdict ul li']) {
    const node = page.locator(selector).first();
    // WebKit's `li` alignment comes from the parent's direction unless told otherwise.
    await expect(node, selector).not.toHaveCSS('text-align', 'left');
    await expect(node, selector).toHaveCSS('direction', 'rtl');
  }
});

test('while totals load there is a word about it, and nothing jumps when they arrive', async ({ page }) => {
  let answer = (_: Totals) => {};
  await installMockIpc(page, {
    has_demo_data: () => false,
    latest_roast: () => aRoast,
    totals: () => new Promise((resolve) => (answer = resolve)),
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  const status = page.locator('main').getByRole('status').filter({ hasText: 'Looking at what is stored' });
  await expect(status).toBeVisible();
  // The Roast waits for the button above it, so the button cannot push it down.
  await expect(page.locator('article')).toHaveCount(0);

  answer(someHistory);
  const article = page.locator('article');
  await expect(article).toBeVisible();
  await expect(status).toHaveCount(0);
  const top = (await article.boundingBox())!.y;
  await settled(page);
  expect((await article.boundingBox())!.y).toBe(top);

  // Coming back knows at once, with the button already over the Roast.
  answer = () => {};
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(roastButton(page)).toBeVisible();
  await expect(article).toBeVisible();
  expect((await article.boundingBox())!.y).toBe(top);
});

test('the sweep of the running dial goes round with no gap in the loop', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);

  // The sweep moves the dash by 100, so the dash and its gap have to repeat every 100.
  const dash = await page
    .locator('.progress .arc')
    .evaluate((arc) => getComputedStyle(arc).strokeDasharray.split(',').map((part) => parseFloat(part)));
  expect(dash).toEqual([18, 82]);
});

test('a long Roast scrolls by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 400 });
  await installMockIpc(page, { ...withHistory, latest_roast: () => aRoast });
  await openRoast(page);
  await expect(page.getByRole('region', { name: 'Verdict' })).toBeAttached();

  const main = page.locator('main');
  await expect(main).toHaveAttribute('tabindex', '0');
  await main.focus();
  await expect(main).toBeFocused();
  await page.keyboard.press('End');
  await expect.poll(() => main.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
});

test('the rail shows a Roast running, then how it ended, until you go and look', async ({ page }) => {
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler, latest_roast: () => run.saved });
  await openRoast(page);
  const rail = page.getByRole('button', { name: 'Roast', exact: true });
  const mark = rail.locator('.pip');
  await expect(mark).toHaveCount(0);

  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);
  await expect(mark).toHaveClass(/running/);
  await expect(rail).toHaveAttribute('title', 'A Roast is being written.');

  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await expect(mark).toHaveClass(/running/);
  run.finish();
  await expect(mark).toHaveClass(/ready/);
  await expect(rail).toHaveAttribute('title', 'Your Roast is ready.');

  await rail.click();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(mark).toHaveCount(0);
  await expect(rail).not.toHaveAttribute('title');

  // A failure away from the screen is the one nothing else would show.
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(2);
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  run.fail('Claude Code did not finish.');
  await expect(mark).toHaveClass(/failed/);
  await expect(rail).toHaveAttribute('title', 'That Roast did not finish. Open this screen to see why.');

  await rail.click();
  await expect(page.getByRole('alert')).toHaveText('Claude Code did not finish.');
  await expect(mark).toHaveCount(0);
});

test('a Roast that ends in plain sight leaves no mark behind, and a failure is brought into view', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 500 });
  const run = controlledRoast();
  await installMockIpc(page, { ...withHistory, generate_roast: run.handler, latest_roast: () => aRoast });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);

  // Reading the last Roast further down while the new one is written.
  await page.locator('main').evaluate((el) => (el.scrollTop = 100_000));
  run.fail('Claude Code did not finish.');

  await expect(page.getByRole('alert')).toBeInViewport();
  await expect(page.getByRole('button', { name: 'Roast', exact: true }).locator('.pip')).toHaveCount(0);
});

test('a finished Roast that could not be saved still shows, and says so', async ({ page }) => {
  const note = 'This Roast was not saved, so it will be gone when you close Rostery. Rostery could not use its local database: database is locked. Close any other copy of Rostery and try again.';
  const older: StoredRoast = { ...aRoast, id: 1, roast: { ...aRoast.roast, headline: 'An Older One' } };
  await installMockIpc(page, {
    ...withHistory,
    latest_roast: () => older,
    generate_roast: () => ({ roast: { ...aRoast, id: 0 }, notSaved: note }),
  });
  await openRoast(page);
  await expect(page.getByRole('heading', { level: 2, name: 'An Older One' })).toBeVisible();

  await roastButton(page).click();

  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveText(note);

  // The database only knows the older one, and that must not take its place while the app is open.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.getByRole('heading', { level: 2, name: aRoast.roast.headline })).toBeVisible();
  await expect(page.getByRole('alert')).toHaveText(note);
});

for (const [sendsTo, flagged] of [
  ['Your Digest goes to OpenAI through Codex.', false],
  ['Your Digest stays on this computer. It goes to Ollama here.', false],
  ['Warning: 192.168.1.5 is not this computer, so your Digest leaves this computer and goes there.', true],
] as const) {
  test(`the Roast screen says where the Digest goes: ${sendsTo}`, async ({ page }) => {
    await installMockIpc(page, { ...withHistory, provider_disclosure: () => ({ sendsTo, flagged }) });
    await openRoast(page);

    const line = page.getByText(sendsTo);
    await expect(line).toBeVisible();
    if (flagged) await expect(line).toHaveClass(/flagged/);
    else await expect(line).not.toHaveClass(/flagged/);
    await expect(page.getByText(DISCLOSURE)).toHaveCount(0);
  });
}

test('with nothing to say where a Digest goes, there is no button to send it', async ({ page }) => {
  await installMockIpc(page, {
    ...withHistory,
    provider_disclosure: () => {
      throw new Error('Rostery could not use its local database.');
    },
  });
  await openRoast(page);

  await expect(page.locator('main [role="alert"]:visible')).toContainText('could not use its local database');
  await expect(roastButton(page)).toHaveCount(0);
});

test('a running Roast goes on saying where it went after Settings change', async ({ page }) => {
  const run = controlledRoast();
  let disclosure = CLAUDE_CODE;
  const codex = { sendsTo: 'Your Digest goes to OpenAI through Codex.', flagged: false };
  await installMockIpc(page, { ...settingsMocks().handlers, ...withHistory, generate_roast: run.handler, provider_disclosure: () => disclosure });
  await openRoast(page);
  await roastButton(page).click();
  await expect.poll(() => run.calls).toBe(1);

  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByText('A Roast is running.')).toBeVisible();
  disclosure = codex;
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect(page.getByText(codex.sendsTo)).toBeVisible();

  await page.getByRole('button', { name: 'Roast', exact: true }).click();
  await expect(page.getByText(CLAUDE_CODE.sendsTo)).toBeVisible();
  await expect(page.getByText(codex.sendsTo)).toHaveCount(0);

  run.finish();
  await expect(page.getByText(codex.sendsTo)).toBeVisible();
});

const defaultVoice = 'You are the roastmaster.';
const allProviders = (): Settings => ({
  provider: { claudeCode: { model: null } },
  systemPrompt: null,
  others: [
    { codex: { model: null } },
    { local: { server: 'ollama', baseUrl: 'http://127.0.0.1:11434/v1', model: '' } },
    { local: { server: 'lmStudio', baseUrl: 'http://127.0.0.1:1234/v1', model: '' } },
  ],
  chooseAgain: false,
});
const statusOf = (id: ProviderStatus['id'], rest: Partial<ProviderStatus> = {}): ProviderStatus => ({
  id,
  available: true,
  signedIn: true,
  detail: 'Logged in.',
  sendsTo: `Your Digest goes somewhere through ${id}.`,
  flagged: false,
  models: [],
  ...rest,
});
const everyStatus: ProviderStatus[] = [
  statusOf('claude-code'),
  statusOf('codex', { signedIn: false, detail: 'Not logged in. Run codex login in a terminal, then check again.' }),
  statusOf('ollama', { available: false, detail: 'Not running at http://127.0.0.1:11434/v1. Install Ollama from ollama.com or start it with ollama serve, then check again.' }),
  statusOf('lm-studio', { detail: 'Running at http://127.0.0.1:1234/v1, with 2 models.', models: ['tiny-model', 'big-model'] }),
];

/** A store that keeps what is saved, the way Rust does, and answers the rest of the Settings screen. */
function settingsMocks(start: Settings = allProviders()) {
  const state = { saved: start, saves: [] as Settings[] };
  const handlers = {
    get_settings: () => state.saved,
    save_settings: ({ settings }: { settings: Settings }) => {
      state.saves.push(settings);
      state.saved = settings;
      return settings;
    },
    provider_statuses: () => everyStatus,
    default_system_prompt: () => defaultVoice,
    digest_preview: () => '{\n  "today": "2027-01-15"\n}',
  };
  return { state, handlers };
}

async function openSettings(page: Page) {
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Settings' })).toBeVisible();
}

const chosenProvider = (settings: Settings) => Object.keys(settings.provider)[0];
const providerOf = (settings: Settings, key: string) => [settings.provider, ...settings.others].find((p) => key in p) as Provider;

/** Ollama chosen, and no model yet, which is when a status answer fills one in. */
const onOllama = (): Settings => {
  const { provider, others, ...rest } = allProviders();
  return { ...rest, provider: others[1], others: [provider, others[0], others[2]] };
};
/** Status checks that each wait to be answered. */
function controlledStatuses() {
  const answers: ((statuses: ProviderStatus[]) => void)[] = [];
  return { answers, handler: () => new Promise((resolve) => answers.push(resolve)) };
}
const offering = (model: string) =>
  everyStatus.map((s) => (s.id === 'ollama' ? { ...s, available: true, detail: 'Running at http://127.0.0.1:11434/v1, with 1 model.', models: [model] } : s));
const leave = (page: Page, screen: string) => page.getByRole('button', { name: screen, exact: true }).click();
const visibleAlert = (page: Page) => page.locator('main [role="alert"]:visible');
const NO_DATABASE = 'Rostery could not use its local database.';

test('provider choice persists', async ({ page }) => {
  const mocks = settingsMocks();
  await installMockIpc(page, { ...empty, latest_roast: () => null, ...mocks.handlers });
  await openSettings(page);

  const codex = page.getByRole('radio', { name: /Codex/ });
  await expect(page.getByRole('radio', { name: /Claude Code/ })).toBeChecked();
  await codex.check();
  await expect.poll(() => mocks.state.saves.length).toBe(1);
  expect(chosenProvider(mocks.state.saved)).toBe('codex');
  // Nothing about the others was thrown away to make that choice.
  expect([mocks.state.saved.provider, ...mocks.state.saved.others]).toHaveLength(4);

  // Away and back, and across a whole reload.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(codex).toBeChecked();
  await openSettings(page);
  await expect(codex).toBeChecked();
});

test('saved settings that could not be read ask for a Provider, and only choosing one ends that', async ({ page }) => {
  const lost = { sendsTo: 'Rostery could not read your saved settings.', flagged: true };
  const mocks = settingsMocks({ ...allProviders(), chooseAgain: true });
  let refuseNext = true;
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...mocks.handlers,
    provider_disclosure: () => (mocks.state.saved.chooseAgain ? lost : CLAUDE_CODE),
    save_settings: (args: { settings: Settings }) => {
      if (!args.settings.chooseAgain && refuseNext) {
        refuseNext = false;
        throw new Error(NO_DATABASE);
      }
      return mocks.handlers.save_settings(args);
    },
  });
  await openSettings(page);

  const radios = page.getByRole('radio');
  await expect(radios).toHaveCount(4);
  for (const radio of await radios.all()) await expect(radio).not.toBeChecked();
  const line = page.getByText(lost.sendsTo);
  await expect(line).toBeVisible();
  await expect(page.locator('.sends')).toHaveClass(/flagged/);

  // A change to the prompt is kept, and it is not a choice of Provider.
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('Be gentle.');
  await box.blur();
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Be gentle.');
  expect(mocks.state.saved.chooseAgain).toBe(true);
  await expect(line).toBeVisible();

  // A choice that fails leaves nothing chosen.
  await page.getByRole('radio', { name: /Codex/ }).click();
  await expect(visibleAlert(page)).toContainText(NO_DATABASE);
  for (const radio of await radios.all()) await expect(radio).not.toBeChecked();
  expect(mocks.state.saved.chooseAgain).toBe(true);

  // Claude Code is the placeholder underneath, and a person can still choose it for real.
  const claude = page.getByRole('radio', { name: /Claude Code/ });
  await claude.check();
  await expect.poll(() => mocks.state.saved.chooseAgain).toBe(false);
  expect(chosenProvider(mocks.state.saved)).toBe('claudeCode');
  await expect(claude).toBeChecked();
  await expect(line).toHaveCount(0);
  await expect(page.getByText(CLAUDE_CODE.sendsTo)).toBeVisible();
});

test('a prompt typed with no Provider chosen is still saved when a dropped file takes the screen away', async ({ page }) => {
  const mocks = settingsMocks({ ...allProviders(), chooseAgain: true });
  await installMockIpc(page, { ...empty, latest_roast: () => null, ...mocks.handlers, import_paths: () => ({ ...nothing, filesRead: 1, added: 5 }) });
  await openSettings(page);

  await page.getByRole('textbox', { name: 'System prompt' }).fill('Be gentle.');
  await emitEvent(page, 'tauri://drag-drop', { paths: ['/d/first.zip'], position: { x: 9, y: 9 } });

  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Be gentle.');
  expect(mocks.state.saved.chooseAgain).toBe(true);
});

test('the words on a row follow a model as soon as it is typed and saved', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const saved = () => (mocks.state.saved.provider as { local: { model: string } }).local.model;
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...mocks.handlers,
    provider_statuses: () => everyStatus.map((s) => (s.id === 'ollama' ? { ...s, sendsTo: `Goes with "${saved()}".` } : s)),
  });
  await openSettings(page);

  const row = page.locator('.row > label', { has: page.getByRole('radio', { name: /Ollama/ }) });
  await expect(row).toHaveAttribute('title', 'Goes with "".');
  const model = page.getByRole('combobox', { name: 'Model' });
  await model.fill('gpt-oss:20b-cloud');
  await model.blur();
  await expect(row).toHaveAttribute('title', 'Goes with "gpt-oss:20b-cloud".');
});

test('each Provider shows its status and a next step, and Check again asks again', async ({ page }) => {
  let statuses = everyStatus;
  let asked = 0;
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...settingsMocks().handlers,
    provider_statuses: () => {
      asked += 1;
      return statuses;
    },
  });
  await openSettings(page);

  await expect(page.getByText('Logged in.', { exact: true })).toBeVisible();
  await expect(page.getByText('Run codex login in a terminal, then check again.')).toBeVisible();
  await expect(page.getByText('start it with ollama serve, then check again.')).toBeVisible();
  await expect(page.getByText('with 2 models.')).toBeVisible();
  expect(asked).toBe(1);

  statuses = everyStatus.map((s) => (s.id === 'codex' ? { ...s, signedIn: true, detail: 'Logged in now.' } : s));
  await page.getByRole('button', { name: 'Check again' }).click();
  await expect(page.getByText('Logged in now.')).toBeVisible();
  expect(asked).toBe(2);
});

test('a status check that never answers does not hold up the screen', async ({ page }) => {
  const mocks = settingsMocks();
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...mocks.handlers,
    provider_statuses: () => new Promise(() => {}),
  });
  await openSettings(page);

  await expect(page.getByText('Checking')).toHaveCount(4);
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');
  await expect(page.getByRole('textbox', { name: 'System prompt' })).toHaveValue(defaultVoice);
  await page.getByRole('button', { name: 'See what gets sent' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await expect(page.getByRole('heading', { level: 1, name: 'Sources' })).toBeVisible();
});

test('a status check that fails says why, and the rest of the screen still works', async ({ page }) => {
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...settingsMocks().handlers,
    provider_statuses: () => {
      throw new Error('Rostery could not use its local database.');
    },
  });
  await openSettings(page);
  await expect(page.locator('main [role="alert"]:visible')).toContainText('could not use its local database');
  await expect(page.getByRole('radio', { name: /Codex/ })).toBeEnabled();
  // Nothing is being asked now, and nothing was answered.
  await expect(page.getByText('Not known')).toHaveCount(4);
  await expect(page.getByText('Checking')).toHaveCount(0);
  await expect(page.locator('.lamp.wait')).toHaveCount(0);
  await expect(page.getByText('Asked just now.')).toHaveCount(0);
});

test('a line about where the Digest goes that fails says why, and the Providers still get checked', async ({ page }) => {
  await installMockIpc(page, {
    ...withHistory,
    ...settingsMocks().handlers,
    provider_disclosure: () => {
      throw new Error(NO_DATABASE);
    },
  });
  await openSettings(page);
  await expect(page.getByText('Logged in.', { exact: true })).toBeVisible();
  await expect(page.getByText('Asked just now.')).toBeVisible();
  await expect(visibleAlert(page)).toHaveText(NO_DATABASE);
});

test('a status answer reaching a Settings screen that has gone changes nothing', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const statuses = controlledStatuses();
  await installMockIpc(page, { ...withHistory, ...mocks.handlers, provider_statuses: statuses.handler });
  await openSettings(page);
  await expect.poll(() => statuses.answers.length).toBe(1);

  // Away, back and on to another Provider, with the first check still out.
  await leave(page, 'Sources');
  await leave(page, 'Settings');
  await expect.poll(() => statuses.answers.length).toBe(2);
  const codex = page.getByRole('radio', { name: /Codex/ });
  await codex.check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');

  statuses.answers[0](offering('tiny'));
  // What is being looked for is a save that does not come, so it gets time to.
  await page.waitForTimeout(300);
  expect(mocks.state.saves.map(chosenProvider)).toEqual(['codex']);
  await expect(codex).toBeChecked();
});

test('a save still on its way when Settings opens again is built on and not undone', async ({ page }) => {
  const mocks = settingsMocks();
  let finish = () => {};
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) =>
      new Promise((resolve) => {
        finish = () => {
          mocks.state.saved = settings;
          resolve(settings);
        };
        // Only the first waits, the way a database stays locked for a while.
        if (mocks.state.saved.systemPrompt) finish();
      }),
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('Speak only of soup.');
  await leave(page, 'Roast');
  await leave(page, 'Settings');
  await page.waitForTimeout(100);
  finish();

  await expect(box).toHaveValue('Speak only of soup.');
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');
  expect(mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
});

test('a read reaching a Settings screen that has gone changes nothing', async ({ page }) => {
  const mocks = settingsMocks();
  const before = mocks.state.saved;
  let firstRead: (settings: Settings) => void = () => {};
  let reads = 0;
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    get_settings: () => ((reads += 1) === 1 ? new Promise((resolve) => (firstRead = resolve)) : mocks.state.saved),
  });
  await openSettings(page);
  await expect.poll(() => reads).toBe(1);
  await leave(page, 'Sources');
  await leave(page, 'Settings');
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');

  firstRead(before);
  await page.waitForTimeout(100);
  const model = page.getByRole('combobox', { name: 'Model' });
  await model.fill('gpt-test');
  await model.blur();
  await expect.poll(() => providerOf(mocks.state.saved, 'codex')).toEqual({ codex: { model: 'gpt-test' } });
  expect(chosenProvider(mocks.state.saved)).toBe('codex');
});

test('a status answer leaves alone a model being typed, and fills in one left empty', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const statuses = controlledStatuses();
  await installMockIpc(page, { ...withHistory, ...mocks.handlers, provider_statuses: statuses.handler });
  await openSettings(page);
  await expect.poll(() => statuses.answers.length).toBe(1);

  const model = page.getByRole('combobox', { name: 'Model' });
  await model.click();
  await page.keyboard.type('my-own-mo');
  statuses.answers[0](offering('first-model'));
  await expect(page.getByText('with 1 model.')).toBeVisible();
  await expect(model).toHaveValue('my-own-mo');
  await page.keyboard.type('del');
  await model.blur();
  await expect.poll(() => (providerOf(mocks.state.saved, 'local') as any).local.model).toBe('my-own-model');
  expect(JSON.stringify(mocks.state.saves)).not.toContain('first-model');
  // A typed model is asked about again, because the words on its row are about what is saved.
  await expect.poll(() => statuses.answers.length).toBe(2);

  // Looking in the box and typing nothing is not a choice, and a Roast cannot start with no model.
  mocks.state.saved = onOllama();
  await page.goto('/');
  await leave(page, 'Settings');
  await expect.poll(() => statuses.answers.length).toBe(3);
  await model.click();
  statuses.answers[2](offering('first-model'));
  await expect(page.getByText('with 1 model.')).toBeVisible();
  await expect(model).toHaveValue('');
  await model.blur();
  await expect(model).toHaveValue('first-model');
  await expect.poll(() => (providerOf(mocks.state.saved, 'local') as any).local.model).toBe('first-model');
});

test('a status answer leaves alone a model on its way to being saved', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const statuses = controlledStatuses();
  let saves = 0;
  let finish = () => {};
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    provider_statuses: statuses.handler,
    save_settings: ({ settings }: { settings: Settings }) => {
      saves += 1;
      return new Promise((resolve) => {
        finish = () => {
          mocks.state.saved = settings;
          resolve(settings);
        };
      });
    },
  });
  await openSettings(page);
  await expect.poll(() => statuses.answers.length).toBe(1);

  const model = page.getByRole('combobox', { name: 'Model' });
  await model.fill('my-own-model');
  await model.blur();
  await expect.poll(() => saves).toBe(1);
  statuses.answers[0](offering('first-model'));
  await expect(page.getByText('with 1 model.')).toBeVisible();
  finish();
  await expect.poll(() => (providerOf(mocks.state.saved, 'local') as any).local.model).toBe('my-own-model');
  await page.waitForTimeout(300);
  expect(saves).toBe(1);
});

test('a Provider choice that fails says why, stays where it was, and the next one works', async ({ page }) => {
  const mocks = settingsMocks();
  let broken = true;
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (broken) throw new Error(NO_DATABASE);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  await expect(page.getByText('Asked just now.')).toBeVisible();
  await page.getByRole('radio', { name: /Codex/ }).click();
  await expect(visibleAlert(page)).toHaveText(NO_DATABASE);
  await expect(page.getByRole('radio', { name: /Claude Code/ })).toBeChecked();

  broken = false;
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');
  await expect(visibleAlert(page)).toHaveCount(0);
});

test('an import that fails while Settings is open says so there', async ({ page }) => {
  let fail: (e: Error) => void = () => {};
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...settingsMocks().handlers,
    'plugin:dialog|open': () => ['/d/notes.json'],
    import_paths: () => new Promise((_, reject) => (fail = reject)),
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Choose file' }).click();
  await expect(page.getByRole('button', { name: 'Choose file' })).toHaveAttribute('aria-disabled', 'true');
  await leave(page, 'Settings');
  await expect(page.getByRole('heading', { level: 1, name: 'Settings' })).toBeVisible();

  fail(new Error('Rostery did not find a Spotify or YouTube Export there.'));
  await expect(visibleAlert(page)).toHaveText('Rostery did not find a Spotify or YouTube Export there.');
});

test('the model and address of a Provider are saved and survive trying another', async ({ page }) => {
  const mocks = settingsMocks();
  await installMockIpc(page, { ...empty, latest_roast: () => null, ...mocks.handlers });
  await openSettings(page);

  await page.getByRole('combobox', { name: 'Model' }).fill('haiku');
  await page.getByRole('combobox', { name: 'Model' }).blur();
  await expect.poll(() => providerOf(mocks.state.saved, 'claudeCode')).toEqual({ claudeCode: { model: 'haiku' } });

  // A local server offers what it has, and the first one is chosen so a Roast can start.
  await page.getByRole('radio', { name: /LM Studio/ }).check();
  await expect(page.getByRole('combobox', { name: 'Model' })).toHaveValue('tiny-model');
  await expect
    .poll(() => providerOf(mocks.state.saved, 'local'))
    .toEqual({ local: { server: 'lmStudio', baseUrl: 'http://127.0.0.1:1234/v1', model: 'tiny-model' } });
  const options = page.locator('datalist option');
  await expect(options).toHaveCount(2);
  await expect(options.first()).toHaveAttribute('value', 'tiny-model');

  const address = page.getByRole('textbox', { name: 'Server address' });
  await expect(address).toHaveValue('http://127.0.0.1:1234/v1');
  await address.fill('http://192.168.1.5:1234/v1');
  await address.press('Enter');
  await expect.poll(() => mocks.state.saves.some((s) => JSON.stringify(s.provider).includes('192.168.1.5'))).toBe(true);

  // Trying another and coming back loses nothing.
  await page.getByRole('radio', { name: /Claude Code/ }).check();
  await expect(page.getByRole('combobox', { name: 'Model' })).toHaveValue('haiku');
  await page.getByRole('radio', { name: /LM Studio/ }).check();
  await expect(page.getByRole('textbox', { name: 'Server address' })).toHaveValue('http://192.168.1.5:1234/v1');
});

test('an address that is not local gets a warning that names the host', async ({ page }) => {
  const mocks = settingsMocks();
  const warning = 'Warning: 192.168.1.5 is not this computer, so your Digest leaves this computer and goes there.';
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...mocks.handlers,
    provider_disclosure: () =>
      JSON.stringify(mocks.state.saved.provider).includes('192.168.1.5') ? { sendsTo: warning, flagged: true } : CLAUDE_CODE,
  });
  await openSettings(page);
  await expect(page.getByText(CLAUDE_CODE.sendsTo)).toBeVisible();

  await page.getByRole('radio', { name: /Ollama/ }).check();
  const address = page.getByRole('textbox', { name: 'Server address' });
  await address.fill('http://192.168.1.5:11434/v1');
  await address.blur();

  const line = page.getByText(warning);
  await expect(line).toBeVisible();
  await expect(line.locator('..')).toHaveClass(/flagged/);
});

test('an address that is refused says why, and keeps what was typed', async ({ page }) => {
  const mocks = settingsMocks();
  const refused = 'Rostery cannot use the address "nope". It should look like http://127.0.0.1:11434/v1, with no name or password in it.';
  await installMockIpc(page, {
    ...empty,
    latest_roast: () => null,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (JSON.stringify(settings).includes('nope')) throw new Error(refused);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  await page.getByRole('radio', { name: /Ollama/ }).check();
  await expect(page.getByRole('textbox', { name: 'Server address' })).toBeVisible();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('local');

  const address = page.getByRole('textbox', { name: 'Server address' });
  await address.fill('nope');
  await address.blur();

  await expect(page.getByRole('alert')).toHaveText(refused);
  await expect(address).toHaveValue('nope');
  expect(JSON.stringify(mocks.state.saved)).not.toContain('nope');

  // A refused address does not get in the way of anything else.
  await page.getByRole('textbox', { name: 'System prompt' }).fill('Be gentle.');
  await page.getByRole('textbox', { name: 'System prompt' }).blur();
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Be gentle.');
});

test('a refused address keeps its warning until it is put right or the Provider changes', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const refused = 'Rostery cannot use the address "nope". It should look like http://127.0.0.1:11434/v1, with no name or password in it.';
  let statuses = everyStatus;
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    provider_statuses: () => statuses,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (JSON.stringify(settings).includes('nope')) throw new Error(refused);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  await expect(page.getByText('Asked just now.')).toBeVisible();
  const address = page.getByRole('textbox', { name: 'Server address' });
  const alert = page.getByRole('alert');
  await address.fill('nope');
  await address.blur();
  await expect(alert).toHaveText(refused);

  // A model saved by hand or by a status answer says nothing about the address.
  const model = page.getByRole('combobox', { name: 'Model' });
  await model.fill('tiny');
  await model.blur();
  await expect.poll(() => JSON.stringify(mocks.state.saved)).toContain('tiny');
  await expect(alert).toHaveText(refused);
  await expect(address).toHaveValue('nope');
  await model.fill('');
  await model.blur();
  await expect.poll(() => JSON.stringify(mocks.state.saved)).not.toContain('tiny');
  statuses = offering('first-model');
  await page.getByRole('button', { name: 'Check again' }).click();
  await expect(model).toHaveValue('first-model');
  await expect.poll(() => JSON.stringify(mocks.state.saved)).toContain('first-model');
  await expect(alert).toHaveText(refused);

  await address.fill('http://127.0.0.1:11435/v1');
  await address.blur();
  await expect(alert).toHaveCount(0);

  await address.fill('nope');
  await address.blur();
  await expect(alert).toHaveText(refused);
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect(alert).toHaveCount(0);
});

test('digest preview opens', async ({ page }) => {
  const digest = '{\n  "today": "2027-01-15",\n  "spotify": {\n    "plays": 944\n  }\n}';
  await installMockIpc(page, { ...withHistory, ...settingsMocks().handlers, digest_preview: () => digest });
  await openSettings(page);

  const button = page.getByRole('button', { name: 'See what gets sent' });
  await button.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog', { name: 'What gets sent' });
  const close = dialog.getByRole('button', { name: 'Close' });
  await expect(dialog).toBeVisible();
  // Exactly what came from Rust, line for line.
  expect(await dialog.locator('pre').evaluate((node) => node.textContent)).toBe(digest);
  await expect(close).toBeFocused();

  await close.click();
  await expect(dialog).toBeHidden();
  await expect(button).toBeFocused();

  // Every time, with what the last time left in there gone first.
  await page.keyboard.press('Enter');
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('pre')).toBeVisible();
  await expect(close).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(button).toBeFocused();
});

test('with no history the preview says so, and a failure says why', async ({ page }) => {
  let preview: () => unknown = () => null;
  await installMockIpc(page, { ...withHistory, ...settingsMocks().handlers, digest_preview: () => preview() });
  await openSettings(page);
  await page.getByRole('button', { name: 'See what gets sent' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByText('There is nothing to send yet.')).toBeVisible();
  await expect(dialog.locator('pre')).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Close' }).click();

  preview = () => {
    throw new Error('Rostery could not use its local database.');
  };
  await page.getByRole('button', { name: 'See what gets sent' }).click();
  await expect(dialog.getByRole('alert')).toContainText('could not use its local database');
  await dialog.getByRole('button', { name: 'Close' }).click();

  // Going from a Digest to nothing to send and to one still being read, focus stays in the dialog.
  preview = () => '{}';
  await page.getByRole('button', { name: 'See what gets sent' }).click();
  await expect(dialog.locator('pre')).toBeVisible();
  await dialog.getByRole('button', { name: 'Close' }).click();
  for (const next of [() => null, () => new Promise(() => {})]) {
    preview = next;
    await page.getByRole('button', { name: 'See what gets sent' }).click();
    await expect(dialog.locator('pre')).toHaveCount(0);
    await expect(dialog.getByRole('button', { name: 'Close' })).toBeFocused();
    await dialog.getByRole('button', { name: 'Close' }).click();
    preview = () => '{}';
    await page.getByRole('button', { name: 'See what gets sent' }).click();
    await expect(dialog.locator('pre')).toBeVisible();
    await dialog.getByRole('button', { name: 'Close' }).click();
  }
});

test('a Digest with a hostile or very long title stays text and stays inside the dialog', async ({ page }) => {
  const attack = '</pre><img src=x onerror=window.ran=1>';
  const digest = JSON.stringify({ title: attack, long: 'w'.repeat(5000) }, null, 2);
  await page.setViewportSize({ width: 640, height: 480 });
  await installMockIpc(page, { ...withHistory, ...settingsMocks().handlers, digest_preview: () => digest });
  await openSettings(page);
  await page.getByRole('button', { name: 'See what gets sent' }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.locator('pre')).toContainText(attack);
  await expect(dialog.locator('img')).toHaveCount(0);
  expect(await page.evaluate(() => (window as any).ran)).toBeUndefined();

  const sizes = await dialog.locator('pre').evaluate((node) => ({ scroll: node.scrollWidth, client: node.clientWidth }));
  expect(sizes.scroll).toBeLessThanOrEqual(sizes.client);
  const box = (await dialog.boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(640);
  expect(box.y + box.height).toBeLessThanOrEqual(480);
});

test('system prompt reset', async ({ page }) => {
  const mocks = settingsMocks();
  await installMockIpc(page, { ...withHistory, ...mocks.handlers });
  await openSettings(page);

  const box = page.getByRole('textbox', { name: 'System prompt' });
  const reset = page.getByRole('button', { name: 'Reset to default' });
  await expect(box).toHaveValue(defaultVoice);
  await expect(reset).toHaveAttribute('aria-disabled', 'true');

  await box.fill('Speak only of soup.');
  await box.blur();
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
  await expect(page.getByText('Saved. The next Roast uses this.')).toBeVisible();

  // The edit is still there on coming back.
  await page.getByRole('button', { name: 'Sources', exact: true }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(box).toHaveValue('Speak only of soup.');

  await reset.focus();
  await page.keyboard.press('Enter');
  await expect(box).toHaveValue(defaultVoice);
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe(null);
  await expect(reset).toBeFocused();
  await expect(page.getByText('Back to the default prompt.')).toBeVisible();

  // A wrong press on Reset should not cost someone their long prompt.
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(box).toHaveValue('Speak only of soup.');
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
  await expect(page.getByRole('button', { name: 'Undo' })).toHaveCount(0);
  // Its button is gone, so focus goes to the one beside it and not to the page.
  await expect(reset).toBeFocused();

  // Typing again makes what the last save said out of date.
  await expect(page.getByText('Saved. The next Roast uses this.')).toBeVisible();
  await box.fill('Speak only of soup, and of bread.');
  await expect(page.getByText('Saved. The next Roast uses this.')).toHaveCount(0);
});

test('a system prompt that is too long says so, and what was typed stays to shorten', async ({ page }) => {
  const mocks = settingsMocks();
  const refused = 'That system prompt is 20001 characters long and the limit is 20000, so Rostery did not save it. Shorten it, or use Reset to go back to the default.';
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if ((settings.systemPrompt ?? '').length > 20_000) throw new Error(refused);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  const long = 'x'.repeat(20_001);
  // Typing and pasting stop at the limit, so this goes past the box to reach what Rust says.
  await box.evaluate((node: HTMLTextAreaElement, text) => {
    node.value = text;
    node.dispatchEvent(new Event('input', { bubbles: true }));
    node.dispatchEvent(new Event('change', { bubbles: true }));
  }, long);

  await expect(page.getByRole('alert')).toHaveText(refused);
  await expect(box).toHaveValue(long);
  expect(mocks.state.saved.systemPrompt).toBe(null);

  // Choosing a Provider still works, and does not drag the refused text along.
  await page.getByRole('radio', { name: /Codex/ }).check();
  await expect.poll(() => chosenProvider(mocks.state.saved)).toBe('codex');
  expect(mocks.state.saved.systemPrompt).toBe(null);

  await page.getByRole('button', { name: 'Reset to default' }).click();
  await expect(box).toHaveValue(defaultVoice);
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('typing and pasting into the system prompt stop at the limit', async ({ page }) => {
  await installMockIpc(page, { ...withHistory, ...settingsMocks().handlers });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('x'.repeat(20_001));
  await expect(box).toHaveValue('x'.repeat(20_000));
  await expect(page.getByText('Up to 20,000 characters.')).toBeVisible();
});

test('a system prompt that could not be saved on the way out says so on the next screen, and comes back', async ({ page }) => {
  const mocks = settingsMocks();
  let broken = true;
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (broken) throw new Error(NO_DATABASE);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('Speak only of soup.');
  await leave(page, 'Roast');

  await expect(page.getByRole('heading', { level: 1, name: 'Roast' })).toBeVisible();
  await expect(visibleAlert(page)).toContainText('Your last change in Settings was not saved');
  await expect(visibleAlert(page)).toContainText(NO_DATABASE);
  // Until it can be put right, wherever the person goes.
  await leave(page, 'Sources');
  await expect(visibleAlert(page)).toContainText(NO_DATABASE);

  await leave(page, 'Settings');
  await expect(box).toHaveValue('Speak only of soup.');
  await expect(visibleAlert(page)).toHaveText(NO_DATABASE);
  expect(mocks.state.saved.systemPrompt).toBe(null);

  // Still not saved, so leaving tries again, and this time it works.
  broken = false;
  await leave(page, 'Roast');
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
  await expect(visibleAlert(page)).toHaveCount(0);
});

test('a system prompt that could not be saved on the way out is saved on coming back', async ({ page }) => {
  const mocks = settingsMocks();
  let broken = true;
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (broken) throw new Error(NO_DATABASE);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('Speak only of soup.');
  await leave(page, 'Roast');
  await expect(visibleAlert(page)).toContainText(NO_DATABASE);

  broken = false;
  await leave(page, 'Settings');
  await expect(box).toHaveValue('Speak only of soup.');
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
  await expect(page.getByText('Saved. The next Roast uses this.')).toBeVisible();
  await expect(visibleAlert(page)).toHaveCount(0);
});

test('a newer system prompt saved on the way out is not undone by an older one that failed', async ({ page }) => {
  const mocks = settingsMocks();
  const saving: { settings: Settings; finish: (error?: Error) => void }[] = [];
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) =>
      new Promise((resolve, reject) => {
        saving.push({
          settings,
          finish: (error) => {
            if (error) return reject(error);
            mocks.state.saved = settings;
            resolve(settings);
          },
        });
      }),
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });
  await box.fill('First go.');
  await box.blur();
  await expect.poll(() => saving.length).toBe(1);
  await box.fill('Second go.');
  await leave(page, 'Roast');
  await expect(page.getByRole('heading', { level: 1, name: 'Roast' })).toBeVisible();

  saving[0].finish(new Error(NO_DATABASE));
  await expect.poll(() => saving.length).toBe(2);
  saving[1].finish();
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Second go.');
  await expect(visibleAlert(page)).toHaveCount(0);

  await leave(page, 'Settings');
  await expect(box).toHaveValue('Second go.');
  expect(saving).toHaveLength(2);
});

test('an address refused on the way out says so on the next screen', async ({ page }) => {
  const mocks = settingsMocks(onOllama());
  const refused = 'Rostery cannot use the address "nope". It should look like http://127.0.0.1:11434/v1, with no name or password in it.';
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      if (JSON.stringify(settings).includes('nope')) throw new Error(refused);
      mocks.state.saved = settings;
      return settings;
    },
  });
  await openSettings(page);
  const address = page.getByRole('textbox', { name: 'Server address' });
  await address.fill('nope');
  await leave(page, 'Roast');
  await expect(visibleAlert(page)).toContainText(refused);

  // Back where it can be put right, it says so once more, and then it is done with.
  await leave(page, 'Settings');
  await expect(visibleAlert(page)).toHaveText(refused);
  await leave(page, 'Sources');
  await expect(visibleAlert(page)).toHaveCount(0);

  // Refused and read, then left with the box still holding it, and that box is going.
  await leave(page, 'Settings');
  await address.fill('nope');
  await address.blur();
  await expect(visibleAlert(page)).toHaveText(refused);
  await leave(page, 'Roast');
  await expect(visibleAlert(page)).toContainText(refused);
});

test('a blank system prompt goes back to the default, and what is typed during a save stays', async ({ page }) => {
  const mocks = settingsMocks();
  let saves = 0;
  let finish = () => {};
  await installMockIpc(page, {
    ...withHistory,
    ...mocks.handlers,
    save_settings: ({ settings }: { settings: Settings }) => {
      saves += 1;
      return new Promise((resolve) => {
        finish = () => {
          // Rust counts a blank prompt as none.
          mocks.state.saved = { ...settings, systemPrompt: settings.systemPrompt?.trim() || null };
          resolve(mocks.state.saved);
        };
      });
    },
  });
  await openSettings(page);
  const box = page.getByRole('textbox', { name: 'System prompt' });

  await box.fill('First go.');
  await box.blur();
  await expect.poll(() => saves).toBe(1);
  await box.fill('First go, and more.');
  finish();
  await expect(page.getByText('Saved. The next Roast uses this.')).toBeVisible();
  await expect(box).toHaveValue('First go, and more.');

  await box.blur();
  await expect.poll(() => saves).toBe(2);
  finish();
  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('First go, and more.');

  await box.fill('   ');
  await box.blur();
  await expect.poll(() => saves).toBe(3);
  finish();
  await expect(box).toHaveValue(defaultVoice);
  await expect(page.getByText('Back to the default prompt.')).toBeVisible();
});

test('an edit made just before leaving the screen is not lost', async ({ page }) => {
  const mocks = settingsMocks();
  await installMockIpc(page, { ...withHistory, ...mocks.handlers });
  await openSettings(page);
  await page.getByRole('textbox', { name: 'System prompt' }).fill('Speak only of soup.');

  await page.getByRole('button', { name: 'Roast', exact: true }).click();

  await expect.poll(() => mocks.state.saved.systemPrompt).toBe('Speak only of soup.');
});

test('Settings stays inside the window with a long status and prompt, and at every width', async ({ page }) => {
  const long = `http://${'very-long-host-name.'.repeat(12)}example.test:11434/v1`;
  const start = allProviders();
  start.provider = { local: { server: 'ollama', baseUrl: long, model: 'w'.repeat(300) } };
  start.others = [{ claudeCode: { model: null } }, { codex: { model: null } }, { local: { server: 'lmStudio', baseUrl: long, model: '' } }];
  start.systemPrompt = 'w'.repeat(20_000);
  await installMockIpc(page, {
    ...withHistory,
    ...settingsMocks(start).handlers,
    provider_statuses: () => everyStatus.map((s) => ({ ...s, detail: `Not running at ${long}. Then check again.` })),
    provider_disclosure: () => ({ sendsTo: `Warning: ${long} is not this computer.`, flagged: true }),
  });
  await openSettings(page);
  await expect(page.getByText('Then check again.').first()).toBeVisible();

  for (const width of [1200, 721, 720, 500, 320]) {
    await page.setViewportSize({ width, height: 800 });
    const problems = await page.evaluate(() => {
      const main = document.querySelector('main')!;
      const edge = main.getBoundingClientRect().right;
      const past = [...main.querySelectorAll('h1, h2, p, label, input, textarea, button')].filter((node) => node.getBoundingClientRect().right > edge + 0.5);
      return { sideways: main.scrollWidth - main.clientWidth, past: past.map((node) => node.tagName) };
    });
    expect(problems.sideways, `main scrolls sideways at ${width}px`).toBeLessThanOrEqual(0);
    expect(problems.past, `something leaves the screen at ${width}px`).toEqual([]);
  }
});

test('Tab reaches every control on Settings', async ({ page }) => {
  await installMockIpc(page, { ...withHistory, ...settingsMocks().handlers });
  await openSettings(page);
  await expect(page.getByText('Logged in.', { exact: true })).toBeVisible();

  const reached = new Set<string>();
  for (let press = 0; press < 25; press += 1) {
    await page.keyboard.press('Tab');
    reached.add(
      await page.evaluate(() => {
        const el = document.activeElement as HTMLInputElement;
        return el.tagName === 'BUTTON' ? el.innerText.trim() : el.tagName === 'INPUT' ? el.type : el.tagName;
      }),
    );
  }
  for (const name of ['Check again', 'radio', 'text', 'See what gets sent', 'TEXTAREA', 'Reset to default']) {
    expect([...reached], `${name} was never reached`).toContain(name);
  }
});

/** The name of everything Tab lands on, the way a WebKit webview really moves. */
async function reachedByTab(page: Page, presses: number) {
  const reached = new Set<string>();
  for (let press = 0; press < presses; press += 1) {
    await page.keyboard.press('Tab');
    const name = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement;
      return el === document.body ? '' : el.innerText.trim();
    });
    if (name) reached.add(name);
  }
  return [...reached];
}

test('Tab reaches the main actions on Sources', async ({ page }) => {
  await installMockIpc(page, { ...withHistory, has_demo_data: () => true });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Clear demo data' })).toBeVisible();

  const reached = await reachedByTab(page, 20);

  for (const name of ['Sources', 'Roast', 'Try demo data', 'Clear demo data', 'Choose file', 'Choose folder']) {
    expect(reached, `${name} was never reached`).toContain(name);
  }
});

test('Tab reaches the main actions on Roast', async ({ page }) => {
  await installMockIpc(page, withHistory);
  await openRoast(page);
  await expect(roastButton(page)).toBeVisible();

  expect(await reachedByTab(page, 15), 'Roast me was never reached').toContain('Roast me');

  const button = roastButton(page);
  await expect(button).toHaveAttribute('tabindex', '0');
});

test('Tab reaches the way out of an empty Roast screen', async ({ page }) => {
  await installMockIpc(page, { ...empty, latest_roast: () => null });
  await openRoast(page);
  await expect(page.getByRole('button', { name: 'Go to Sources' })).toBeVisible();

  expect(await reachedByTab(page, 15), 'Go to Sources was never reached').toContain('Go to Sources');
});
