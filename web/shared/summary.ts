import type { ImportSummary, Universe } from './types';

const count = (n: number, one: string, many: string) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/** For example "60 Planets, 214 Links, 10 Galaxies". */
export function describeUniverse(u: Universe): string {
  return [
    count(u.planets.length, 'Planet', 'Planets'),
    count(u.links.length, 'Link', 'Links'),
    count(u.galaxies.length, 'Galaxy', 'Galaxies'),
  ].join(', ');
}

/** An import in plain words, for example "Added 41,203 Plays. Skipped 12 private sessions." */
export function describeImport(s: ImportSummary): string {
  const parts = [`Added ${count(s.added, 'Play', 'Plays')}.`];
  if (s.duplicates) parts.push(`${count(s.duplicates, 'Play was', 'Plays were')} already here.`);
  if (s.excludedPrivate) parts.push(`Skipped ${count(s.excludedPrivate, 'private session', 'private sessions')}.`);
  if (s.excludedAds) parts.push(`Skipped ${count(s.excludedAds, 'ad', 'ads')}.`);
  if (s.rejected) parts.push(`Left out ${count(s.rejected, 'row', 'rows')} Rostery could not use.`);
  if (s.filesSkipped) parts.push(`Left out ${count(s.filesSkipped, 'file', 'files')} Rostery could not read.`);
  if (s.skippedReason) parts.push(s.skippedReason);
  if (s.demoPlaysRemoved) parts.push('Removed the demo data and any Roast of it, so only your real history is left.');
  return parts.join(' ');
}
