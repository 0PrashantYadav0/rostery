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
  /** Why the last Roast could not be read. This never stops a new one, which takes its place. */
  readError = $state('');
  latest = $state.raw<StoredRoast | null>(null);
  #asked = 0;

  /** The last answer wins, so a slow one from before a Roast landed cannot replace that Roast. */
  async load() {
    const asked = (this.#asked += 1);
    // A Roast that landed earlier is not news any more, and it may be gone by now.
    if (!this.running) this.phrase = '';
    try {
      const latest = await latestRoast();
      if (asked !== this.#asked) return;
      this.latest = latest;
      this.readError = '';
    } catch (e) {
      if (asked === this.#asked) this.readError = errorMessage(e);
    }
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
      this.readError = '';
      // Focus stays on the button, so without this a screen reader hears nothing when the wait ends.
      this.phrase = 'Your Roast is ready.';
    } catch (e) {
      this.error = errorMessage(e);
      this.phrase = '';
    }
    stop?.();
    this.running = false;
  }
}

export const roastRun = new RoastRun();
