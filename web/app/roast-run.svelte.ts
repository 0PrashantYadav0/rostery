import { errorMessage, generateRoast, latestRoast, onRoastProgress, providerDisclosure } from '../shared/api';
import type { Disclosure, StoredRoast } from '../shared/types';

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
  /** Remembered here so that coming back to the screen draws it at once, with nothing moving after. */
  hasHistory = $state<boolean>();
  /** Where the next Roast would go. Settings keeps this up to date. */
  disclosure = $state.raw<Disclosure>();
  /** Where the running Roast went, which Settings can no longer change. */
  sendingTo = $state.raw<Disclosure>();
  /** How the last run ended, until the person has been to the Roast screen to see. */
  outcome = $state<'ready' | 'failed' | null>(null);
  /** A Roast the database would not take. It is all there is of it, so a read must not replace it. */
  #unsaved = false;
  #asked = 0;

  /** The last answer wins, so a slow one from before a Roast landed cannot replace that Roast. */
  async load() {
    const asked = (this.#asked += 1);
    // A Roast that landed earlier is not news any more, and it may be gone by now.
    if (!this.running) this.phrase = '';
    try {
      const latest = await latestRoast();
      if (asked !== this.#asked) return;
      if (!this.#unsaved) this.latest = latest;
      this.readError = '';
    } catch (e) {
      if (asked === this.#asked) this.readError = errorMessage(e);
    }
  }

  async loadDisclosure() {
    this.disclosure = await providerDisclosure();
  }

  async start() {
    if (this.running) return;
    this.running = true;
    this.sendingTo = this.disclosure;
    this.outcome = null;
    this.error = '';
    this.phrase = 'Reading your history';
    // Only the words on screen depend on this, so a Roast goes ahead without it.
    const stop = await onRoastProgress((phrase) => {
      if (this.running) this.phrase = phrase;
    }).catch(() => undefined);
    try {
      const { roast, notSaved } = await generateRoast();
      this.#asked += 1;
      this.latest = roast;
      this.#unsaved = notSaved !== null;
      this.error = notSaved ?? '';
      this.readError = '';
      // Focus stays on the button, so without this a screen reader hears nothing when the wait ends.
      this.phrase = 'Your Roast is ready.';
      this.outcome = 'ready';
    } catch (e) {
      this.error = errorMessage(e);
      this.phrase = '';
      this.outcome = 'failed';
    }
    stop?.();
    this.running = false;
  }
}

export const roastRun = new RoastRun();
