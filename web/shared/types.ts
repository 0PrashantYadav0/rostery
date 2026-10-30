// Mirrors the Rust structs in crates/core/src/model.rs and universe/mod.rs, in camelCase.

export type Source = 'spotify' | 'youtube';
export type Origin = 'export' | 'api' | 'demo';

export interface SourceTotals {
  plays: number;
  minutes: number;
  creators: number;
  firstPlay: number | null;
  lastPlay: number | null;
}

export interface Totals {
  spotify: SourceTotals;
  youtube: SourceTotals;
}

export interface ImportSummary {
  filesRead: number;
  filesSkipped: number;
  added: number;
  duplicates: number;
  excludedPrivate: number;
  excludedAds: number;
  rejected: number;
  skippedReason: string | null;
}

export type UniverseFilter = Source | 'both';

export interface TopItem {
  title: string;
  url: string | null;
  plays: number;
}

export interface Planet {
  id: number;
  name: string;
  source: Source;
  minutes: number;
  plays: number;
  radius: number;
  position: [number, number, number];
  galaxy: number;
  interests: string[];
  url: string | null;
  topItems: TopItem[];
}

export interface Link {
  a: number;
  b: number;
  strength: number;
  shared: string[];
}

export interface Galaxy {
  id: number;
  name: string;
  /** A share of the colour wheel, from 0 up to 1. */
  hue: number;
  size: number;
}

export interface Universe {
  filter: UniverseFilter;
  planets: Planet[];
  links: Link[];
  galaxies: Galaxy[];
}
