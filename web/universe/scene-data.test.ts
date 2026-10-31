import { describe, expect, it } from 'vitest';
import {
  availableFilters,
  bySize,
  defaultFilter,
  formatWeight,
  galaxyColor,
  linkOpacity,
  neighbours,
  pickPlanet,
  planetSeed,
  sourceLinkLabel,
  sourceUrl,
  topLabelIds,
} from './scene-data';
import { placeLabels, type LabelBox } from './labels';
import type { Planet, Source, Universe } from '../shared/types';

function planet(id: number, radius: number, source: Source = 'spotify', position: Planet['position'] = [0, 0, 0]): Planet {
  return {
    id,
    name: `Creator ${id}`,
    source,
    minutes: 100,
    plays: 10,
    radius,
    position,
    galaxy: 0,
    interests: [],
    url: null,
    topItems: [],
  };
}

const sky = (planets: Planet[], links: Universe['links'] = []): Universe => ({ filter: 'both', planets, links, galaxies: [] });

describe('topLabelIds', () => {
  it('returns the 12 largest Planets, largest first', () => {
    const planets = Array.from({ length: 30 }, (_, i) => planet(i, 0.6 + (i % 15) * 0.2));
    const ids = topLabelIds(planets);
    expect(ids).toHaveLength(12);
    const radii = ids.map((id) => planets[id].radius);
    expect(radii).toEqual([...radii].sort((a, b) => b - a));
    expect(Math.min(...radii)).toBeGreaterThanOrEqual(Math.max(...planets.filter((p) => !ids.includes(p.id)).map((p) => p.radius)));
  });

  it('breaks a tie the same way every time', () => {
    const planets = [planet(3, 2), planet(1, 2), planet(2, 2)];
    expect(topLabelIds(planets)).toEqual([1, 2, 3]);
    expect(topLabelIds([...planets].reverse())).toEqual([1, 2, 3]);
  });

  it('copes with fewer than 12', () => {
    expect(topLabelIds([planet(0, 1)])).toEqual([0]);
    expect(topLabelIds([])).toEqual([]);
  });
});

describe('bySize', () => {
  it('sorts largest first and leaves the Universe alone', () => {
    const planets = [planet(0, 1), planet(1, 3), planet(2, 2)];
    expect(bySize(planets).map((p) => p.id)).toEqual([1, 2, 0]);
    expect(planets.map((p) => p.id)).toEqual([0, 1, 2]);
  });
});

describe('formatWeight', () => {
  const spotify = (minutes: number) => formatWeight({ source: 'spotify', minutes, plays: 3 });
  const youtube = (plays: number) => formatWeight({ source: 'youtube', minutes: plays * 8, plays });

  it('shows Spotify in hours, or in minutes under an hour', () => {
    expect(spotify(412 * 60)).toBe('412 hours');
    expect(spotify(37)).toBe('37 min');
  });

  it('rounds up to an hour at the boundary, and has a singular', () => {
    expect(spotify(59.4)).toBe('59 min');
    expect(spotify(59.6)).toBe('1 hour');
    expect(spotify(60)).toBe('1 hour');
    expect(spotify(150)).toBe('3 hours');
  });

  it('shows a large number with a separator', () => {
    expect(spotify(5000 * 60)).toBe('5,000 hours');
    expect(youtube(1500)).toBe('1,500 watches');
  });

  it('shows YouTube in watches', () => {
    expect(youtube(128)).toBe('128 watches');
    expect(youtube(1)).toBe('1 watch');
  });

  it('has an answer for a Planet with no Plays, which an Affinity can make', () => {
    expect(spotify(0)).toBe('0 min');
    expect(youtube(0)).toBe('0 watches');
  });
});

describe('neighbours', () => {
  const planets = [planet(0, 4), planet(1, 3), planet(2, 2), planet(3, 1)];
  const universe = sky(planets, [
    { a: 0, b: 1, strength: 0.2, shared: [] },
    { a: 0, b: 2, strength: 0.7, shared: ['dreamy', 'headphones'] },
    { a: 1, b: 3, strength: 0.9, shared: [] },
  ]);

  it('returns the linked Planets, strongest first', () => {
    expect(neighbours(universe, 0).map((n) => n.planet.id)).toEqual([2, 1]);
  });

  it('finds a Planet on either end of a Link', () => {
    expect(neighbours(universe, 3).map((n) => n.planet.id)).toEqual([1]);
    expect(neighbours(universe, 1).map((n) => n.planet.id)).toEqual([3, 0]);
  });

  it('says why they are linked', () => {
    const [interests, sessions] = neighbours(universe, 0);
    expect(interests.reason).toBe('shared Interests: dreamy, headphones');
    expect(sessions.reason).toBe('shared Sessions');
  });

  it('leaves out a Link to a Planet that is not there', () => {
    expect(neighbours(sky(planets, [{ a: 0, b: 99, strength: 0.5, shared: [] }]), 0)).toEqual([]);
  });

  it('gives nothing for a Planet with no Links', () => {
    expect(neighbours(sky(planets), 0)).toEqual([]);
  });
});

describe('planetSeed', () => {
  it('is stable for a name', () => {
    expect(planetSeed('Moth Season')).toBe(planetSeed('Moth Season'));
    // Pinned, because a change would repaint every Planet between the app and an older Share Site.
    expect(planetSeed('Moth Season')).toBeCloseTo(0.65034, 4);
  });

  it('stays from 0 up to 1 for odd names', () => {
    for (const name of ['', 'a', '🪐🪐🪐', 'مرحبا بالعالم', 'x'.repeat(10_000)]) {
      const seed = planetSeed(name);
      expect(seed).toBeGreaterThanOrEqual(0);
      expect(seed).toBeLessThan(1);
    }
  });

  it('gives each of many similar names its own seed', () => {
    const seeds = new Set(Array.from({ length: 150 }, (_, i) => planetSeed(`Creator ${i}`)));
    expect(seeds.size).toBe(150);
  });
});

describe('availableFilters', () => {
  const some = sky([planet(0, 1)]);
  const none = sky([]);

  it('offers all three when both Sources have Planets', () => {
    expect(availableFilters({ spotify: some, youtube: some, both: some })).toEqual(['spotify', 'youtube', 'both']);
  });

  it('hides a Source with no Planets, and Both with it, since Both would show the same thing', () => {
    expect(availableFilters({ spotify: some, youtube: none, both: some })).toEqual(['spotify']);
    expect(availableFilters({ spotify: none, youtube: some, both: some })).toEqual(['youtube']);
  });

  it('works when a Share Site carries only some of them', () => {
    expect(availableFilters({ youtube: some })).toEqual(['youtube']);
    expect(availableFilters({ both: some })).toEqual(['both']);
    expect(availableFilters({ spotify: some, youtube: some })).toEqual(['spotify', 'youtube']);
  });

  it('offers nothing when nothing has Planets', () => {
    expect(availableFilters({})).toEqual([]);
    expect(availableFilters({ spotify: none, youtube: none, both: none })).toEqual([]);
  });

  it('starts on Both when it can', () => {
    expect(defaultFilter(['spotify', 'youtube', 'both'])).toBe('both');
    expect(defaultFilter(['youtube'])).toBe('youtube');
    expect(defaultFilter([])).toBeUndefined();
  });
});

describe('pickPlanet', () => {
  const along = (planets: Planet[]) => pickPlanet(planets, [0, 0, 100], [0, 0, -1]);

  it('finds the Planet the ray hits', () => {
    expect(along([planet(0, 2, 'spotify', [0, 0, 0])])?.id).toBe(0);
    expect(along([planet(0, 2, 'spotify', [3, 0, 0])])).toBeUndefined();
  });

  it('takes the nearer of two in line', () => {
    const far = planet(0, 2, 'spotify', [0, 0, -20]);
    const near = planet(1, 1, 'spotify', [0, 0, 20]);
    expect(along([far, near])?.id).toBe(1);
    expect(along([near, far])?.id).toBe(1);
  });

  it('ignores a Planet behind the ray', () => {
    expect(along([planet(0, 2, 'spotify', [0, 0, 200])])).toBeUndefined();
  });

  it('gives a small far Planet a wider target, but only as wide as asked', () => {
    const small = planet(0, 0.6, 'spotify', [1, 0, 0]);
    expect(pickPlanet([small], [0, 0, 100], [0, 0, -1])).toBeUndefined();
    expect(pickPlanet([small], [0, 0, 100], [0, 0, -1], 0.02)?.id).toBe(0);
  });

  it('prefers the Planet the ray really hits over a wider target in front of it', () => {
    const small = planet(0, 0.6, 'spotify', [1, 0, 40]);
    const big = planet(1, 3, 'spotify', [0, 0, 0]);
    expect(pickPlanet([small, big], [0, 0, 100], [0, 0, -1], 0.02)?.id).toBe(1);
  });
});

describe('sourceUrl', () => {
  it('passes https links on the two hosts Rust builds', () => {
    for (const url of [
      'https://open.spotify.com/artist/abc',
      'https://open.spotify.com/search/AC%2FDC%20%26%20Friends%3F',
      'https://www.youtube.com/channel/UCabc-123_x',
      'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
    ]) {
      expect(sourceUrl(url)).toBe(url);
    }
  });

  it('refuses anything else', () => {
    for (const url of [
      null,
      '',
      'not a url',
      'javascript:alert(1)',
      'data:text/html,<script>alert(1)</script>',
      'http://www.youtube.com/channel/abc',
      'https://evil.example/channel/abc',
      'https://www.youtube.com.evil.example/channel/abc',
      'https://www.youtube.com@evil.example/channel/abc',
      'https://youtube.com/channel/abc',
      'https://open.spotify.com:8443/artist/abc',
    ]) {
      expect(sourceUrl(url), String(url)).toBeNull();
    }
  });
});

describe('sourceLinkLabel', () => {
  it('uses the words a person expects for each Source', () => {
    expect(sourceLinkLabel('spotify')).toBe('Open in Spotify');
    expect(sourceLinkLabel('youtube')).toBe('Open on YouTube');
  });
});

describe('linkOpacity', () => {
  it('gets stronger with the Link, and stays visible and in range', () => {
    const values = [0.12, 0.3, 0.5, 0.8, 1].map(linkOpacity);
    expect(values).toEqual([...values].sort((a, b) => a - b));
    expect(new Set(values).size).toBe(values.length);
    expect(values[0]).toBeGreaterThan(0);
    expect(values[4]).toBeLessThanOrEqual(1);
  });
});

describe('galaxyColor', () => {
  it('gives the first Galaxy the app own brass', () => {
    const [r, g, b] = galaxyColor(0.11);
    expect(r).toBeGreaterThan(g);
    expect(g).toBeGreaterThan(b);
  });

  it('stays in range all the way round, including past the seam', () => {
    for (let hue = 0; hue < 1; hue += 0.01) {
      for (const channel of galaxyColor(hue)) {
        expect(channel).toBeGreaterThanOrEqual(0);
        expect(channel).toBeLessThanOrEqual(1);
      }
    }
    expect(galaxyColor(0)).toEqual(galaxyColor(1));
  });

  it('never turns purple, which is every other space picture', () => {
    for (let hue = 0; hue < 1; hue += 0.01) {
      const [r, g, b] = galaxyColor(hue);
      const violet = b > g * 1.15 && r > g * 1.15 && b >= r;
      expect(violet, `hue ${hue} gives ${[r, g, b]}`).toBe(false);
    }
  });

  it('keeps Galaxies apart that sit next to each other in order', () => {
    const golden = 0.618033988749895;
    const colors = Array.from({ length: 10 }, (_, i) => galaxyColor((0.11 + i * golden) % 1));
    for (let i = 1; i < colors.length; i += 1) {
      const gap = Math.hypot(...colors[i].map((c, axis) => c - colors[i - 1][axis]));
      expect(gap, `Galaxies ${i - 1} and ${i}`).toBeGreaterThan(0.15);
    }
  });
});

describe('placeLabels', () => {
  const box = (x: number, y: number, radius = 0, width = 100, height = 20): LabelBox => ({ x, y, radius, width, height });
  const hits = (a: { x: number; y: number }, b: { x: number; y: number }) =>
    a.x < b.x + 100 && b.x < a.x + 100 && a.y < b.y + 20 && b.y < a.y + 20;

  it('puts a label up and to the right, clear of its Planet', () => {
    const [placed] = placeLabels([box(100, 100, 20)], 500, 500);
    expect(placed!.x).toBeGreaterThan(100 + 20 * Math.SQRT1_2);
    expect(placed!.y).toBeLessThan(100);
  });

  it('flips to the left near the right edge', () => {
    const [placed] = placeLabels([box(460, 100, 10)], 500, 500);
    expect(placed!.x + 100).toBeLessThan(460);
  });

  it('leaves out a label that would fall off the sky', () => {
    expect(placeLabels([box(100, 2, 10)], 500, 500)).toEqual([null]);
    expect(placeLabels([box(100, 900, 10)], 500, 500)).toEqual([null]);
    expect(placeLabels([box(50, 100, 10, 80)], 60, 500)).toEqual([null]);
  });

  it('keeps the more important of two labels that would collide', () => {
    const [first, second] = placeLabels([box(100, 100), box(110, 105)], 500, 500);
    expect(first).not.toBeNull();
    expect(second).toBeNull();
  });

  it('never leaves any two on top of each other', () => {
    const crowd = Array.from({ length: 12 }, (_, i) => box(60 + (i % 4) * 70, 60 + (i % 3) * 16, i));
    const placed = placeLabels(crowd, 500, 500).filter((p) => p !== null);
    expect(placed.length).toBeGreaterThan(0);
    placed.forEach((a, i) => placed.slice(i + 1).forEach((b) => expect(hits(a, b)).toBe(false)));
  });

  it('skips a Planet behind the camera without losing its place in the order', () => {
    const [behind, seen] = placeLabels([null, box(100, 100)], 500, 500);
    expect(behind).toBeNull();
    expect(seen).not.toBeNull();
  });
});
