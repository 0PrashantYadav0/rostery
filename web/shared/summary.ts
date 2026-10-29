import type { ImportSummary } from './types';

const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** An import in plain words, for example "Added 41,203 Plays. Skipped 12 private sessions." */
export function describeImport(s: ImportSummary): string {
  const parts = [`Added ${count(s.added, 'Play', 'Plays')}.`];
  if (s.duplicates) parts.push(`${count(s.duplicates, 'Play was', 'Plays were')} already here.`);
  if (s.excludedPrivate) parts.push(`Skipped ${count(s.excludedPrivate, 'private session', 'private sessions')}.`);
  if (s.excludedAds) parts.push(`Skipped ${count(s.excludedAds, 'ad', 'ads')}.`);
  if (s.rejected) parts.push(`Left out ${count(s.rejected, 'row', 'rows')} Rostery could not use.`);
  if (s.filesSkipped) parts.push(`Left out ${count(s.filesSkipped, 'file', 'files')} Rostery could not read.`);
  if (s.skippedReason) parts.push(s.skippedReason);
  return parts.join(' ');
}
