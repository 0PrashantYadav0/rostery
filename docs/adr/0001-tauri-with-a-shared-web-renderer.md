# Tauri with a renderer shared between the app and the Share Site

The desktop app has to run on macOS and Linux with its logic in Rust, and it also has to produce a static HTML, CSS and JS site that draws the same Universe. We use Tauri 2 with a Svelte and three.js frontend, because one renderer then serves both the app and the Share Site.

A pure Rust GUI (egui, iced, Slint) would avoid the webview, but we would have to write the Universe twice: once in Rust for the app and again in JS for the site. Everything except drawing stays in the `rostery-core` crate, which has no UI dependency and is testable with plain `cargo test`.

## Consequences

Linux users need WebKitGTK at runtime. macOS Finder launches do not inherit the shell `PATH`, so finding the `claude` and `codex` binaries needs its own step (see ADR 0003).
