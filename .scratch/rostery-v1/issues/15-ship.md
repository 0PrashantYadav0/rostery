# 15: Ship it

**What to build:** Someone finding the repo can understand it, install it on macOS or Linux, and contribute. CI proves it on both systems and tags produce release builds.

**Blocked by:** 14

**Status:** ready-for-agent

- [ ] An original app icon
- [ ] `pnpm tauri build` produces a working `.dmg` on macOS, and CI builds `.deb` and `.AppImage`
- [ ] CI runs format, lint, Rust tests, Vitest and Playwright on macOS and Linux
- [ ] README covers install, Exports, Providers, the honest privacy limits and the Spotify Connection warning
- [ ] `LICENSE`, `CONTRIBUTING.md` and `SECURITY.md` exist

Stop point 6 follows this ticket.
