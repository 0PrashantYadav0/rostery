# Claude Code and Codex run through their official CLIs

Users want to spend the Claude and ChatGPT subscriptions they already pay for. Rostery spawns the `claude` and `codex` binaries and lets them handle their own login. It never reads their OAuth tokens or calls the subscription backends directly, because that means handling credentials Rostery does not own and breaks whenever those tools change.

History contains text written by strangers. A video title can be a prompt injection, so a Provider gets no tools:

- `claude -p --output-format json --tools "" --strict-mcp-config --no-session-persistence --setting-sources ""`, run in an empty temp directory.
- `codex exec --sandbox read-only --ephemeral --ignore-rules --ignore-user-config --skip-git-repo-check -C <temp>`, also in an empty temp directory.

The Digest goes over stdin, because argv is visible in `ps`. We remove `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN` and `OPENAI_API_KEY` from the child environment so a stray key cannot switch the user from their subscription to metered billing.

## Consequences

We tested this on 2026-11-05 with Claude Code 2.1.310 and codex-cli 0.164.0. Claude's `--json-schema` returns no `structured_output` when `--tools ""` is set, because it works through a tool, so Rostery puts the JSON contract in the prompt and validates the reply. `--bare` is off limits since it skips the keychain read that subscription login needs. With `verbose` on in the CLI's global config, `--output-format json` prints an array of events and `--setting-sources ""` does not stop that, so Rostery reads the last `result` event.

Codex in read-only mode can still run read-only shell commands. Its instructions forbid that, but an injected prompt could try. Titles are shortened and stripped before they reach the Digest, and the user previews a Share Site before publishing it.

Hosted Providers receive the Digest. "Runs locally" is only fully true with a local LLM, and the app says so in plain words before the first call.
