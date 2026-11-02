# 07: Codex and local LLM Providers, plus settings

**What to build:** Choose Claude Code, Codex, Ollama or LM Studio in settings, each with live status and a hint when it is not ready. Edit the system prompt. See the exact Digest before it is sent. The Roast screen always says where the data goes.

**Blocked by:** 06

**Status:** done, apart from the open box, which waits for a real Ollama at stop point 3

A local server that wants an API key does not work yet. Task 14 adds the key, once Task 10 has brought the keychain.

- [x] Codex runs read-only and ephemeral on the user's own login
- [x] An OpenAI-compatible local server that wants no API key works on mock servers and a real `llama-server`, and a URL that is not loopback, or an Ollama cloud model, gets flagged
- [ ] A Roast works on a real Ollama, and a context length too short for it says so
- [x] Provider status is detected live, with a next step when something is missing
- [x] The system prompt can be edited and reset, and the edit is used by the next Roast
- [x] "See what gets sent" shows the exact Digest JSON
