// Mirrors the Rust structs in crates/core/src/{model,llm,roast}.rs and universe/mod.rs, in camelCase.

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

export interface FinishedRoast {
  /** Its `id` is 0 when it could not be saved. */
  roast: StoredRoast;
  /** The sentence to show when it could not be saved. */
  notSaved: string | null;
}

export type LocalServer = 'ollama' | 'lmStudio';

export type Provider =
  | { claudeCode: { model: string | null } }
  | { codex: { model: string | null } }
  | { local: { server: LocalServer; baseUrl: string; model: string } };

export type ProviderId = 'claude-code' | 'codex' | 'ollama' | 'lm-studio';

export interface Settings {
  provider: Provider;
  /** The editable part of the system prompt, when it is not the default. */
  systemPrompt: string | null;
  /** Every other Provider, set up the way it was last left. */
  others: Provider[];
  /** What was saved could not be read, so `provider` is a placeholder nobody chose, and no Roast can start until someone does. */
  chooseAgain: boolean;
}

export interface Disclosure {
  sendsTo: string;
  /** The Digest would not go where a local Provider suggests, or Rostery no longer knows where it should go, so `sendsTo` is a warning. */
  flagged: boolean;
}

export interface ProviderStatus extends Disclosure {
  id: ProviderId;
  available: boolean;
  signedIn: boolean;
  /** One sentence, with the next step when something is missing. */
  detail: string;
  models: string[];
}
