# Privacy

Rostery runs on your computer. There is no Rostery server, no account, no analytics and no crash reporting. The project never receives your history, your Roast or your tokens. This page says exactly what leaves your computer, when, and how to remove it.

## What Rostery keeps, and where

- **Your history.** Plays from your Spotify Export and Google Takeout, and anything a Connection adds, sit in a SQLite database in Rostery's app data folder. On macOS that is `~/Library/Application Support/dev.rostery.app/`. On Linux it is `~/.local/share/dev.rostery.app/`.
- **Your Roasts and settings.** In the same database.
- **Tokens and keys.** GitHub, Vercel, Spotify and Google tokens, a Google client secret and any local server API key go in your system keychain: Keychain on macOS, and a Secret Service such as GNOME Keyring or KWallet on Linux. They are never written to the database, a file or the page.
- **The Share Site.** The last exported site sits in a `site` folder beside the database. Each export replaces it.

## What leaves your computer

| When you | What goes where |
|---|---|
| Import an Export | Nothing. |
| Roast with Ollama or LM Studio | A Digest of your history goes to the address in Settings. On `127.0.0.1` it stays on your computer, unless you pick an Ollama cloud model, which Ollama sends on to ollama.com. Rostery warns about cloud models, and about any address that is not this computer, and names where it goes. |
| Roast with Claude Code | A Digest goes to Anthropic, under your own Claude subscription, through the `claude` command. Claude Code adds context of its own, such as the date and its working folder, which Rostery keeps empty. |
| Roast with Codex | A Digest goes to OpenAI, under your own ChatGPT subscription, through the `codex` command. Codex also sends your own `~/.codex/AGENTS.md` if you have one, and Rostery cannot stop that. |
| Tag Creators, before a Roast | Up to 80 artist and channel names, each with Spotify or YouTube beside it, go to the same place as the Digest. No titles and no counts. Each name goes once. |
| Open Settings, or press Check again | Each command line tool checks its own login. A local server is asked which models it has. |
| Connect Spotify or YouTube | Sign-in and API requests go to Spotify or Google, under the app you registered yourself. |
| Open a Planet's link | Your browser opens that artist or channel on Spotify or YouTube. |
| Export a Share Site | Nothing. The folder is written on your computer. |
| Publish | The Share Site goes to GitHub Pages or Vercel, and becomes public. |

A Digest holds counts, dates, artist and channel names, song and video titles, and Interests. Rostery removes most web addresses, IDs and hidden characters from every name and title first. Settings has a "See what gets sent" button that shows the exact Digest and names before anything goes.

If you start Rostery from a terminal where `ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK` or a similar setting is exported, Claude Code follows it. That is your own Claude Code setup. Starting Rostery from Finder or your desktop does not pass those along.

## Spotify

Connecting Spotify is optional, and an Export is better. Spotify's Developer Policy forbids analysing Spotify Content into derived listening metrics, and ingesting it into an AI model. A Universe and a Roast do both, so Rostery asks you to read those words and agree before it connects, and the app is your own. See [ADR 0002](docs/adr/0002-exports-are-the-primary-history.md).

What a Spotify Connection adds: your last 50 plays, your top artists and their genres. To remove it, press **Disconnect** under Connections on the Sources screen. That deletes the token from your keychain and every play, artist and genre the Connection added. Your Export stays. You can also remove Rostery's access from your Spotify account page under Apps.

## YouTube

What a YouTube Connection adds: your subscriptions, liked videos and the topics of those channels. YouTube's API has no watch history, so that only ever comes from your Takeout. By connecting you agree to the [YouTube Terms of Service](https://www.youtube.com/t/terms), and Google's use of your data follows the [Google Privacy Policy](https://policies.google.com/privacy).

Rostery deletes YouTube API data older than 30 days each time it starts, as YouTube's policies require. To remove it sooner, press **Disconnect**, which deletes the tokens and every subscription, like and topic the Connection added. Your Takeout stays. You can also remove access at [myaccount.google.com/permissions](https://myaccount.google.com/permissions).

## Sharing

A Share Site holds only what you choose on the Share screen. Song and video titles are left out unless you tick them. Your Roast is included unless you untick it, and it may quote titles and names from all your history whatever else you choose, so read it first. The site loads nothing from anywhere else and asks search engines not to index it. Once published, it is public, and Rostery cannot take it down: delete the repo on GitHub or the project on Vercel to remove it.

## Deleting everything

Settings has **Delete everything**. After you type DELETE, it removes every row in the database, every token and key Rostery keeps in your keychain, and the Share Site folder. Sites you already published stay published. To remove Rostery completely, also delete the app and its app data folder.

## Questions

Rostery is open source, so everything above can be checked in the code. Report a privacy problem as described in [SECURITY.md](SECURITY.md).
