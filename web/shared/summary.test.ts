import { describe, expect, it } from 'vitest';
import { describeImport, describeUniverse } from './summary';
import type { ImportSummary, Universe } from './types';

const noImport: ImportSummary = {
  filesRead: 0,
  filesSkipped: 0,
  added: 0,
  duplicates: 0,
  excludedPrivate: 0,
  excludedAds: 0,
  rejected: 0,
  skippedReason: null,
};

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

  it('says how many files it could not read', () => {
    expect(describeImport({ ...noImport, added: 2, filesSkipped: 1 })).toBe(
      'Added 2 Plays. Left out 1 file Rostery could not read.',
    );
    expect(describeImport({ ...noImport, added: 2, filesSkipped: 3 })).toBe(
      'Added 2 Plays. Left out 3 files Rostery could not read.',
    );
  });

  it('says why a file was left out', () => {
    expect(describeImport({ ...noImport, added: 2, filesSkipped: 1, skippedReason: 'It is in HTML.' })).toBe(
      'Added 2 Plays. Left out 1 file Rostery could not read. It is in HTML.',
    );
  });

  it('reports a repeat import as nothing new', () => {
    expect(describeImport({ ...noImport, duplicates: 300 })).toBe('Added 0 Plays. 300 Plays were already here.');
  });
});

describe('describeUniverse', () => {
  const sized = (planets: number, links: number, galaxies: number) =>
    ({ planets: Array(planets), links: Array(links), galaxies: Array(galaxies) }) as unknown as Universe;

  it('counts each kind of thing', () => {
    expect(describeUniverse(sized(1500, 2, 3))).toBe('1,500 Planets, 2 Links, 3 Galaxies');
  });

  it('has a singular for each word', () => {
    expect(describeUniverse(sized(1, 1, 1))).toBe('1 Planet, 1 Link, 1 Galaxy');
  });
});
