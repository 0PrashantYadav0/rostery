import type { ImportSummary } from './types';

export const noImport: ImportSummary = {
  filesRead: 0,
  added: 0,
  duplicates: 0,
  excludedPrivate: 0,
  excludedAds: 0,
  rejected: 0,
};

export function addSummaries(a: ImportSummary, b: ImportSummary): ImportSummary {
  return {
    filesRead: a.filesRead + b.filesRead,
    added: a.added + b.added,
    duplicates: a.duplicates + b.duplicates,
    excludedPrivate: a.excludedPrivate + b.excludedPrivate,
    excludedAds: a.excludedAds + b.excludedAds,
    rejected: a.rejected + b.rejected,
  };
}

const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** An import in plain words, for example "Added 41,203 Plays. Skipped 12 private sessions." */
export function describeImport(s: ImportSummary): string {
  const parts = [`Added ${count(s.added, 'Play', 'Plays')}.`];
  if (s.duplicates) parts.push(`${count(s.duplicates, 'Play was', 'Plays were')} already here.`);
  if (s.excludedPrivate) parts.push(`Skipped ${count(s.excludedPrivate, 'private session', 'private sessions')}.`);
  if (s.excludedAds) parts.push(`Skipped ${count(s.excludedAds, 'ad', 'ads')}.`);
  if (s.rejected) parts.push(`Left out ${count(s.rejected, 'row', 'rows')} Rostery could not use.`);
  return parts.join(' ');
}
