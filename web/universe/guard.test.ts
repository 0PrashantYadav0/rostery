// The Share Site mounts UniverseView with no IPC, so nothing it reaches may touch Tauri or the app.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import PlanetPanel from './PlanetPanel.svelte';
import type { Planet, Universe } from '../shared/types';

const web = resolve(import.meta.dirname, '..');

/** Every file `entry` reaches through relative imports, and every package any of them names. */
function reach(entry: string) {
  const files = new Set<string>();
  const packages = new Set<string>();
  const visit = (file: string) => {
    if (files.has(file)) return;
    files.add(file);
    const source = readFileSync(file, 'utf8');
    for (const [, specifier] of source.matchAll(/(?:from\s+|import\s*\(?\s*)['"]([^'"]+)['"]/g)) {
      if (!specifier.startsWith('.')) {
        packages.add(specifier);
        continue;
      }
      const base = resolve(dirname(file), specifier);
      const found = [base, `${base}.ts`, `${base}.svelte`].find(existsSync);
      if (found) visit(found);
    }
  };
  visit(entry);
  return { files: [...files], packages: [...packages] };
}

describe('UniverseView', () => {
  const { files, packages } = reach(resolve(web, 'universe/UniverseView.svelte'));

  it('reaches the renderer, so this is really walking the imports', () => {
    expect(files.map((f) => f.slice(web.length))).toEqual(expect.arrayContaining(['/universe/render.ts', '/universe/PlanetPanel.svelte']));
    expect(packages).toContain('three');
  });

  it('imports nothing from Tauri', () => {
    expect(packages.filter((p) => p.includes('tauri'))).toEqual([]);
  });

  it('imports nothing from the app', () => {
    expect(files.filter((f) => f.startsWith(resolve(web, 'app')))).toEqual([]);
  });
});

describe('the app may only open Source links', () => {
  const capability = JSON.parse(readFileSync(resolve(web, '../src-tauri/capabilities/default.json'), 'utf8'));

  it('scopes the opener to the two hosts and grants nothing else from it', () => {
    const fromOpener = capability.permissions.filter((p: string | { identifier: string }) =>
      (typeof p === 'string' ? p : p.identifier).startsWith('opener:'),
    );
    expect(fromOpener).toEqual([
      {
        identifier: 'opener:allow-open-url',
        allow: [{ url: 'https://open.spotify.com/*' }, { url: 'https://www.youtube.com/*' }],
      },
    ]);
  });
});

describe('the link back to the Source', () => {
  const planet: Planet = {
    id: 0,
    name: 'Moth Season',
    source: 'spotify',
    minutes: 600,
    plays: 40,
    radius: 4,
    position: [0, 0, 0],
    galaxy: 0,
    interests: [],
    url: 'https://open.spotify.com/artist/abc',
    topItems: [{ title: 'Low Tide', url: 'https://open.spotify.com/track/def', plays: 9 }],
  };
  const universe: Universe = {
    filter: 'spotify',
    planets: [planet],
    links: [],
    galaxies: [{ id: 0, name: 'dreamy', hue: 0.11, size: 1 }],
  };
  const html = (props: Partial<{ planet: Planet; showItems: boolean; openLink: (url: string) => void }>) =>
    render(PlanetPanel, { props: { planet, universe, showItems: true, onclose() {}, onselect() {}, ...props } }).body;

  // With no openLink, the way a Share Site mounts it, these are plain links in a new tab.
  it('is a plain link that keeps the opener and referrer away', () => {
    const anchors = html({}).match(/<a\b[^>]*>/g)!;
    expect(anchors).toHaveLength(2);
    for (const anchor of anchors) {
      expect(anchor).toContain('target="_blank"');
      expect(anchor).toContain('rel="noopener noreferrer"');
    }
    expect(anchors[0]).toContain('href="https://open.spotify.com/track/def"');
    expect(anchors[1]).toContain('href="https://open.spotify.com/artist/abc"');
  });

  it('says where it goes', () => {
    expect(html({})).toContain('Open in Spotify');
    expect(html({ planet: { ...planet, source: 'youtube', url: 'https://www.youtube.com/channel/UCabc' } })).toContain('Open on YouTube');
  });

  it('is not there for a YouTube Planet with no channel', () => {
    expect(html({ planet: { ...planet, source: 'youtube', url: null, topItems: [] } })).not.toMatch(/<a\s/);
  });

  it('never carries a URL that is not on the Source', () => {
    const hostile = { ...planet, url: 'javascript:alert(1)', topItems: [{ title: 'x', url: 'https://evil.example/a', plays: 1 }] };
    expect(html({ planet: hostile })).not.toMatch(/<a\s/);
  });

  it('leaves out Item titles when they are off, as they are by default on a Share Site', () => {
    expect(html({ showItems: true })).toContain('Low Tide');
    expect(html({ showItems: false })).not.toContain('Low Tide');
  });
});
