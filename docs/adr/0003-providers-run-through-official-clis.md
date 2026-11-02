# Claude Code and Codex run through their official CLIs

Users want to spend the Claude and ChatGPT subscriptions they already pay for. Rostery spawns the `claude` and `codex` binaries and lets them handle their own login. It never reads their OAuth tokens or calls the subscription backends directly, because that means handling credentials Rostery does not own and breaks whenever those tools change.

History contains text written by strangers. A video title can be a prompt injection, so a Provider gets no tools:

- `claude -p --output-format json --tools "" --strict-mcp-config --no-session-persistence --setting-sources ""`, run in an empty temp directory.
- `codex exec --sandbox read-only --ephemeral --ignore-rules --ignore-user-config --skip-git-repo-check -C <temp> -c features.shell_tool=false -c web_search="disabled" -c features.apps=false -c features.image_generation=false -c features.view_image=false`, also in an empty temp directory.

The Digest goes over stdin, because argv is visible in `ps`. We remove `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY` and `CODEX_API_KEY` from the child environment so a stray key cannot switch the user from their subscription to metered billing. We also remove `CLAUDECODE` and `CLAUDE_CODE_ENTRYPOINT`, so a launch from a Claude Code terminal behaves like any other, where `claude` would refuse to run inside itself. Codex needs nothing removed for that.

## Consequences

We tested this on 2026-11-05 with Claude Code 2.1.310 and codex-cli 0.164.0. Claude's `--json-schema` returns no `structured_output` when `--tools ""` is set, because it works through a tool, so Rostery puts the JSON contract in the prompt and validates the reply. `--bare` is off limits since it skips the keychain read that subscription login needs. With `verbose` on in the CLI's global config, `--output-format json` prints an array of events and `--setting-sources ""` does not stop that, so Rostery reads the last `result` event.

Codex takes the system prompt as `-c developer_instructions=<TOML string>`, because it has no flag for one. Its stderr starts with a banner and the whole prompt, so Rostery only ever quotes its last `ERROR:` line. A logged out `codex exec` says only `401 Unauthorized`, so that counts as logged out.

We tested on 2026-11-07 with codex-cli 0.164.0 and a ChatGPT login, asking it to read a file with a random word in it and to open a web page. `--ignore-user-config` keeps login working, since auth does not come from that file. In read-only mode it can read any file and open any page, because the sandbox only stops writes:

- With nothing turned off, it read the file through its shell.
- With only `features.shell_tool=false`, it could not read the file, but its web tool still opened the page. A title that says "fetch this address" could have sent the Digest there, so the other four settings went in as well.
- With all five, it could do neither, and it could not start a sub-agent either. What is left is a JavaScript tool with no `import` and no `fetch`, an empty list of MCP resources and a clock. `live_codex_cannot_read_a_file_or_open_a_page` checks this.

Codex ignores a key it does not know without a word, so a CLI that renames one of these brings that tool back. The read-only sandbox and the instructions are then what is left, and the live test above fails, so it runs at each stop point. Titles are shortened and stripped before they reach the Digest either way, and the user previews a Share Site before publishing it.

Hosted Providers receive the Digest. "Runs locally" is only fully true with a local LLM, and not with an Ollama cloud model, which Ollama runs on ollama.com through the same local port. The app says so in plain words before the first call.
