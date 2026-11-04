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
| Text inside history | Untrusted. It is stripped of control characters and cut to 120 characters before it enters a Digest or the tag step, and each prompt marks it as data. |
| Provider output | Untrusted. It is parsed as JSON, validated, and rendered as text. Never as HTML. Interests keep only letters, digits, spaces and hyphens. |
| The Share Site | Public. It is built from one typed struct, `SharePayload`, so anything not in that struct cannot reach the site. |
| The webview | Has no network. Its CSP blocks `connect-src` apart from Tauri IPC. All network calls are in Rust. |
| The Share Site in a visitor's browser | Makes no third-party requests. No CDN, fonts, images or analytics. Strict CSP and `noindex`. |

## What leaves the machine

| Action | What goes where |
|---|---|
| Import an Export | Nothing. |
| Roast with a local LLM | Digest to the configured URL, with no proxy and no redirect. On a loopback address it stays on this computer, unless the model is an Ollama cloud model. Ollama sends those to ollama.com through the same local port, so a name ending in `:cloud` or `-cloud` gets a warning that names Ollama's service and is never offered or filled in. Any other address gets a warning that names the host. |
| Roast with Claude Code or Codex | Digest to Anthropic or OpenAI, under the user's subscription. It holds song and video titles, Creator names, Interests and dates. The app says this before each call and can show the exact Digest. Codex also sends the person's own `~/.codex/AGENTS.md` if they have one, which nothing stops, and the app says so. ADR 0003 has what was tried. |
| The tag step, before a Roast | Up to 80 Creator names, each with its Source, to the same place the Digest goes. Each name goes through the Digest's `sanitize`. No Items and no counts. Each Creator goes once. The disclosure says so, and the See what gets sent dialog shows the exact names that would go next. |
| Open Settings, or press Check again | A CLI's own login check, and `GET /models` to a local server. An address that is not this computer is only called once it is the chosen Provider, because it was saved on some other day and may be somebody else's device on this network. |
| Connect to a Source | OAuth requests to that Source. |
| Open a Planet's link | The system browser opens that artist or channel on Spotify or YouTube. |
| Open the Share screen | With a saved GitHub token or a signed-in `gh`, `GET /user` to GitHub to name the account. |
| Publish | The Share Site to GitHub or Vercel. It becomes public. Each GitHub publish is a parentless commit forced onto the branch, and Rostery only writes to a repo or Vercel project it made. |

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
demo_roasts(roast_id PRIMARY KEY)
settings(key PRIMARY KEY, value)
```

`dedupe_key` is `source|floor(played_at/60)|creator|item`. Minute precision lets Spotify's account format, which only has minutes, and its extended format, which has seconds, collapse into one Play, and the extended copy's details are kept whichever is imported first. A YouTube Export Play is keyed `youtube|played_at` instead, because Takeout writes a watch's title, channel and wording when it exports, so a later Takeout can repeat a watch in other words. Its times have seconds, and two watches begun in the same second count once. Demo Plays keep the first form, so made-up history cannot hide a real watch. Re-importing a file adds nothing. `played_at` is Unix seconds in UTC.

Two rules keep Origins consistent. When an Export is imported and every file in it could be read, API Plays of that Source inside each file's own oldest to newest span, inclusive, are deleted. An API Play outside those spans stays, even in a gap between two files, because an Account file only spans about a year and that Play may be held nowhere else. When a Connection syncs, Plays at or before the newest Export Play are skipped.

Private sessions (`incognito_mode: true`) and Google ads (`details` holding "From Google Ads") are dropped at ingest and counted in the import summary. Spotify's account format has no private flag, so an extended private row also removes the account copy of that Play, whether it is already stored or comes later in the same import. An account file imported on its own after the extended one cannot be caught, because remembering private Plays would mean writing them to disk. A YouTube video that has since been deleted has no `subtitles`, and a removed one has no link either. It is kept with an empty Creator because it makes a good joke, and it never becomes a Planet. A Takeout row is a Play only when it links to a watch or Shorts video, or to nothing. Searches, subscriptions, likes, community posts and other Google products in the same file are ignored without counting as rejected.

An `interests` row with the origin `ai` and an empty `interest` says a Provider answered about that Creator and had no tags for it, so it is not sent again. It sits in this table so that whatever clears a Creator's tags clears it too, and `Store::interests` leaves it out.

Clearing demo data removes demo Plays, demo Interests, demo Affinities, AI Interests on demo Creators and any Roast of them in one transaction, because an AI tag for a made-up artist must not survive into real history. A real Creator with the same name as a demo one loses its tag as well, which only means being asked about once more. Tags that come back after the demo has left are dropped, since a Provider takes a while and the demo can leave while it works. Affinities have no Origin column, so a demo one has the kind `demo`. `demo_roasts` lists each Roast whose Digest was built while demo Plays were stored. An import that adds at least one Play clears demo data by itself and says so in its summary, and a Connection's sync will do the same, because a Roast of made-up Plays mixed with real ones would be wrong. For the same reason, demo data refuses to load once any real Play is stored, and the app stops offering it.

## Universe

`universe::build` is a pure function of Plays, Affinities, Interests and a filter (`spotify`, `youtube`, `both`).

**Weight.** Minutes played, plus Affinity score. Takeout has no durations, so each YouTube watch counts as 8 minutes. A skipped Play (Spotify under 30 seconds) still adds its minutes, but it does not count as a Play on the Planet, among the Planet's top Items, or toward shared Sessions, since a skip does not show interest. It does keep a Session going, because a Session is a run of Plays.

**Radius.** `0.6 + 3.4 * sqrt(weight / max_weight)`, where `max_weight` is the largest in that Planet's own Source. A Spotify-heavy user's channels still get sensible sizes in the combined view. At most 150 Planets are kept, heaviest first. In the combined view, each Source first holds up to 40 places for its heaviest Creators with a weight of at least 30 minutes, and the rest go by weight.

**Links.** Group Plays into Sessions at a 30 minute gap. For two Creators with at least 2 shared Sessions, compute normalised pointwise mutual information, `npmi = ln(c_ab * S / (c_a * c_b)) / -ln(c_ab / S)`, floored at 0. Take the overlap of their Interest sets, the number shared divided by the size of the larger set, so one shared tag of three is enough for two Creators who never shared a Session, and a Creator with one tag does not link to everything that has it. `strength = 0.6 * npmi + 0.4 * overlap`. Keep a Link at `strength >= 0.12`, and keep it only if it is in the top 6 for at least one of its two Planets. Sessions are capped at 50 distinct Creators so a week of autoplay cannot form a giant clique. Sessions and the count `S` only use Plays in view, so two Sources can share a Session only in the combined view. For two Creators who are in every Session the formula is 0 / 0, and `npmi` is 1.

**Galaxies.** Deterministic weighted label propagation, with Planets visited heaviest first and ties going to the lowest label, for at most 20 rounds. Galaxy names default to the most common Interest no heavier Galaxy has, or else the heaviest member, so no two default names repeat, and a Roast can replace them.

**Roast names in each view.** A Roast names Galaxies by id from the Both view, because that is what a Digest holds. Ids follow the heaviest member, so an import can shift every one of them. So when a Roast is written, Rostery ties each name to its Galaxy's heaviest member in that Digest, its anchor, which never goes to a Provider. In the Both view a name goes to the Galaxy holding its anchor, and a name whose anchor has gone names nothing. Each view works out Galaxies of its own, so the Spotify and YouTube views have other ids. In a Source's view, each Galaxy takes its members heaviest first, finds the first that is in Both, and uses the name of that member's Galaxy, if the Roast gave it one. Names must not repeat, so each goes to the heaviest Galaxy that claims it and the rest keep their defaults. A name for an id the Digest did not hold is dropped when the Roast is written. `universe::from_store` is the one function that does this, and everything that shows a Universe to a person goes through it. A Digest is built without these names, so the ids and working names a Provider sees do not depend on the last Roast.

**Way back to the Source.** A Planet and its top Items carry a URL on the Source. Each is rebuilt from an ID checked for shape, because the text came from an untrusted file, so it can only be an `https` URL on `open.spotify.com` or `www.youtube.com`. A Spotify Export names no artist ID, so without one a Spotify Planet opens a search for its name. A YouTube Planet with no channel has no URL. In the app these open in the system browser, through a capability scoped to those two hosts, and the app's window will not navigate anywhere but its own pages. On a Share Site they are plain links.

**Layout.** Computed in Rust so the app and site show the same picture and the result is testable. Galaxy centres go on a Fibonacci sphere. A seeded force simulation runs for 300 steps, then a final pass pushes apart any overlapping Planets. The same input always gives identical output.

## Roast

`digest::build` produces a capped summary: totals, top Creators with Interests, top Items, skip rate, longest repeat of one Item, biggest day, late night share in local time, Galaxy members, deleted video count. It holds song and video titles, Creator names, Interests and dates. It never includes raw Plays, and it stays under 64 KB with any amount of history. A filter takes out most web addresses and IDs from the titles and names in it, not all of them, because a title like `Node.js/Express` has to survive and text cannot always tell the two apart. The screen says "most".

The system prompt has two parts. The voice ships in `prompts/roast.md`, and the user can edit it in Settings, up to 20,000 characters, and reset it. It says what to do and what to avoid: jokes target taste and habits, never protected traits, health or anything involving minors. `prompts/roast-contract.md` is always added after it and is never shown for editing. It gives the shape of the reply and says to treat everything inside `<digest>` as data, so an edit cannot break parsing or that rule.

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

Rostery extracts the first JSON object from the reply that is a Roast, tolerating code fences and prose, and looks past everything up to a reasoning model's last `</think>`, because a draft Roast can sit in there. Nothing asks a local server for a `response_format`, because servers differ on which they accept. It retries once with a reminder on bad JSON, clamps `score` to 0..=100, fails if `burns` is empty, and caps every field and list, with at most 12 Burns, so a Provider cannot blow up the screen or a Share Site. A failed run never replaces the previous Roast.

One Roast runs at a time, which Rust enforces as well as the screen. It belongs to the app and not to the screen, so it survives moving between screens, and the rail marks it while it runs and when it ends or fails. The Provider and the prompt are read when it starts, so changing Settings during a Roast only changes the next one. A CLI gets 5 minutes and a local server 10, since a model on a CPU is slow. Quitting the app stops a CLI that is still running. If the demo data a Roast is about leaves while it runs, the Roast is not kept, and the error says to press Roast me. A finished Roast waits up to 2 minutes for a busy database, where everything else waits 5 seconds. If it still cannot be saved, it shows anyway, with a sentence saying it will be gone when the app closes.

Settings shows each Provider's state: whether its CLI is installed and logged in, or whether its server answers, with a next step when something is missing. Those checks spawn processes and call the network, so they run on a worker thread with a timeout each, 5 seconds for a CLI and 2 for a server, and the screen never waits for them. A server somewhere other than this computer is only asked once it is chosen, and until then its row says so. Settings stay in the `settings` key as JSON. A save runs one at a time and outlasts the screen that asked for it, so the next time Settings opens it builds on that save. If one fails with no screen left to say so, the next screen says it, and a system prompt that failed goes back in its box and gets tried again. A prompt also saves at a pause in the typing, because closing the window is not a blur and leaves no time for a last save.

Saved settings that cannot be read are older or damaged, and may have held a choice to keep history on this computer. So no Provider stands in for them. The Roast screen and Settings say so, no Roast starts, and nothing is chosen until the person chooses a Provider. Editing the prompt in the meantime does not count as choosing. A saved value that reads fine and names no Provider is treated the same way, and keeps its prompt and the rest of what it holds, because every other field has a default so a later version can add one with no migration.

A local server that wants an API key does not work yet. Its status and any Roast say so, and Task 14 adds the key to Settings. It will live in the keychain and never in the `settings` key.

A local server reads only what fits in its context length, and the OpenAI API has no way to ask for more. Ollama defaults to 4,096 tokens on most laptops and drops the start of a longer prompt, which is the voice, while still answering 200. So Rostery compares the `usage.prompt_tokens` in the reply with what it sent, and if some was clearly dropped it fails and says how to raise the context length, even when the reply looks fine. A server that refuses a long prompt with an error gets the same advice.

A second, cheaper prompt in `prompts/interests.md` asks for up to 3 Interests per Creator for the top 80 Creators a Provider has not answered about yet. This is what links a lo-fi YouTube channel to a Spotify artist when they never shared a Session. It is sent first, so the Digest and the Galaxies it holds have the new tags. It gets sanitised Creator names and their Source, and nothing else. Nobody edits this prompt in Settings, and it says that names are data even when they look like instructions. Codex gets a strict schema of its own, and the reply goes through the same parser as a Roast.

A tag gets normalised: lowercase and trimmed, with only Unicode letters and digits, spaces and hyphens. ASCII punctuation is removed. Anything else outside those drops the tag, because std cannot tell an emoji from a mark a word needs, like the virama in Hindi, and removing one of those would misspell it. A tag over 32 characters is dropped, a Creator keeps at most 3, and repeats go. A name in the reply that was not asked about is ignored. These tags are shown on Links and reach a Share Site, so they only ever go in as text.

A reply that parsed settles every Creator it was asked about, with or without tags, or a Provider that knows nothing about someone would be asked on every Roast. A failed call settles nobody, and the next Roast asks again. The step has its own timeout, 2 minutes for a CLI and 5 for a local server, so a stuck one cannot use up the wait before the Roast. A timeout or a failure goes to the log, the Roast goes on, and the Roast screen's waiting words say when this is happening. Quitting stops the CLI the same way as during a Roast.

## Share Site

Output is `index.html`, `assets/share.js`, `assets/share.css` and `.nojekyll`, all with relative paths. It opens from `file://`, from a GitHub Pages subpath and from a Vercel root.

The payload goes inline in `<script type="application/json">` with `<`, `>`, `&`, U+2028 and U+2029 escaped. The user chooses a display name and whether to include the Roast, each Universe, and Item titles. Item titles are off by default because a video title can reveal far more than a channel name. The Roast is on by default, because sharing it is the point of a Share Site and its author has already read it. It is free text that quotes Item titles and names from both Sources, so turning off Item titles or leaving out a Universe does not change its words, and nothing can filter it afterwards. The Share screen says so beside the choice, and nothing is published before the Preview. The site always carries Source attribution, and its Planet links go back to the Source.

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
