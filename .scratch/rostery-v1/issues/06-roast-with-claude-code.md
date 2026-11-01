# 06: Roast through Claude Code

**What to build:** Press "Roast me" and the local Claude Code CLI, on the user's own subscription, writes a Roast and Verdict from a Digest of their history. The app shows Burns, an archetype and a score. If anything fails, the message says what to do and the last Roast stays.

**Blocked by:** 04

**Status:** done

- [x] A Digest holds aggregates and names, never URLs, IDs or raw Plays, and stays under 64 KB
- [x] Late-night stats use the user's local time
- [x] Claude Code runs with no tools, no settings, no session saved, in an empty folder, with the Digest on stdin and no API key in its environment
- [x] The CLI is found when the app is launched from Finder
- [x] Logged out, timed out, killed and non-JSON replies each give a useful message and keep the previous Roast
- [x] With no history, the app asks for some in place of calling the Provider
- [x] The app states that the Digest goes to Anthropic before the call
