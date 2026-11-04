# Contributing

Thanks for helping. Rostery handles people's listening and watching history, so a change is judged first on what it does with that data.

## Before you start

- Read [GLOSSARY.md](GLOSSARY.md). Code, tests and copy use those words, such as Play, Creator, Planet and Digest.
- Read [docs/design.md](docs/design.md) for the part you are changing, and the [ADRs](docs/adr) behind it.
- For anything larger than a fix, open an issue first and say what you want to change.

## Ground rules

- **Nothing leaves the computer unless the screen says so first.** A change that sends anything new anywhere must add it to the "What leaves the machine" table in `docs/design.md`, to [PRIVACY.md](PRIVACY.md), and to the disclosure the app shows.
- **Tokens and keys go in the system keychain**, through `src-tauri/src/secrets.rs`, and never into the database, a file, a log or the webview.
- **Strangers write titles and names.** Anything from history, or from a Provider, is rendered as text only, never as HTML.
- **No new dependency** for what a few lines can do.
- Copy is plain English with no em dashes.

## Checks

Run these before you open a pull request. CI runs them on macOS and Linux.

```sh
pnpm build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check
pnpm test
pnpm test:e2e
```

Write the test first where you can, and see it fail. A bug fix comes with a test that fails without it.

## Pull requests

Keep each one to a single change, and say what it does and how you checked it. Screenshots help for anything on screen.
