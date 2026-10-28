// Mirrors the Rust structs in crates/core/src/model.rs, in camelCase.

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
