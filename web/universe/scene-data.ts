// Pure facts about a Universe. No three.js here, so these load with the screen and a test can run them.
import type { Planet, Source, Universe, UniverseFilter } from '../shared/types';

/** How many of the largest Planets carry a name in the sky. More would slow every frame and hide the picture. */
const LABELLED = 12;

/** Largest first. Two Sources scale apart, so equal sizes are common, and `id` settles those the same way each time. */
export const bySize = (planets: Planet[]) => [...planets].sort((a, b) => b.radius - a.radius || a.id - b.id);

export const topLabelIds = (planets: Planet[]) => bySize(planets).slice(0, LABELLED).map((p) => p.id);

/** For example "412 hours" or "37 min" for Spotify, and "128 watches" for YouTube. */
export function formatWeight({ source, minutes, plays }: Pick<Planet, 'source' | 'minutes' | 'plays'>): string {
  if (source === 'youtube') return `${plays.toLocaleString('en')} ${plays === 1 ? 'watch' : 'watches'}`;
  const whole = Math.round(minutes);
  if (whole < 60) return `${whole} min`;
  const hours = Math.round(whole / 60);
  return `${hours.toLocaleString('en')} ${hours === 1 ? 'hour' : 'hours'}`;
}

export interface Neighbour {
  planet: Planet;
  strength: number;
  /** Why these two are linked. */
  reason: string;
}

/** The Planets linked to this one, strongest first. */
export function neighbours(universe: Universe, planetId: number): Neighbour[] {
  return universe.links
    .filter((link) => link.a === planetId || link.b === planetId)
    .flatMap((link) => {
      const planet = universe.planets[link.a === planetId ? link.b : link.a];
      // ponytail: a Link with both reasons only names its Interests, because a Link does not say how many
      // Sessions it shares. Add that count in Rust if someone asks why a pair is so close.
      const reason = link.shared.length ? `shared Interests: ${link.shared.join(', ')}` : 'shared Sessions';
      return planet ? [{ planet, strength: link.strength, reason }] : [];
    })
    .sort((a, b) => b.strength - a.strength || a.planet.id - b.planet.id);
}

/** FNV-1a, from 0 up to 1. The same name always gets the same Planet, in the app and on a Share Site. */
export function planetSeed(name: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < name.length; i += 1) {
    hash = Math.imul(hash ^ name.charCodeAt(i), 0x01000193);
  }
  return (hash >>> 0) / 2 ** 32;
}

type Universes = Partial<Record<UniverseFilter, Universe>>;

/**
 * The tabs worth showing. With one Source, Both would show that Source again, so it goes too, unless
 * it is all a Share Site carries.
 */
export function availableFilters(universes: Universes): UniverseFilter[] {
  const has = (filter: UniverseFilter) => Boolean(universes[filter]?.planets.length);
  const sources = (['spotify', 'youtube'] as const).filter(has);
  return has('both') && sources.length !== 1 ? [...sources, 'both'] : sources;
}

export const defaultFilter = (filters: UniverseFilter[]) => (filters.includes('both') ? 'both' : filters[0]);

type Point = readonly [number, number, number];

function nearestHit(planets: Planet[], origin: Point, direction: Point, reach: number) {
  let nearest: Planet | undefined;
  let nearestAt = Infinity;
  for (const planet of planets) {
    const to = planet.position.map((c, axis) => c - origin[axis]);
    const along = to[0] * direction[0] + to[1] * direction[1] + to[2] * direction[2];
    if (along <= 0) continue;
    const size = Math.max(planet.radius, along * reach);
    const off = to[0] * to[0] + to[1] * to[1] + to[2] * to[2] - along * along;
    if (off > size * size) continue;
    const at = along - Math.sqrt(size * size - off);
    if (at < nearestAt) {
      nearestAt = at;
      nearest = planet;
    }
  }
  return nearest;
}

/**
 * The Planet a ray from the camera hits. This reads the Planets, not the mesh that draws them, so it
 * cannot go out of step with a stale bounding sphere, and it is cheap enough to run on every mouse move.
 * `direction` has length 1. `reach` widens each target to that share of its distance, so a far-off
 * Planet a few pixels across can still be clicked, but a real hit always wins.
 */
export function pickPlanet(planets: Planet[], origin: Point, direction: Point, reach = 0): Planet | undefined {
  return nearestHit(planets, origin, direction, 0) ?? nearestHit(planets, origin, direction, reach);
}

const SOURCE_HOSTS = ['open.spotify.com', 'www.youtube.com'];

/**
 * The link back to a Source, or null. Rust builds these from checked IDs, and this is a second lock on
 * the same door, because a page will open whatever is here. `host` includes any port, so a port fails too.
 */
export function sourceUrl(url: string | null): string | null {
  try {
    const { protocol, host, username, password } = new URL(url ?? '');
    return protocol === 'https:' && SOURCE_HOSTS.includes(host) && !username && !password ? url : null;
  } catch {
    return null;
  }
}

export const sourceLinkLabel = (source: Source) => (source === 'spotify' ? 'Open in Spotify' : 'Open on YouTube');

/** How strongly a Link shows. A faint one is still there, because a Link at the floor is still a real Link. */
export function linkOpacity(strength: number): number {
  const share = Math.min(Math.max((strength - 0.12) / 0.88, 0), 1);
  return 0.1 + 0.75 * share;
}

type Rgb = [number, number, number];

/**
 * Mineral pigments round the wheel, the colours an old star atlas was painted in. Rust hands out hues
 * evenly round the wheel, and a plain wheel spends a quarter of itself on violet, which would make this
 * look like every other space picture. So its colours are looked up here instead.
 */
const PIGMENTS: [hue: number, color: Rgb][] = [
  [0, [0.86, 0.36, 0.28]], // cinnabar
  [0.11, [0.84, 0.66, 0.33]], // brass
  [0.22, [0.66, 0.7, 0.3]], // olive
  [0.36, [0.33, 0.68, 0.46]], // malachite
  [0.5, [0.25, 0.7, 0.65]], // verdigris
  [0.6, [0.3, 0.64, 0.84]], // cerulean
  [0.72, [0.34, 0.47, 0.84]], // lapis
  [0.78, [0.45, 0.56, 0.66]], // slate, which keeps the way from blue to rose out of violet
  [0.85, [0.78, 0.46, 0.46]], // dusty rose
  [0.93, [0.86, 0.38, 0.47]], // madder
  [1, [0.86, 0.36, 0.28]], // cinnabar again, so the wheel closes
];

/** A Galaxy's colour, from 0 up to 1 in each channel, as painted in the sky and shown in the legend. */
export function galaxyColor(hue: number): Rgb {
  const wheel = hue - Math.floor(hue);
  const at = PIGMENTS.findIndex(([stop]) => stop >= wheel);
  if (at <= 0) return [...PIGMENTS[0][1]];
  const [from, a] = PIGMENTS[at - 1];
  const [to, b] = PIGMENTS[at];
  const share = (wheel - from) / (to - from);
  return a.map((channel, i) => channel + (b[i] - channel) * share) as Rgb;
}

export const cssColor = ([r, g, b]: Rgb) => `rgb(${Math.round(r * 255)} ${Math.round(g * 255)} ${Math.round(b * 255)})`;
