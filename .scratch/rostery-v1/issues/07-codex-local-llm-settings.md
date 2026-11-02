# 07: Codex and local LLM Providers, plus settings

**What to build:** Choose Claude Code, Codex, Ollama or LM Studio in settings, each with live status and a hint when it is not ready. Edit the system prompt. See the exact Digest before it is sent. The Roast screen always says where the data goes.

**Blocked by:** 06

**Status:** done

- [x] Codex runs read-only and ephemeral on the user's own login
- [x] Any OpenAI-compatible local server works, and a URL that is not loopback gets flagged
- [x] Provider status is detected live, with a next step when something is missing
- [x] The system prompt can be edited and reset, and the edit is used by the next Roast
- [x] "See what gets sent" shows the exact Digest JSON
