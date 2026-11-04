# 15: Ship it

**What to build:** Someone finding the repo can understand it, install it on macOS or Linux, and contribute. CI proves it on both systems and tags produce release builds.

**Blocked by:** 14

**Status:** done

- [x] An original app icon
- [x] `pnpm tauri build` produces a working `.dmg` on macOS, and CI builds `.deb` and `.AppImage`
- [x] CI runs format, lint, Rust tests, Vitest and Playwright on macOS and Linux
- [x] README covers install, Exports, Providers, the honest privacy limits and the Spotify Connection warning
- [x] `LICENSE`, `CONTRIBUTING.md` and `SECURITY.md` exist

Stop point 6 follows this ticket.

The .dmg was built and launched on macOS. The .deb and .AppImage are built by the release workflow on Linux.
