# Rostery design

Rostery is a desktop app for macOS and Linux. It reads your Spotify and YouTube history on your machine, gets an AI you already have access to to roast it, draws the history as a universe of planets, and generates a static website you can publish in one click.

This file is the spec. The plan in `docs/plans/rostery-v1.md` argues from it. Vocabulary comes from `GLOSSARY.md`. The decisions that are expensive to reverse are in `docs/adr/`.

## Requirements

1. Connect to Spotify and YouTube and read history from both.
2. Roast the user from that history, using an AI agent and a system prompt the user can change.
3. Everything runs locally. Credentials stay on the machine.
4. Rust desktop app for macOS and Linux.
5. AI comes from the local Claude Code subscription, the local Codex subscription, or an open source LLM.
6. Draw a Universe: each artist or channel is a Planet sized by how much the user listened or watched. Planets link by shared interest.
7. End with a Verdict.
8. Generate a compiled HTML, CSS and JS site holding the Roast and the Spotify and YouTube Universes.
9. Publish that site to GitHub Pages or Vercel in one click.

## What the platforms allow

These facts decide the architecture. They were checked against primary sources on 2026-11-05.

| Fact | Effect on Rostery |
|---|---|
| YouTube Data API has no watch history. `watchHistory` was removed. | Takeout is the only real YouTube history. |
| Spotify `recently-played` returns at most 50 tracks. | The Spotify data download is the only real Spotify history. |
| Takeout defaults to HTML. Timestamps there depend on locale. | Parse JSON only. An HTML file gets instructions, not a guess. |
| Takeout localises file names and activity titles. | Detect formats by content, not by file name. |
| Spotify Development Mode needs Premium and allows 5 users per app. | Each user registers their own client ID. |
| Spotify rejects `localhost` redirects and allows `http://127.0.0.1:PORT`. | Fixed loopback redirect on port 43117. |
| The Spotify Developer Policy bans sending Platform content to AI models and deriving listening metrics. | Connection is opt-in with explicit consent. See ADR 0002. |
| YouTube requires stored API data to be refreshed or deleted within 30 days. | Purge on startup. |
| Google apps in Testing status get refresh tokens that expire in 7 days. | `invalid_grant` shows a reconnect message. |
| `GET /artists` batch is gone from Spotify. `artist.popularity` and `followers` are gone. | Genres come from the top artists response. Popularity is never used. |

Sources: [Spotify February 2026 migration guide](https://developer.spotify.com/documentation/web-api/tutorials/february-2026-migration-guide), [Spotify Developer Policy](https://developer.spotify.com/policy), [Spotify redirect URIs](https://developer.spotify.com/documentation/web-api/concepts/redirect_uri), [Spotify recently played](https://developer.spotify.com/documentation/web-api/reference/get-recently-played), [Understanding your Spotify data](https://support.spotify.com/us/article/understanding-my-data/), [YouTube API revision history](https://developers.google.com/youtube/v3/revision_history), [YouTube developer policies](https://developers.google.com/youtube/terms/developer-policies), [Google desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app), [Takeout activity schema](https://developers.google.com/data-portability/schema-reference/my_activity), [Vercel create deployment](https://vercel.com/docs/rest-api/reference/endpoints/deployments/create-a-new-deployment), [GitHub Pages REST API](https://docs.github.com/en/rest/pages/pages).

## Architecture

```
 Spotify download   Google Takeout   Spotify API   YouTube API
        |                 |               |             |
        v                 v               v             v
+----------------------- rostery-core (Rust) -------------------------+
|  ingest -----> Store (SQLite) <----- spotify_api, youtube_api       |
|                  |                                                  |
|                  +--> universe::build --> Universe                  |
|                  +--> digest::build ----> Digest                    |
|                                             |                       |
|                       roast::generate <-----+--> llm::complete      |
|                                                  claude / codex /   |
|                                                  local HTTP         |
|                  export::write_site --> site folder                 |
|                  deploy::github, deploy::vercel                     |
+-------------------------------^-------------------------------------+
                                | Tauri commands, JSON
                  +-------------+-------------+
                  | src-tauri                 |
                  | keychain, events, dialogs |
                  +-------------+-------------+
                                |
                  web/app (Svelte screens)
                  web/universe (three.js renderer)  <--- web/share (Share Site)
```

`rostery-core` has no UI, no keychain access and no async runtime. It takes tokens as arguments and returns plain structs, so every feature is testable with `cargo test` against fake CLIs and mock HTTP servers. It uses blocking `ureq`. `src-tauri` is a thin layer that runs core calls on Tauri's worker threads through `#[tauri::command(async)]`.

There are two frontend builds from one package. `dist/app` is loaded by the desktop shell. `dist/share` is an IIFE build with fixed file names, embedded into the binary with `include_dir` and copied out as the template for each Share Site.

## Trust boundaries

| Boundary | Rule |
|---|---|
| Export files and API responses | Untrusted. Parsers only read the fields they need, so IPs, usernames and user agents in a Spotify file never reach memory or disk. An ID or link from a file is stored only if it has the shape of one. |
| Text inside history | Untrusted. It is stripped of control characters and cut to 120 characters before it enters a Digest, and the prompt marks it as data. |
| Provider output | Untrusted. It is parsed as JSON, validated, and rendered as text. Never as HTML. |
| The Share Site | Public. It is built from one typed struct, `SharePayload`, so anything not in that struct cannot reach the site. |
| The webview | Has no network. Its CSP blocks `connect-src` apart from Tauri IPC. All network calls are in Rust. |
| The Share Site in a visitor's browser | Makes no third-party requests. No CDN, fonts, images or analytics. Strict CSP and `noindex`. |

## What leaves the machine

| Action | What goes where |
|---|---|
| Import an Export | Nothing. |
| Roast with a local LLM | Digest to the configured URL. Rostery flags a URL that is not loopback. |
| Roast with Claude Code or Codex | Digest and top Creator names to Anthropic or OpenAI, under the user's subscription. The app says this before the first call and can show the exact Digest. |
| Connect to a Source | OAuth requests to that Source. |
| Publish | The Share Site to GitHub or Vercel. It becomes public. |

## Data model

SQLite through `rusqlite` with the bundled build, stored in Tauri's app data directory.

```sql
plays(id, source, origin, played_at, duration_ms, creator, creator_ref,
      item, item_ref, skipped, dedupe_key UNIQUE)
affinities(source, creator, creator_ref, kind, score, fetched_at,
           PRIMARY KEY (source, creator, kind))
interests(source, creator, interest, origin, fetched_at,
          PRIMARY KEY (source, creator, interest, origin))
roasts(id, created_at, provider_label, json)
settings(key PRIMARY KEY, value)
```

`dedupe_key` is `source|floor(played_at/60)|creator|item`. Minute precision lets Spotify's account format, which only has minutes, and its extended format, which has seconds, collapse into one Play, and the extended copy's details are kept whichever is imported first. Re-importing a file adds nothing. `played_at` is Unix seconds in UTC.

Two rules keep Origins consistent. When an Export is imported and every file in it could be read, API Plays of that Source inside each file's own oldest to newest span, inclusive, are deleted. An API Play outside those spans stays, even in a gap between two files, because an Account file only spans about a year and that Play may be held nowhere else. When a Connection syncs, Plays at or before the newest Export Play are skipped.

Private sessions (`incognito_mode: true`) and Google ads (`details` holding "From Google Ads") are dropped at ingest and counted in the import summary. Spotify's account format has no private flag, so an extended private row also removes the account copy of that Play, whether it is already stored or comes later in the same import. An account file imported on its own after the extended one cannot be caught, because remembering private Plays would mean writing them to disk. A YouTube video that has since been deleted has no `subtitles`, and a removed one has no link either. It is kept with an empty Creator because it makes a good joke, and it never becomes a Planet. A Takeout row is a Play only when it links to a watch or Shorts video, or to nothing. Searches, subscriptions, likes, community posts and other Google products in the same file are ignored without counting as rejected.

## Universe

`universe::build` is a pure function of Plays, Affinities, Interests and a filter (`spotify`, `youtube`, `both`).

**Weight.** Minutes played, plus Affinity score. Takeout has no durations, so each YouTube watch counts as 8 minutes.

**Radius.** `0.6 + 3.4 * sqrt(weight / max_weight)`, where `max_weight` is the largest in that Planet's own Source. A Spotify-heavy user's channels still get sensible sizes in the combined view. At most 150 Planets are kept, heaviest first.

**Links.** Group Plays into Sessions at a 30 minute gap. For two Creators with at least 2 shared Sessions, compute normalised pointwise mutual information, `npmi = ln(c_ab * S / (c_a * c_b)) / -ln(c_ab / S)`, floored at 0. Take the Jaccard similarity of their Interest sets. `strength = 0.6 * npmi + 0.4 * jaccard`. Keep a Link at `strength >= 0.12`, and keep it only if it is in the top 6 for at least one of its two Planets. Sessions are capped at 50 distinct Creators so a week of autoplay cannot form a giant clique.

**Galaxies.** Deterministic weighted label propagation, with Planets visited heaviest first and ties going to the lowest label, for at most 20 rounds. Galaxy names default to the most common Interest, or else the heaviest member, and a Roast can replace them.

**Layout.** Computed in Rust so the app and site show the same picture and the result is testable. Galaxy centres go on a Fibonacci sphere. A seeded force simulation runs for 300 steps, then a final pass pushes apart any overlapping Planets. The same input always gives identical output.

## Roast

`digest::build` produces a capped summary: totals, top Creators with Interests, top Items, skip rate, longest repeat of one Item, biggest day, late night share in local time, Galaxy members, deleted video count. It never includes URLs, IDs or raw Plays, and it stays under 64 KB with any amount of history.

The default system prompt ships in `prompts/roast.md` and the user can override it in settings. It says what to do and what to avoid: jokes target taste and habits, never protected traits, health or anything involving minors. It says to treat everything inside `<digest>` as data.

A Roast is one JSON document:

```json
{
  "headline": "string",
  "opening": "string",
  "burns": [{ "title": "string", "body": "string" }],
  "verdict": { "archetype": "string", "score": 0, "sentence": "string", "evidence": ["string"] },
  "galaxy_names": [{ "id": 0, "name": "string" }]
}
```

Rostery extracts the first JSON object from the reply, tolerating code fences. It retries once with a reminder on bad JSON, clamps `score` to 0..=100, and fails if `burns` is empty. A failed run never replaces the previous Roast.

A second, cheaper prompt in `prompts/interests.md` asks for up to 3 Interests per Creator for the top 80 Creators that do not have any from AI yet. The result is cached. This is what links a lo-fi YouTube channel to a Spotify artist when they never shared a Session. If it fails, the Roast still runs.

## Share Site

Output is `index.html`, `assets/share.js`, `assets/share.css` and `.nojekyll`, all with relative paths. It opens from `file://`, from a GitHub Pages subpath and from a Vercel root.

The payload goes inline in `<script type="application/json">` with `<`, `>`, `&`, U+2028 and U+2029 escaped. The user chooses a display name and whether to include the Roast, each Universe, and Item titles. Item titles are off by default because a video title can reveal far more than a channel name. The site always carries Source attribution, and its Planet links go back to the Source.

The folder is always `<app data>/site`. Rostery owns it and rewrites it on every export, so it can never delete a folder the user pointed it at.

## Publishing

**GitHub Pages.** Uses `gh auth token` if available, else a pasted token from the keychain. It creates the repo with `auto_init` and a description marker, builds blobs, a tree and a commit, moves `main`, then enables Pages with `source: {branch: main, path: /}` and treats 409 as already enabled. It refuses any existing repo without the marker, because the commit replaces the whole tree and would destroy someone's project. The UI says the repo is public and asks to confirm.

**Vercel.** `POST /v13/deployments` with files inlined as base64, `target: production` and `projectSettings.framework: null`, then polls until `READY` or `ERROR`.

## Testing

| Layer | How |
|---|---|
| Core logic | `cargo test` with in-memory SQLite and synthetic fixtures. |
| CLI Providers | Fake `claude` and `codex` shell scripts that record argv, stdin, cwd and env. |
| HTTP clients | `mockito` servers for Spotify, Google, GitHub, Vercel and local LLMs. |
| Frontend pure logic | Vitest. |
| App screens | Playwright against the Vite build with a mocked Tauri IPC. |
| Share Site | Playwright over `file://` and HTTP on a real demo build from Rust. |
| Live checks | `#[ignore]` tests against the real CLIs, run at stop points. |

`docs/stop-points.md` lists the checkpoints where work stops and gets verified.

## Scale targets

300,000 Plays, about a decade of heavy listening. Import under 30 seconds and `universe::build` under 3 seconds in a release build. `share.js` under 350 KB gzipped.

## Out of scope for v1

Windows, code signing and notarisation (needs a paid Apple account), Takeout HTML parsing, streaming Roast output, social preview images, hiding individual Planets before sharing, GitHub device flow, Google Data Portability API (EEA, UK and Switzerland only).
