# Stop points

Work stops at each of these until the checks pass. "Automated" checks run from this repo. "You" checks need your accounts or your eyes, and development does not wait on them.

`export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"` if `cargo` is not on your `PATH`.

## 1. Real history gets in (after tickets 01 to 03)

Automated:

- `cargo test -p rostery-core`
- `cargo test --release -p rostery-core -- --ignored import_300k` finishes under 30 seconds
- `pnpm test:e2e` passes

You: download your Spotify data (Account, Privacy settings) and your Google Takeout history with the format set to JSON. Run `pnpm tauri dev`, drop both files in, and check the totals look right. Check that the same file dropped twice adds nothing.

## 2. The Universe draws (after tickets 04 and 05)

Automated:

- `cargo test --release -p rostery-core -- --ignored universe_300k` finishes under 3 seconds
- `pnpm test:e2e` passes, including the no-WebGL fallback
- Screenshot of the demo Universe gets reviewed by eye

You: load your real history. Does your most-played artist have the biggest Planet? Do the Galaxies feel like your actual taste?

## 3. The Roast is funny (after tickets 06 to 08)

Automated:

- `cargo test -p rostery-core`, with fake CLI and mock HTTP tests
- `cargo test -p rostery-core -- --ignored live_` against the real `claude` and `codex` on demo data
- Local LLM is covered by mock tests here because Ollama is not installed on the dev machine

You: run a Roast on your real history with each Provider you have. Install Ollama and try one model. With its default context length, a Roast should stop and say how to raise it, naming `OLLAMA_CONTEXT_LENGTH`. Raise it and try again, then check that an edit to the system prompt shows in the Roast. Tick the open box in ticket 07 once that works. If you have LM Studio, run two Roasts in a row, because Rostery has not seen whether its token count leaves out what it cached, and that would stop the second one by mistake. Sign in to Ollama and pull a cloud model such as `gpt-oss:20b-cloud`. Press Check again: the Ollama row should leave it out of the model list and never fill it in. Type its name in the Model box: Settings and the Roast screen should both warn that the Digest goes to ollama.com. Rostery goes by the name, so also try an alias made with `ollama create` from that model. It is not caught and still says the Digest stays here, so decide whether that needs fixing. Read the default prompt in `prompts/roast.md` and change anything that is not funny.

## 4. The site can be shared (after tickets 09 to 11)

Automated:

- `pnpm test:e2e` covers `file://`, a subpath over HTTP, a hostile-string payload, and no third-party requests
- `share.js` is under 350 KB gzipped
- GitHub and Vercel flows pass against mock servers

You: this one matters, because publishing is public. Export, preview in your browser, then publish once to GitHub Pages and once to Vercel. Confirm the URLs work and show only what you chose.

## 5. Connections work (after tickets 12 and 13)

Automated:

- OAuth and sync pass against mock servers, including the PKCE test vector, a wrong `state`, a busy port and expired tokens

You: these need your own credentials. Register a Spotify app (Premium needed) with redirect `http://127.0.0.1:43117/callback`, and a Google Cloud desktop OAuth client with the YouTube Data API on. Connect both, sync, then disconnect, and confirm API data is gone. Read ADR 0002 first.

## 6. Ready to ship (after tickets 14 and 15)

Automated:

- Full test suite, `cargo clippy -- -D warnings`, `cargo fmt --check`
- `pnpm tauri build` produces a `.dmg` and the app launches
- Whole-branch review is done
- CI passes on macOS and Linux after the push

You: install the `.dmg`. The app is unsigned, so macOS blocks the first launch. Right-click, then Open. Start from the first-run screen and go through the whole flow once.
