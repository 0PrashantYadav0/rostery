# Claude Code and Codex run through their official CLIs

Users want to spend the Claude and ChatGPT subscriptions they already pay for. Rostery spawns the `claude` and `codex` binaries and lets them handle their own login. It never reads their OAuth tokens or calls the subscription backends directly, because that means handling credentials Rostery does not own and breaks whenever those tools change.

History contains text written by strangers. A video title can be a prompt injection, so a Provider gets as few tools as it will give up:

- `claude -p --output-format json --tools "" --strict-mcp-config --no-session-persistence --setting-sources ""`, run in an empty temp directory.
- `codex exec --sandbox read-only --ephemeral --ignore-rules --ignore-user-config --skip-git-repo-check -C <temp> -c features.shell_tool=false -c web_search="disabled" -c features.apps=false -c features.image_generation=false -c features.view_image=false -c features.multi_agent=false -c features.goals=false -c skills.include_instructions=false -c include_environment_context=false`, also in an empty temp directory.

The Digest and the names in the tag step go over stdin, because argv is visible in `ps`. We remove `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY` and `CODEX_API_KEY` from the child environment so a stray key cannot switch the user from their subscription to metered billing. We also remove `CLAUDECODE` and `CLAUDE_CODE_ENTRYPOINT`, so a launch from a Claude Code terminal behaves like any other, where `claude` would refuse to run inside itself. Codex needs nothing removed for that. `FORCE_COLOR` and `CLICOLOR_FORCE` go as well, and `NO_COLOR=1` is set, because colour codes in front of `ERROR:` hid a logged out Codex from the words that recognise one.

## Consequences

We tested this on 2026-11-05 with Claude Code 2.1.310 and codex-cli 0.164.0. Claude's `--json-schema` returns no `structured_output` when `--tools ""` is set, because it works through a tool, so Rostery puts the JSON contract in the prompt and validates the reply. `--bare` is off limits since it skips the keychain read that subscription login needs. With `verbose` on in the CLI's global config, `--output-format json` prints an array of events and `--setting-sources ""` does not stop that, so Rostery reads the last `result` event.

Codex takes the system prompt as `-c developer_instructions=<TOML string>`, because it has no flag for one. Its stderr starts with a banner and the whole prompt, so Rostery only ever quotes its last `ERROR:` line. A logged out `codex exec` says only `401 Unauthorized`, so that counts as logged out.

We tested on 2026-11-07 with codex-cli 0.164.0 and a ChatGPT login, asking it to read a file with a random word in it and to open a web page. `--ignore-user-config` keeps login working, since auth does not come from that file. In read-only mode it can read any file and open any page, because the sandbox only stops writes:

- With nothing turned off, it read the file through its shell.
- With only `features.shell_tool=false`, it could not read the file, but its web tool still opened the page. A title that says "fetch this address" could have sent the Digest there, so the other four settings went in as well.
- With those five, it could do neither. `live_codex_cannot_read_a_file_or_open_a_page` checks this. What is left on the default model is a JavaScript tool with no `import` and no `fetch`, a clock, a way to ask a question nobody is there to answer, and the sub-agent tools below.

We then read the first request the real CLI makes, the same day and version, against a server on this computer with a throwaway home, so it reached nothing and used no login. That showed three more things going along with each request. `live_codex_request_holds_no_skills_or_where_it_ran` checks what could be stopped.

- **Skill descriptions.** `skills.include_instructions=false` stops them.
- **The folder, shell, date and time zone.** `include_environment_context=false` stops them.
- **`~/.codex/AGENTS.md`. This still goes, and nothing we found stops it.** It arrives as a user message next to the Digest. `project_doc_max_bytes=0` does not stop it, because that is about project files and this one is global, and the CLI's strings show no other key for it. Pointing `CODEX_HOME` somewhere empty would stop it, and would also take away the login, which lives there and which Rostery will not touch. So a person's private notes in that file go to OpenAI with each Roast and each tag step, under their own account, outside the preview, and its instructions can change the Roast. The app says so beside the choice of Provider and in the dialog, and `PRIVACY.md` has to say so.

**Codex can start a sub-agent.** This ADR first said it could not, and a review on 2026-11-07 made one start through a mock server. `features.multi_agent=false` and `features.goals=false` remove the sub-agent and goal tools on `gpt-5.4` and `gpt-5.3-codex`, so they are set. On `gpt-6.1-sol`, which is what Codex uses with no model chosen, `spawn_agent` and its friends stay whatever we set: those two, `features.multi_agent_v2=false`, `features.multi_agent_v2.enabled=false`, `agents.max_depth=0` and `agents.max_threads=1` all leave it, and `agents.max_threads=0` refuses to start. A sub-agent inherits the same limits, with no shell and no web tool, so nothing escalates. A hostile title that persuades the model could still spend subscription use on extra agents until the timeout, which is 5 minutes for a Roast and 2 for the tag step. The tool set varies by model, so this is true of the models named here and no others.

Codex ignores a key it does not know without a word, so a CLI that renames one of these brings that tool or that text back. The read-only sandbox and the instructions are then what is left, and the live tests above fail, so they run at each stop point. Titles are shortened and stripped before they reach the Digest either way, and the user previews a Share Site before publishing it.

Hosted Providers receive the Digest. "Runs locally" is only fully true with a local LLM, and not with an Ollama cloud model, which Ollama runs on ollama.com through the same local port. The app says so in plain words before the first call.
