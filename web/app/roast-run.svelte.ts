import { errorMessage, generateRoast, latestRoast, onRoastProgress } from '../shared/api';
import type { StoredRoast } from '../shared/types';

/**
 * The Roast and the one being written. It lives out here because the Roast screen is gone whenever the
 * person looks elsewhere, and a Roast takes a minute or more.
 */
class RoastRun {
  running = $state(false);
  phrase = $state('');
  error = $state('');
  latest = $state.raw<StoredRoast | null>(null);
  #asked = 0;

  /** The last answer wins, so a slow one from before a Roast landed cannot replace that Roast. */
  async load() {
    const asked = (this.#asked += 1);
    const latest = await latestRoast();
    if (asked === this.#asked) this.latest = latest;
  }

  async start() {
    if (this.running) return;
    this.running = true;
    this.error = '';
    this.phrase = 'Reading your history';
    // Only the words on screen depend on this, so a Roast goes ahead without it.
    const stop = await onRoastProgress((phrase) => {
      if (this.running) this.phrase = phrase;
    }).catch(() => undefined);
    try {
      const saved = await generateRoast();
      this.#asked += 1;
      this.latest = saved;
    } catch (e) {
      this.error = errorMessage(e);
    }
    stop?.();
    this.running = false;
    this.phrase = '';
  }
}

export const roastRun = new RoastRun();
