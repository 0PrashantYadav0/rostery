# No backend, and every credential belongs to the user

Rostery has no server, no account system and no telemetry. Spotify and Google OAuth use a client ID the user registers themselves, over a loopback redirect at `http://127.0.0.1:43117/callback`. Spotify rejects `localhost` as a redirect, so the literal IP is required. Secrets live in the OS keychain through the `keyring` crate. All network calls happen in Rust, and the webview's CSP blocks outgoing connections.

Setting up your own client ID takes a few minutes and is the worst part of onboarding. We accept that because a shared client ID is capped at 5 users on Spotify and would need Google's verification, and any backend would break the privacy promise. Exports avoid this setup entirely, which is one more reason they come first (ADR 0002).

Publishing works the same way. GitHub reuses `gh auth token` when the user is logged in, or else a pasted token. Vercel uses a pasted token. A GitHub device flow would feel better but needs a registered OAuth App, which can be added later without changing anything else.
