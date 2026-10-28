import { describe, expect, it } from 'vitest';
import { addSummaries, describeImport, noImport } from './summary';

describe('describeImport', () => {
  it('says what was added and what was left out', () => {
    expect(describeImport({ ...noImport, filesRead: 2, added: 41, excludedPrivate: 12 })).toBe(
      'Added 41 Plays. Skipped 12 private sessions.',
    );
  });

  it('uses singular words and explains repeats', () => {
    expect(
      describeImport({ ...noImport, added: 1, duplicates: 1, excludedPrivate: 1, excludedAds: 1, rejected: 1 }),
    ).toBe(
      'Added 1 Play. 1 Play was already here. Skipped 1 private session. Skipped 1 ad. ' +
        'Left out 1 row Rostery could not use.',
    );
  });

  it('reports a repeat import as nothing new', () => {
    expect(describeImport({ ...noImport, duplicates: 300 })).toBe('Added 0 Plays. 300 Plays were already here.');
  });
});

it('adds the summaries of several files', () => {
  const a = { ...noImport, filesRead: 1, added: 5, duplicates: 1, excludedPrivate: 2 };
  const b = { ...noImport, filesRead: 3, added: 7, rejected: 4 };
  expect(addSummaries(a, b)).toEqual({ ...noImport, filesRead: 4, added: 12, duplicates: 1, excludedPrivate: 2, rejected: 4 });
});
