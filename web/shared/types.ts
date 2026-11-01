// Mirrors the Rust structs in crates/core/src/{model,digest,roast}.rs and universe/mod.rs, in camelCase.

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
  demoPlaysRemoved: number;
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

export interface BiggestDay {
  date: string;
  plays: number;
  minutes: number;
}

export interface Repeat {
  item: string;
  creator: string;
  times: number;
  date: string;
}

export interface CreatorStat {
  name: string;
  minutes: number;
  plays: number;
  interests: string[];
}

export interface ItemStat {
  title: string;
  creator: string;
  plays: number;
}

export interface SourceDigest {
  plays: number;
  minutes: number;
  minutesAreEstimates: boolean;
  creators: number;
  items: number;
  firstDay: string;
  lastDay: string;
  skipRatePercent: number;
  lateNightPercent: number;
  deletedVideos: number;
  biggestDay: BiggestDay | null;
  longestRepeat: Repeat | null;
  topCreators: CreatorStat[];
  topItems: ItemStat[];
}

export interface GalaxyDigest {
  id: number;
  name: string;
  size: number;
  members: string[];
}

export interface Digest {
  today: string;
  spotify: SourceDigest | null;
  youtube: SourceDigest | null;
  galaxies: GalaxyDigest[];
}

export interface Burn {
  title: string;
  body: string;
}

export interface Verdict {
  archetype: string;
  /** Out of 100. */
  score: number;
  sentence: string;
  evidence: string[];
}

export interface GalaxyName {
  id: number;
  name: string;
}

export interface Roast {
  headline: string;
  opening: string;
  burns: Burn[];
  verdict: Verdict;
  galaxyNames: GalaxyName[];
}

export interface StoredRoast {
  id: number;
  createdAt: number;
  providerLabel: string;
  roast: Roast;
}
