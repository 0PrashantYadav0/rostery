# Rostery v1 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a macOS and Linux desktop app that roasts your Spotify and YouTube history, draws it as a Universe, and publishes both as a static site.

**Architecture:** A Rust `rostery-core` crate holds all logic with no UI. `src-tauri` is a thin Tauri 2 shell that adds the keychain and dialogs. One Vite package builds the Svelte app and the Share Site, and both use the same three.js renderer.

**Tech stack:** Rust stable, Tauri 2, rusqlite (bundled), ureq, serde, keyring, zip, include_dir, Svelte 5 (no SvelteKit), TypeScript, Vite, three.js, Vitest, Playwright, mockito, pnpm.

**Spec:** `docs/design.md`. Vocabulary: `GLOSSARY.md`. Decisions: `docs/adr/`. Tickets: `.scratch/rostery-v1/issues/`. Checkpoints: `docs/stop-points.md`.

## Global Constraints

- Rust tools: `export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"` in every shell that runs cargo. Do not edit shell profiles.
- App name `Rostery`, bundle identifier `dev.rostery.app`, core crate `rostery-core`, app crate `rostery-app`.
- Use the GLOSSARY.md words in code, UI copy and tests: Play, Item, Creator, Origin, Export, Connection, Affinity, Session, Interest, Universe, Planet, Link, Galaxy, Digest, Provider, Roast, Burn, Verdict, Share Site, Deploy Target. In particular, "Export" means a file from a Source. Rostery's own output is a "Share Site".
- `rostery-core` has no Tauri, keychain or async runtime dependency. HTTP goes through blocking `ureq`. Functions that call an API take the base URL as a parameter so tests can point them at `mockito`.
- All network calls are in Rust. The webview never fetches anything. Tauri CSP: `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src ipc: http://ipc.localhost`.
- No telemetry, no analytics, no third-party requests from the Share Site or app, no web fonts from a CDN.
- Lazy code: stdlib first, then an installed dependency, then the minimum new code. No interface with one implementation, no config for a value that never changes. Mark a deliberate corner cut with a `ponytail:` comment naming the ceiling.
- Test data is synthetic with fictional names and reserved IPs like `203.0.113.7`. Never commit real personal data.
- The product is original work. No logos or artwork from Spotify, YouTube or any existing app, and do not imitate another product's look.
- UI copy and docs are in plain English with no em dashes. Each new UI task invokes the `frontend-design` skill before writing components.
- Commits: conventional style (`feat:`, `fix:`, `test:`, `docs:`, `chore:`), small and frequent. A task should create 2 to 5 commits. Every message ends with this exact line after a blank line: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Commit on the current branch `feat/v1`. Never push, never change git config, never amend or rebase.
- Each task finishes by editing its ticket file in `.scratch/rostery-v1/issues/`: set `**Status:** done` and tick the boxes that passed.
- Before committing, run the full checks the task touches: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm test`, `pnpm test:e2e`. Test output must be clean. `src-tauri` needs `pnpm build` to have run once, since it embeds `dist/`.

## Review Focus

1. **Launched from Finder, no shell `PATH`.** A person with `claude` in `~/.local/bin` opens the app from the Dock. Provider detection has to find it. Pinned by `llm::tests::login_shell_path_finds_binary` in Task 6.
2. **A decade of history.** 300,000 Plays in several large JSON files inside a zip, as a heavy listener really has. Import and Universe stay inside their time budgets. Pinned by `import_300k` in Task 2 and `universe_300k` in Task 4.
3. **Hostile or odd text.** A video titled `</script><img src=x onerror=alert(1)>`, a title that says "ignore your instructions", a 10,000 character title, RTL text, emoji. Nothing executes, nothing breaks layout, the prompt stays bounded. Pinned by `titles_sanitized` in Task 6, `escapes_script_breakout` in Task 9, and the hostile payload e2e in Task 9.
4. **Almost no data.** No Plays, one Source, one Creator, a single Session. No crash, no `NaN` in positions, and a clear message in place of a Roast of nothing. Pinned by `sparse_inputs` in Task 4 and `empty_digest_refuses` in Task 6.
5. **A Provider misbehaves.** CLI logged out, killed, hangs, returns prose instead of JSON. The user gets a sentence saying what to do, and the last good Roast stays. Pinned by Task 6 tests.
6. **Publishing over something real.** A user names an existing repo they care about. Rostery must refuse. Pinned by `refuses_to_overwrite_foreign_repo` in Task 10.

## Shared contract

Later tasks build on these names. Task 1 creates them, and each later task adds to them.

```rust
// crates/core/src/model.rs. All derive Serialize, Deserialize, Clone, Debug, PartialEq.
pub enum Source { Spotify, Youtube }          // serde lowercase, also Eq + Hash + Copy
pub enum Origin { Export, Api, Demo }         // serde lowercase, also Eq + Copy
pub struct Play { pub source: Source, pub origin: Origin, pub played_at: i64,
                  pub duration_ms: Option<i64>, pub creator: String,
                  pub creator_ref: Option<String>, pub item: String,
                  pub item_ref: Option<String>, pub skipped: bool }
pub struct SourceTotals { pub plays: u64, pub minutes: u64, pub creators: u64,
                          pub first_play: Option<i64>, pub last_play: Option<i64> }
pub struct Totals { pub spotify: SourceTotals, pub youtube: SourceTotals }
pub struct Affinity { pub source: Source, pub creator: String, pub creator_ref: Option<String>,
                      pub kind: String, pub score: f64, pub fetched_at: i64 }
```

```rust
// crates/core/src/store.rs
impl Store {
    pub fn open(path: &Path) -> Result<Store>;
    pub fn open_in_memory() -> Result<Store>;
    pub fn insert_plays(&mut self, plays: &[Play]) -> Result<InsertCount>; // InsertCount { added: u64, duplicates: u64 }, one transaction
    pub fn totals(&self) -> Result<Totals>;
    pub fn plays(&self, source: Option<Source>) -> Result<Vec<Play>>;       // ordered by played_at
    pub fn delete_origin(&mut self, source: Source, origin: Origin) -> Result<u64>;
    pub fn get_setting<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>>;
    pub fn set_setting<T: Serialize>(&self, key: &str, value: &T) -> Result<()>;
}
```

`crates/core/src/error.rs` holds `pub enum Error` built with `thiserror`, `pub type Result<T>`, and `impl Serialize for Error` that serializes the display string, so a Tauri command can return `Result<T, Error>`. Error messages are written for the person using the app, with what to do next.

Tauri commands are `snake_case` in `src-tauri/src/commands.rs`. `web/shared/api.ts` wraps each one with a typed function. `web/shared/types.ts` mirrors the Rust structs in `camelCase`, so every struct crossing IPC or entering the Share Site has `#[serde(rename_all = "camelCase")]`.

---

### Task 1: Walking skeleton with demo data

Ticket 01. The whole stack from SQLite to a screen, using demo data.

**Files:**
- Create: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/{lib,error,model,store,demo}.rs`
- Create: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,icons/*,src/main.rs,src/commands.rs}`
- Create: `package.json`, `pnpm-lock.yaml`, `tsconfig.json`, `svelte.config.js`, `vite.config.ts`, `web/app/{index.html,main.ts,App.svelte,styles.css,screens/Sources.svelte}`, `web/shared/{api.ts,types.ts}`
- Create: `playwright.config.ts`, `e2e/mock-ipc.ts`, `e2e/app.spec.ts`

**Interfaces:**
- Produces: the Shared contract above. `demo::generate(seed: u64, now: i64) -> DemoData { plays: Vec<Play>, interests: Vec<(Source, String, Vec<String>)> }`. Commands `totals() -> Totals`, `load_demo() -> Totals`, `clear_demo() -> Totals`. `AppState { db_path: PathBuf }` managed by Tauri; each command opens its own `Store`. Scripts `pnpm dev`, `pnpm build`, `pnpm test`, `pnpm test:e2e`. `e2e/mock-ipc.ts` exports `installMockIpc(page, handlers)` built on `@tauri-apps/api/mocks`, which later tasks extend.

- [ ] **Step 1: Write failing store tests in `store.rs`**

`insert_is_idempotent`: inserting the same 3 Plays twice returns `added: 3` then `added: 0, duplicates: 3`. `same_minute_counts_once`: two Plays with the same Source, Creator and Item at `played_at` 1000 and 1019 (same minute) give one row, and one at 1020 gives a second. `totals_are_per_source`: Spotify minutes come from `duration_ms`, and a Creator counted twice is one in `creators`. `plays_are_ordered`.

- [ ] **Step 2: Run and see them fail**

Run: `cargo test -p rostery-core`. Expected: compile failure or FAIL.

- [ ] **Step 3: Implement `model.rs`, `error.rs`, `store.rs`**

Schema from the Data model section of the spec, created with `CREATE TABLE IF NOT EXISTS` on open, plus `PRAGMA journal_mode=WAL`. `dedupe_key` is `source|floor(played_at/60)|creator|item`, inserted with `INSERT OR IGNORE`.

- [ ] **Step 4: Write and implement `demo.rs`**

Test `generate_is_deterministic_and_rich`: the same seed gives equal output; at least 1000 Plays; at least 20 Creators per Source; every `played_at` before `now`. Use a small inline LCG, not the `rand` crate. Invent the names (for example "The Paper Lanterns", "Vanta Kids", "Slow Math Club"). Group fictional Creators in 5 taste groups, and pick a group per Session so that Links and Galaxies appear later. Use `Origin::Demo`.

- [ ] **Step 5: Create the Tauri shell and Svelte app**

`pnpm create` is not needed; write the files. `tauri.conf.json`: `productName` Rostery, identifier from Global Constraints, `frontendDist` `../dist/app`, `beforeDevCommand` `pnpm dev`, `beforeBuildCommand` `pnpm build`, CSP from Global Constraints, window 1200x800. `vite.config.ts`: root `web/app`, `outDir` `../../dist/app`, port 5173, strictPort. Generate placeholder icons with `pnpm tauri icon` from a generated PNG. Keep a real one for Task 15.

`App.svelte` has a left rail with Sources, Universe, Roast, Share, Settings. Only Sources works now. `Sources.svelte` shows totals per Source and a "Try demo data" button. Dark interface.

- [ ] **Step 6: Write the e2e test**

`e2e/app.spec.ts`: `demo button shows totals`. With a mocked IPC, click "Try demo data" and expect a visible Spotify count and a YouTube count. Playwright serves the built app with `vite preview`.

- [ ] **Step 7: Verify the real shell compiles and runs**

Run: `pnpm build && cargo test --workspace && pnpm tauri build --debug --no-bundle`. Expected: all pass, binary at `target/debug/rostery-app`. Launch it for 5 seconds and confirm it does not crash.

- [ ] **Step 8: Commit in logical pieces**

### Task 2: Import a Spotify Export

Ticket 02. Blocked by Task 1.

**Files:**
- Create: `crates/core/src/ingest/{mod,spotify_export}.rs`, `crates/core/tests/import_spotify.rs`
- Modify: `crates/core/src/{lib,store}.rs`, `src-tauri/src/commands.rs`, `src-tauri/Cargo.toml` (add `tauri-plugin-dialog`), `src-tauri/capabilities/default.json`, `web/app/screens/Sources.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Store`, `Play`.
- Produces: `ingest::import_path(store: &mut Store, path: &Path) -> Result<ImportSummary>`, where `ImportSummary { files_read: u32, added: u64, duplicates: u64, excluded_private: u64, excluded_ads: u64, rejected: u64 }` (camelCase serde). Accepts a `.json`, a `.zip` or a directory, recursively. `ingest::detect_format(head: &[u8]) -> Option<Format>` with `enum Format { SpotifyExtended, SpotifyAccount, TakeoutActivity }`, sniffing the first 4 KB for `"ms_played"`, `"msPlayed"`, or both `"titleUrl"` and `"products"`. `Store::replace_api_plays_before(source, ts)` runs after an Export import. Command `import_path(path: String) -> ImportSummary`.

- [ ] **Step 1: Write failing tests in `tests/import_spotify.rs`**

Build fixtures in code, including temp zips made with the `zip` crate:

- `extended_row_maps_fields`: `ts`, `ms_played`, `master_metadata_album_artist_name`, `master_metadata_track_name`, `spotify_track_uri` become `played_at`, `duration_ms`, `creator`, `item`, `item_ref`.
- `private_sessions_are_excluded`: `incognito_mode: true` is not stored and `excluded_private` is 1.
- `pii_never_reaches_disk`: a row has `ip_addr: "203.0.113.7"` and `username: "sentinel_user"`. After import into a file-backed Store and closing it, the DB file's bytes contain neither string.
- `podcasts_use_show_as_creator`: `episode_show_name` and `episode_name` map to `creator` and `item`.
- `short_plays_flagged_skipped`: `ms_played: 12000` gives `skipped: true`, and `skipped: true` in the row also does.
- `account_format_parses_minutes`: `{"endTime":"2024-03-01 13:05","artistName":"A","trackName":"T","msPlayed":200000}` gives the matching UTC epoch.
- `account_and_extended_do_not_double_count`: same Play in both formats in the same minute gives one row.
- `zip_reads_only_history`: a zip with `Userdata.json` holding a sentinel and one history file imports fine and `files_read` is 1.
- `rows_missing_time_are_rejected`: one bad row leaves `rejected: 1` and imports the others.
- `unrecognised_input_explains_what_to_do`: a random JSON file returns `Err` whose message names both exports.
- `export_replaces_overlapping_api_plays`: an `Origin::Api` Play older than the newest Export Play is deleted on import, and a newer one stays.
- `import_300k` (`#[ignore]`): 300,000 generated rows across 3 files in a zip import under 30 seconds in release.

- [ ] **Step 2: Run and see them fail**

Run: `cargo test -p rostery-core --test import_spotify`. Expected: compile failure.

- [ ] **Step 3: Implement**

Deserialize with `#[derive(Deserialize)]` structs that only declare the fields used, so PII never enters memory as named data. Read a matched zip entry up to a 1 GiB cap. Insert in one transaction per file.

- [ ] **Step 4: Run tests and the ignored release test**

Run: `cargo test -p rostery-core --test import_spotify` and `cargo test --release -p rostery-core --test import_spotify -- --ignored import_300k`. Expected: PASS.

- [ ] **Step 5: Add the import UI with the `frontend-design` skill**

`Sources.svelte` has a drop zone using Tauri's `onDragDropEvent` plus a file picker from the dialog plugin. It shows the summary in plain words ("Added 41,203 Plays. Skipped 12 private sessions."). Add a short how-to with the exact path in Spotify ("Account, Privacy settings, Download your data, Extended streaming history"). e2e test `import summary appears`, with the mocked command returning a summary.

- [ ] **Step 6: Run all checks and commit**

### Task 3: Import a YouTube Takeout Export

Ticket 03. Blocked by Task 2.

**Files:**
- Create: `crates/core/src/ingest/takeout.rs`, `crates/core/tests/import_takeout.rs`
- Modify: `crates/core/src/ingest/mod.rs`, `web/app/screens/Sources.svelte`

**Interfaces:**
- Consumes: `import_path`, `detect_format`, `ImportSummary`.
- Produces: `Format::TakeoutActivity` handling. Deleted videos are Plays with `creator: ""`.

- [ ] **Step 1: Write failing tests**

- `watch_entry_maps_fields`: `title: "Watched Night Drive Mix"`, `titleUrl: "https://www.youtube.com/watch?v=abc123"`, `subtitles: [{name, url}]`, `time` becomes `item: "Night Drive Mix"`, `item_ref: "abc123"`, `creator`, `creator_ref`, `played_at`, `duration_ms: None`.
- `ads_are_excluded`: `details: [{"name":"From Google Ads"}]` is not stored and `excluded_ads` is 1.
- `deleted_videos_have_empty_creator`: no `subtitles` gives `creator: ""`, stored.
- `topic_suffix_is_removed`: creator `"Some Band - Topic"` becomes `"Some Band"`.
- `non_watch_activity_is_skipped`: `titleUrl` with `results?search_query` or a post URL is ignored and not counted as rejected.
- `localised_file_name_still_imports`: `Takeout/YouTube/historial/historial-de-reproducciones.json` in a zip imports through content sniffing.
- `localised_title_is_kept_whole`: `"Visionné Foo"` stays as is.
- `html_export_explains_json`: `watch-history.html` returns `Err` whose message says to choose JSON in Takeout's format options.
- `mixed_folder_imports_both_sources`: a directory with a Spotify file and a Takeout file gives Plays for both.

- [ ] **Step 2: Run and see them fail**

Run: `cargo test -p rostery-core --test import_takeout`.

- [ ] **Step 3: Implement `takeout.rs`**

- [ ] **Step 4: Update the how-to copy**

Include the Takeout steps and the "change format to JSON" step, and show a clear note that YouTube's API cannot supply history. Add e2e `html file shows json guidance`.

- [ ] **Step 5: Run all checks and commit**

### Task 4: Universe engine and first render

Ticket 04. Blocked by Task 1. Needs Task 2's Store for real data, but demo data is enough to build it.

**Files:**
- Create: `crates/core/src/universe/{mod,graph,layout}.rs`, `web/universe/render.ts`, `web/app/screens/Universe.svelte`
- Modify: `crates/core/src/{lib,store}.rs`, `src-tauri/src/commands.rs`, `web/shared/*`, `web/app/App.svelte`, `package.json` (add `three`, `@types/three`), `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Play`, `Affinity`, `Store`.
- Produces, all camelCase serde:

```rust
pub enum UniverseFilter { Spotify, Youtube, Both }   // serde lowercase
pub struct Planet { pub id: u32, pub name: String, pub source: Source, pub minutes: f64,
                    pub plays: u64, pub radius: f64, pub position: [f64; 3], pub galaxy: u32,
                    pub interests: Vec<String>, pub url: Option<String>,
                    pub top_items: Vec<TopItem> }
pub struct TopItem { pub title: String, pub url: Option<String>, pub plays: u64 }
pub struct Link { pub a: u32, pub b: u32, pub strength: f64, pub shared: Vec<String> }
pub struct Galaxy { pub id: u32, pub name: String, pub hue: f64, pub size: u32 }
pub struct Universe { pub filter: UniverseFilter, pub planets: Vec<Planet>, pub links: Vec<Link>, pub galaxies: Vec<Galaxy> }
pub struct UniverseInput<'a> { pub plays: &'a [Play], pub affinities: &'a [Affinity],
                               pub interests: &'a HashMap<(Source, String), Vec<String>>,
                               pub galaxy_names: &'a HashMap<u32, String> }
pub fn build(input: &UniverseInput, filter: UniverseFilter) -> Universe;
```

Also `Store::affinities()`, `Store::set_affinities()`, `Store::interests() -> HashMap<(Source, String), Vec<String>>`, `Store::set_interests(source, creator, &[String], origin: &str, fetched_at)`, and the command `get_universe(filter) -> Universe`. `load_demo` also stores demo Interests. TS: `mountUniverse(container: HTMLElement, universe: Universe): UniverseHandle` with `{ setUniverse(u), onSelect(cb), destroy() }`.

- [ ] **Step 1: Write failing tests in `graph.rs` and `layout.rs`**

Constants from the spec's Universe section go in code with exactly those values.

- `planet_size_follows_time`: Creators with 400 and 100 minutes get radius 4.0 and 2.3.
- `youtube_watches_size_planets`: 10 watches gives 80 minutes and `plays: 10`.
- `sources_scale_independently`: in `Both`, the top Spotify and the top YouTube Planet both have radius 4.0.
- `filter_excludes_other_source`.
- `shared_sessions_make_a_link`: A and B share 2 Sessions and get a Link, with C elsewhere.
- `one_shared_session_is_not_enough`.
- `session_gap_boundary`: Plays 1800 seconds apart are one Session, 1801 seconds apart are two.
- `shared_interests_make_a_link`: A and B never share a Session and have the same 2 Interests; Link exists and `shared` lists them.
- `cross_source_link_only_in_both_view`.
- `galaxies_group_linked_planets`: two disconnected pairs give two Galaxies.
- `layout_is_deterministic`: two builds serialize identically.
- `no_nan_and_no_overlap`: 150 Planets all have finite positions and any two are at least `r1 + r2` apart.
- `sparse_inputs`: 0 Plays gives an empty Universe; 1 Play gives one Planet at a finite position; one Source under `Both` works.
- `deleted_videos_make_no_planet`: `creator: ""` is left out.
- `affinity_only_planet`: a top artist with no Plays gets a Planet.
- `planet_cap_keeps_heaviest`: 300 Creators give 150 Planets.
- `planet_urls_point_at_source`: a Play with `item_ref: "spotify:track:abc"` gives a `TopItem` URL `https://open.spotify.com/track/abc`. A YouTube `item_ref` gives `https://www.youtube.com/watch?v=<id>`.
- `universe_300k` (`#[ignore]`): under 3 seconds in release.

- [ ] **Step 2: Run and see them fail**

Run: `cargo test -p rostery-core universe`.

- [ ] **Step 3: Implement `graph.rs` and `layout.rs`**

Follow the formulas in the spec. Use a small inline PRNG seeded from Creator names, so layout is deterministic and needs no dependency. O(n²) repulsion is fine at 150 Planets.

- [ ] **Step 4: Run tests and the release timing test**

- [ ] **Step 5: Implement `render.ts` and `Universe.svelte`**

Spheres through `InstancedMesh` colored from the Galaxy hue, `LineSegments` for Links, a `Points` starfield, `OrbitControls`. e2e `universe draws something`: with a mocked Universe, the canvas exists and a screenshot has more than 50 distinct colors. Launch Chromium with `--use-angle=swiftshader`.

- [ ] **Step 6: Run all checks and commit**

### Task 5: Make the Universe explorable

Ticket 05. Blocked by Task 4.

**Files:**
- Create: `web/universe/{shaders.ts,labels.ts,scene-data.ts}`, `web/universe/scene-data.test.ts`, `web/universe/PlanetPanel.svelte`, `web/universe/PlanetList.svelte`
- Modify: `web/universe/render.ts`, `web/app/screens/Universe.svelte`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Universe`, `mountUniverse`, command `get_universe`.
- Produces: `UniverseView.svelte` (in `web/universe/`), the component both the app and Share Site mount: props `universes: Partial<Record<UniverseFilter, Universe>>`, `showItems: boolean`. `mountUniverse` gains `select(id)` and `flyTo(id)`.

- [ ] **Step 1: Invoke `frontend-design`, then write failing Vitest tests in `scene-data.test.ts`**

`topLabelIds` returns the 12 largest Planets. `formatWeight` shows Spotify as "412 hours" or "37 min" and YouTube as "128 watches". `neighbours` returns linked Planets sorted by strength. `planetSeed` is stable for a name.

- [ ] **Step 2: Implement the look and interactions**

Procedural GLSL surfaces per instance: Spotify Planets are banded gas giants, YouTube Planets are cratered rock with a thin ring, and the seed varies each one. Add an atmosphere rim, soft bloom-like glow without a postprocessing dependency, and Link opacity by strength. Labels are DOM elements projected for the top 12 and the hovered Planet. Hover shows a tooltip. Click flies the camera to the Planet and opens `PlanetPanel`: name, weight, Galaxy, Interests, top linked Planets with the reason ("shared Sessions" or the shared Interests), and an "Open in Spotify" or "Open on YouTube" link. Filter tabs for Spotify, YouTube and Both, showing only those available. A Galaxy legend. Slow auto-rotation that stops on interaction and is off under `prefers-reduced-motion`.

`PlanetList` is a keyboard-reachable list of all Planets, sorted by size. It is one toggle away at all times, and it replaces the canvas when WebGL is missing.

- [ ] **Step 3: Write e2e tests**

`filter tabs change the planet count`; `list item opens the planet panel`; `falls back to list without webgl`, using `addInitScript` to make `getContext('webgl2')` and `getContext('webgl')` return null; `one source hides the other tab`; `empty universe shows guidance`, with a mocked empty Universe.

- [ ] **Step 4: Run all checks, take a screenshot of the demo Universe, and commit**

Save the screenshot to `docs/images/universe.png`.

### Task 6: Digest, Claude Code Provider and the Roast screen

Ticket 06. Blocked by Task 4.

**Files:**
- Create: `crates/core/src/{digest,llm,roast}.rs`, `prompts/roast.md`, `crates/core/tests/provider_claude.rs`, `web/app/screens/Roast.svelte`
- Modify: `crates/core/src/{lib,store}.rs`, `src-tauri/src/commands.rs`, `web/shared/*`, `web/app/App.svelte`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Store`, `Universe`.
- Produces, camelCase serde:

```rust
pub fn digest::build(plays: &[Play], interests: &InterestMap, universe: &Universe,
                     utc_offset_secs: i32, now: i64) -> Digest;
pub enum Provider {
    ClaudeCode { model: Option<String> },
    Codex { model: Option<String> },                       // Task 7
    Local { base_url: String, model: String, api_key: Option<String> },  // Task 7
}
pub struct CompletionRequest { pub system: String, pub user: String, pub schema: serde_json::Value }
pub fn llm::complete(p: &Provider, req: &CompletionRequest, timeout: Duration) -> Result<String>;
pub fn llm::login_shell_path() -> Option<String>;   // PATH from `$SHELL -ilc`, cached in a OnceLock
pub struct Roast { pub headline: String, pub opening: String, pub burns: Vec<Burn>,
                   pub verdict: Verdict, pub galaxy_names: Vec<GalaxyName> }
pub struct Verdict { pub archetype: String, pub score: u8, pub sentence: String, pub evidence: Vec<String> }
pub struct StoredRoast { pub id: i64, pub created_at: i64, pub provider_label: String, pub roast: Roast }
pub fn roast::generate(p: &Provider, digest: &Digest, system: &str, timeout: Duration) -> Result<Roast>;
pub fn roast::default_system_prompt() -> &'static str;  // include_str!("../../../prompts/roast.md")
```

Also `Store::save_roast`, `Store::latest_roast`, and commands `generate_roast() -> StoredRoast`, `latest_roast() -> Option<StoredRoast>`, `digest_preview() -> Digest`. `generate_roast` emits a `roast-progress` Tauri event with a short phrase. A test-only binary override: `ROSTERY_CLAUDE_BIN` and `ROSTERY_CODEX_BIN`, read in `llm.rs` only under `#[cfg(any(test, debug_assertions))]`.

- [ ] **Step 1: Write failing Digest tests**

`top_creators_ranked_by_minutes`; `skip_rate_computed`; `longest_repeat_found` (same Item 7 times in a row); `late_night_uses_local_offset` (a Play at 23:00 UTC is late night at +0530, and not at -0500); `deleted_video_count`; `titles_sanitized` (control characters gone, 10,000 characters cut to 120); `digest_has_no_refs` (serialized output has no `spotify:track`, `watch?v=` or `http`); `size_stays_under_64kb` with 300,000 Plays; `galaxies_listed_with_members`; `one_source_leaves_other_none`.

- [ ] **Step 2: Write failing Provider tests in `tests/provider_claude.rs`**

A helper writes a fake `claude` shell script into a temp dir that records argv, stdin, cwd and env to files, then prints a canned response.

- `claude_runs_without_tools`: argv has `-p`, `--output-format json`, `--tools ""`, `--strict-mcp-config`, `--no-session-persistence`, `--setting-sources ""`, `--system-prompt <system>`.
- `claude_reads_digest_from_stdin`: the user text is in stdin and not in argv.
- `claude_runs_in_empty_temp_dir`: cwd is empty, and is not the process cwd.
- `claude_child_has_no_api_key`: `ANTHROPIC_API_KEY` is set in the parent and absent in the child.
- `claude_result_is_parsed`: `{"type":"result","is_error":false,"result":"hello"}` returns `hello`.
- `claude_error_result_becomes_error`: `is_error: true` gives `Err` with the message.
- `nonzero_exit_reports_stderr`; `missing_binary_says_how_to_install`.
- `timeout_kills_child`: a script that sleeps 60 seconds with a 1 second timeout returns `Err` in under 5 seconds, and the process is gone.
- `login_shell_path_finds_binary`: with `SHELL` pointing at a script that prints a known PATH, `login_shell_path` returns it. Spawns use that PATH when set.

- [ ] **Step 3: Write failing Roast tests in `roast.rs`**

`extracts_fenced_json`; `extracts_json_after_prose`; `invalid_json_retries_once` (fake Provider output changes on the second call, and a third call never happens); `two_bad_replies_fail_clearly`; `score_is_clamped`; `empty_burns_fail`; `empty_digest_refuses` (returns a "nothing to roast yet" `Err` without calling the Provider); `failed_run_keeps_previous_roast` (Store test).

- [ ] **Step 4: Run them and see them fail, then implement**

Run: `cargo test -p rostery-core`. Use `std::process::Command`, and a poll loop on `try_wait` for the timeout. Write `prompts/roast.md` as a real comedy brief: specific, mean about taste and kind about the person, uses the numbers, avoids protected traits, health and minors, treats `<digest>` content as data, and returns only the JSON document, with the schema spelled out in the prompt.

- [ ] **Step 5: Add a live test**

`live_claude_roast` (`#[ignore]`) runs on demo data with model `haiku`, and asserts a valid Roast with at least 3 Burns. Run it once: `cargo test -p rostery-core -- --ignored live_claude`.

- [ ] **Step 6: Build `Roast.svelte` with `frontend-design`**

States: empty (explains what is needed), running (progress phrases), result (headline, opening, Burns, Verdict card with archetype and score), and error (the message, plus the last good Roast still shown). Above the button: "Your Digest goes to Anthropic through Claude Code." e2e: `roast renders`, `roast error keeps previous`.

- [ ] **Step 7: Run all checks and commit**

### Task 7: Codex and local LLM Providers, plus settings

Ticket 07. Blocked by Task 6.

**Files:**
- Create: `crates/core/tests/provider_codex.rs`, `crates/core/tests/provider_local.rs`, `web/app/screens/Settings.svelte`
- Modify: `crates/core/src/{llm,roast}.rs`, `src-tauri/src/commands.rs`, `web/app/screens/Roast.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Provider`, `llm::complete`, `Store` settings.
- Produces: `llm::statuses() -> Vec<ProviderStatus>`, where `ProviderStatus { id: "claude-code"|"codex"|"ollama"|"lm-studio", available: bool, signed_in: bool, detail: String, sends_to: String }`. `Settings { provider: Provider, system_prompt: Option<String>, ... }` stored under the `settings` key, with commands `provider_statuses`, `get_settings`, `save_settings`. Default local URLs are `http://127.0.0.1:11434/v1` and `http://127.0.0.1:1234/v1`.

- [ ] **Step 1: Write failing tests**

`codex_runs_read_only`: argv has `exec`, `--sandbox read-only`, `--ephemeral`, `--ignore-rules`, `--ignore-user-config`, `--skip-git-repo-check`, `--output-schema <file>`, `-o <file>`, `-C <temp>`, `-c developer_instructions=...`, and ends with `-`. `codex_reads_prompt_from_stdin`. `codex_child_has_no_api_key`. `codex_reads_output_file`. `codex_schema_is_strict`: the schema file sets `additionalProperties: false` and lists every property in `required`, which Codex demands.

`local_posts_chat_completion` against `mockito`: `POST /chat/completions` with `model`, system and user messages, and `stream: false`. `local_http_error_is_readable`: 500 and connection refused both give a helpful message that names the URL. `local_api_key_sent_when_present`. `local_lists_models`: `GET /models`. `non_loopback_is_flagged`: `sends_to` warns for `http://192.168.1.5:11434/v1` and not for `127.0.0.1`.

`statuses_detect_cli_login`: fake `claude auth status` printing `{"loggedIn":true}` and fake `codex login status` exiting 0 show signed in; a nonzero exit shows signed out with a next step.

- [ ] **Step 2: Run them and see them fail, then implement**

Pass the strict JSON Schema for a Roast to Codex. If `codex exec --help` or its config shows a way to turn off the shell tool, add it and test it; otherwise leave the ceiling described in ADR 0003.

- [ ] **Step 3: Live test**

`live_codex_roast` (`#[ignore]`). Run it: `cargo test -p rostery-core -- --ignored live_codex`. Confirm `--ignore-user-config` keeps login working.

- [ ] **Step 4: Build `Settings.svelte` with `frontend-design`**

Provider cards with status and a fix hint, a model field, local URL field, editable system prompt with Reset, and a "See what gets sent" dialog showing the exact Digest JSON. The Roast screen's disclosure line now follows `sends_to`. e2e: `provider choice persists`, `digest preview opens`, `system prompt reset`.

- [ ] **Step 5: Run all checks and commit**

### Task 8: AI Interests and Galaxy names

Ticket 08. Blocked by Task 7.

**Files:**
- Create: `crates/core/src/interests.rs`, `prompts/interests.md`
- Modify: `crates/core/src/{lib,roast,store}.rs`, `src-tauri/src/commands.rs`, `web/app/screens/Roast.svelte`

**Interfaces:**
- Consumes: `llm::complete`, `Store::set_interests`, `universe::build`.
- Produces: `interests::tag_missing(p: &Provider, store: &mut Store, universe: &Universe, timeout: Duration) -> Result<u32>`, which tags up to the 80 heaviest Planets that have no `ai` Interests. `Store::latest_galaxy_names() -> HashMap<u32, String>`, used by `get_universe`. `generate_roast` runs: tag, rebuild Universe, Digest, Roast, save.

- [ ] **Step 1: Write failing tests**

`tags_are_normalised`: `" Lo-Fi "` becomes `lo-fi`, at most 3 per Creator, at most 32 characters, duplicates removed. `only_untagged_creators_are_sent`: the second run sends nothing and makes no Provider call. `invented_creators_are_ignored`: a name in the reply that was not sent is dropped. `tagging_failure_does_not_stop_the_roast`: a failing tag step still produces a Roast. `ai_tags_create_links`: after tagging, two Creators with no shared Sessions have a Link. `galaxy_names_apply_by_id`: `get_universe` after a Roast shows the names. `stale_galaxy_id_is_ignored`: a name for a Galaxy that no longer exists causes no error.

- [ ] **Step 2: Run them and see them fail, then implement**

- [ ] **Step 3: Live test and Stop Point 3 evidence**

`live_interests` (`#[ignore]`) on demo data through Claude Code. Save one sample Roast JSON in the report.

- [ ] **Step 4: Run all checks and commit**

### Task 9: Generate the Share Site

Ticket 09. Blocked by Tasks 5 and 8.

**Files:**
- Create: `web/share/{index.html,main.ts,Share.svelte,styles.css}`, `vite.share.config.ts`, `crates/core/src/export.rs`, `crates/core/examples/demo_site.rs`, `e2e/share.spec.ts`, `web/app/screens/Share.svelte`
- Modify: `package.json`, `crates/core/src/lib.rs`, `src-tauri/{Cargo.toml,src/commands.rs}`, `playwright.config.ts`, `web/shared/*`, `web/app/App.svelte`

**Interfaces:**
- Consumes: `UniverseView.svelte`, `Universe`, `Roast`, `Store`. Each Universe comes from `universe::from_store`, not `universe::build`, because that is what gives the Spotify and YouTube views their Roast names (`docs/design.md`, Roast names in each view).
- Produces, camelCase serde:

```rust
pub struct ExportOptions { pub display_name: String, pub include_roast: bool,
                           pub include_spotify: bool, pub include_youtube: bool,
                           pub include_item_titles: bool }   // Default: "Anonymous", false, true, true, false
pub struct SharePayload { pub display_name: String, pub generated_at: i64, pub roast: Option<Roast>,
                          pub universes: BTreeMap<UniverseFilter, Universe>, pub attribution: Vec<String> }
pub struct SiteFile { pub path: String, pub bytes: Vec<u8> }
pub fn export::build_payload(store: &Store, opts: &ExportOptions, now: i64) -> Result<SharePayload>;
pub fn export::render_site(template: &[SiteFile], payload: &SharePayload) -> Result<Vec<SiteFile>>;
pub fn export::write_site(files: &[SiteFile], dir: &Path) -> Result<()>;   // dir is owned by Rostery; replaces contents
```

`include_roast` defaults to true because sharing the Roast is the point of a Share Site. A Roast is free text: `prompts/roast.md` has every Burn quote a name from the Digest, and the Digest holds top Items and Creators for both Sources, so `include_item_titles` and `include_youtube` cannot filter what it says. The Share screen says so beside the choice, and the person can turn it off.

Commands `export_site(opts) -> ExportResult { dir: String, bytes: u64 }`, `open_site_preview()`, `reveal_site()`. `src-tauri` embeds `dist/share` with `include_dir!`. `web/share/index.html` has the markers `__ROSTERY_DATA__`, `__ROSTERY_TITLE__`, `__ROSTERY_DESC__`. Output from the Vite share build is exactly `index.html`, `assets/share.js`, `assets/share.css`, using a classic deferred script with no `type=module` so it runs from `file://`. Script `pnpm build` builds both. A meta CSP: `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; base-uri 'none'; form-action 'none'`, plus `<meta name="robots" content="noindex">`.

- [ ] **Step 1: Write failing Rust tests in `export.rs`**

- `escapes_script_breakout`: a Creator named `</script><img src=x onerror=alert(1)>` leaves no `</script><img` in `index.html`, and the JSON parses back equal.
- `title_is_html_escaped`: a Roast headline with `"><script>` is escaped in `<title>` and `og:` tags.
- `line_separators_escaped`: U+2028 and U+2029 are escaped.
- `excluded_source_is_absent`: `include_youtube: false` means a sentinel channel name appears in no file, with a stored Roast that quotes it and the default options.
- `excluded_roast_is_absent`.
- `roast_is_off_by_default`: `ExportOptions::default()` gives a payload with no Roast, though one is stored.
- `item_titles_off_by_default`: a sentinel Item title appears in no file, with a stored Roast that quotes it and the default options, and appears when the option is on.
- `included_roast_keeps_its_words`: `include_roast: true` with `include_youtube: false` and `include_item_titles: false` keeps a stored Roast's quoted title and channel. This is why the Share screen has to warn, so a change here has to change that warning too.
- `no_secrets_in_site`: a sentinel in the settings table appears in no file.
- `paths_are_relative`: `index.html` has no `="/` or `http` asset reference.
- `site_files_are_exact`: `index.html`, `assets/share.js`, `assets/share.css`, `.nojekyll`.
- `rewrite_removes_stale_files`.
- `attribution_lists_included_sources`.

- [ ] **Step 2: Run and see them fail, then implement `export.rs`**

- [ ] **Step 3: Build the Share Site with `frontend-design`**

A single scrolling page: Roast headline, opening, Burns, Verdict as a shareable moment, then the Universe through `UniverseView`, then attribution and "Made with Rostery. Generated on one person's machine." Readable and full-featured on a phone. Text renders only through Svelte text bindings, with no `{@html}`.

- [ ] **Step 4: Write `demo_site.rs` and `e2e/share.spec.ts`**

`cargo run -p rostery-core --example demo_site -- dist/share dist/demo-site [--hostile]` builds a site from demo data and a canned Roast, with `include_roast: true`. e2e, against both `file://` and an HTTP server under `/some/subpath/`: `roast and verdict show`; `universe tabs work`; `no third party requests` (every request is same-origin or file); `hostile payload stays inert` (with `--hostile`, no dialog fires, no injected `img`); `phone width has no sideways scroll`; `falls back to list without webgl`; `share js under budget` (gzip size under 350 KB).

- [ ] **Step 5: Build the app's Share screen**

Options, a live list of exactly what will be included, Export, Preview in browser, and Reveal folder. Deploy buttons come next.

The Roast option starts unticked. Beside it, always: "Your Roast quotes song and video titles and names from all your history, and the other choices here do not change its words. Read it before you include it." e2e: `including the roast is an explicit choice` (an export with nothing touched has no Roast, and the warning is on screen before the box can be ticked).

- [ ] **Step 6: Run all checks and commit**

### Task 10: One-click GitHub Pages

Ticket 10. Blocked by Task 9.

**Files:**
- Create: `crates/core/src/deploy/{mod,github}.rs`, `crates/core/tests/deploy_github.rs`, `src-tauri/src/secrets.rs`
- Modify: `crates/core/src/lib.rs`, `src-tauri/{Cargo.toml,src/commands.rs}`, `web/app/screens/Share.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `SiteFile`, `write_site`.
- Produces: `deploy::read_site(dir: &Path) -> Result<Vec<SiteFile>>`. `DeployResult { url: String, dashboard_url: String }`. `deploy::github::publish(api: &str, token: &str, repo: &str, site: &[SiteFile]) -> Result<DeployResult>`. `src-tauri/src/secrets.rs` has `get(name)`, `set(name, value)`, `delete(name)` over the `keyring` crate with service `dev.rostery.app`, with a clear error when no secret service exists on Linux. Commands `deploy_status() -> DeployStatus { github: Option<String>, vercel: bool }`, `save_deploy_token(target, token)`, `forget_deploy_token(target)`, `deploy_github(repo) -> DeployResult`. GitHub token lookup: keychain first, then `gh auth token`.

- [ ] **Step 1: Write failing `mockito` tests**

- `creates_repo_when_missing`: `GET /repos/o/r` 404, then `POST /user/repos` with `auto_init: true` and description containing `Published by Rostery`.
- `pushes_a_commit`: blobs for each file with base64, a tree without `base_tree`, a commit whose parent is the current `main`, then `PATCH refs/heads/main`.
- `updates_existing_rostery_repo`: marker present means no repo creation.
- `refuses_to_overwrite_foreign_repo`: description lacks the marker, returns `Err` naming the repo, and no write endpoint is called.
- `enables_pages_once`: `POST /pages` with `{source: {branch: "main", path: "/"}}`, and 409 counts as success.
- `reports_page_url`: `https://o.github.io/r/`, and repo `o.github.io` gives `https://o.github.io/`.
- `bad_token_says_reconnect`: 401.
- `invalid_repo_name_rejected`: only `[A-Za-z0-9._-]{1,100}`, with no call made.
- `waits_for_initial_commit`: right after creation, `main` 404s once and then succeeds.

- [ ] **Step 2: Run them and see them fail, then implement**

Send `Accept: application/vnd.github+json` and `X-GitHub-Api-Version` as the docs show.

- [ ] **Step 3: UI with `frontend-design`**

"Connect GitHub" connects at one click if `gh` is logged in. Otherwise it opens `https://github.com/settings/tokens/new?scopes=public_repo&description=Rostery` and shows a paste field. "Publish to GitHub Pages" asks for a repo name (default `rostery-universe`), says plainly that the repo and site are public, confirms, then shows the live URL with a Copy button and a note that Pages can take a minute. It exports fresh before publishing. e2e: `github publish shows url`, `publish asks for confirmation`.

- [ ] **Step 4: Run all checks and commit**

### Task 11: One-click Vercel

Ticket 11. Blocked by Task 10.

**Files:**
- Create: `crates/core/src/deploy/vercel.rs`, `crates/core/tests/deploy_vercel.rs`
- Modify: `crates/core/src/deploy/mod.rs`, `src-tauri/src/commands.rs`, `web/app/screens/Share.svelte`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `SiteFile`, `DeployResult`, `secrets`.
- Produces: `deploy::vercel::publish(api: &str, token: &str, project: &str, site: &[SiteFile], poll_every: Duration) -> Result<DeployResult>`, command `deploy_vercel(project) -> DeployResult`.

- [ ] **Step 1: Write failing `mockito` tests**

- `posts_inline_base64_files`: `POST /v13/deployments?skipAutoDetectionConfirmation=1` with `name`, `target: "production"`, `projectSettings.framework: null`, and each file as `{file, data, encoding: "base64"}`, including `.nojekyll` being left out since Vercel does not need it.
- `polls_until_ready`: `GET /v13/deployments/{id}` goes `QUEUED`, `BUILDING`, `READY`.
- `error_state_is_reported`: `ERROR` and `errorMessage`.
- `poll_gives_up`: stays `BUILDING` past the limit and reports it with the dashboard URL.
- `uses_production_alias`: prefers `alias[0]` over `url`, with `https://` added.
- `bad_token_says_reconnect`: 401 or 403.
- `project_name_is_normalised`: lowercase, invalid characters to `-`, trimmed to 100.

- [ ] **Step 2: Run them and see them fail, then implement**

Add a `ponytail:` comment noting that inline upload suits a small static site, and `POST /v2/files` is the upgrade path.

- [ ] **Step 3: UI**

"Connect Vercel" opens `https://vercel.com/account/tokens` and shows a paste field. Publish matches the GitHub flow. e2e: `vercel publish shows url`.

- [ ] **Step 4: Run all checks, walk Stop Point 4, and commit**

### Task 12: Spotify Connection

Ticket 12. Blocked by Tasks 2 and 10 (uses `secrets`).

**Files:**
- Create: `crates/core/src/{oauth,spotify_api}.rs`, `crates/core/tests/oauth.rs`, `crates/core/tests/spotify_sync.rs`, `web/app/components/ConnectSpotify.svelte`
- Modify: `crates/core/src/{lib,store}.rs`, `src-tauri/src/commands.rs`, `web/app/screens/Sources.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `Store`, `secrets`, `import_path`'s overlap rule.
- Produces:

```rust
pub struct OAuthConfig { pub auth_url: String, pub token_url: String, pub client_id: String,
                         pub client_secret: Option<String>, pub scopes: Vec<String>,
                         pub extra_auth_params: Vec<(String, String)>, pub port: u16 }
pub struct TokenSet { pub access_token: String, pub refresh_token: Option<String>, pub expires_in: u64 }
pub const REDIRECT_PORT: u16 = 43117;   // redirect is http://127.0.0.1:43117/callback
pub fn oauth::pkce_pair() -> (String, String);
pub fn oauth::challenge(verifier: &str) -> String;
pub fn oauth::authorize(cfg: &OAuthConfig, open_url: impl FnOnce(&str) -> Result<()>, timeout: Duration) -> Result<TokenSet>;
pub fn oauth::refresh(cfg: &OAuthConfig, refresh_token: &str) -> Result<TokenSet>;
pub fn spotify_api::sync(api: &str, access_token: &str, store: &mut Store, now: i64) -> Result<SyncSummary>;
```

`SyncSummary { plays_added: u64, affinities: u64, interests: u64 }`. Commands `connection_status() -> Connections { spotify: bool, youtube: bool }`, `connect_spotify(client_id, consent: bool)`, `sync_spotify() -> SyncSummary`, `disconnect_spotify()`. Keychain name `spotify_refresh`. Scopes `user-read-recently-played user-top-read`. `disconnect_spotify` also deletes `ai` rows in `interests`, including the empty ones that mean a Creator was asked about, for any Creator it leaves with no Play or Affinity, because Task 8 tags from what is stored.

- [ ] **Step 1: Write failing OAuth tests**

`pkce_challenge_matches_rfc7636`: verifier `dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk` gives challenge `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`. `full_flow_exchanges_code`: the test's `open_url` closure acts as the browser, parsing the URL and hitting the redirect; a `mockito` token endpoint receives `code`, `code_verifier` and `redirect_uri`. `wrong_state_is_rejected`. `provider_error_is_reported`: `?error=access_denied`. `timeout_returns_error`. `busy_port_says_so`. `only_binds_loopback`: a connect on a non-loopback address of the machine is refused. `refresh_keeps_old_refresh_token_when_absent`. `invalid_grant_says_reconnect`.

Make the listener handle stray requests such as `/favicon.ico` without ending the flow.

- [ ] **Step 2: Write failing sync tests**

`recent_plays_stored_with_api_origin`: track, first artist, `duration_ms`, `played_at`, URI. `top_artists_become_affinities_with_genres`: rank scores `(51 - rank) * 10`, kind `top_artist`, genres become Interests with origin `spotify_api`. `second_sync_uses_after_cursor`. `plays_before_latest_export_are_skipped`. `missing_genres_are_fine`. `rate_limit_respects_retry_after_once`. `disconnect_deletes_api_data_only`: Export Plays stay. `connect_requires_consent`: `consent: false` returns `Err` before opening anything.

- [ ] **Step 3: Run them and see them fail, then implement**

Generate random bytes with the `getrandom` crate, `sha2` for the digest, `base64` for encoding.

- [ ] **Step 4: UI with `frontend-design`**

`ConnectSpotify.svelte` shows a warning that an Export has more data and needs no setup, quotes the two policy clauses from ADR 0002, and has the consent checkbox. Then setup steps (Premium needed; create an app; add the exact redirect URI with a copy button; paste the Client ID) and Connect, Sync now, Disconnect. "Listening data from Spotify" attribution is visible. e2e: `connect blocked without consent`, `sync summary shows`, `disconnect clears status`.

- [ ] **Step 5: Run all checks and commit**

### Task 13: YouTube Connection

Ticket 13. Blocked by Task 12.

**Files:**
- Create: `crates/core/src/youtube_api.rs`, `crates/core/tests/youtube_sync.rs`, `web/app/components/ConnectYoutube.svelte`
- Modify: `crates/core/src/{lib,store}.rs`, `src-tauri/src/{main,commands}.rs`, `web/app/screens/Sources.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: `oauth`, `Store`, `secrets`.
- Produces: `youtube_api::sync(api: &str, access_token: &str, store: &mut Store, now: i64) -> Result<SyncSummary>`, `Store::purge_youtube_api_data(older_than: i64) -> Result<u64>`, called at app startup with `now - 30 days`. It and `disconnect_youtube` also delete `ai` rows in `interests` for any Creator they leave with no Play or Affinity, as in Task 12. Commands `connect_youtube(client_id, client_secret)`, `sync_youtube`, `disconnect_youtube`. Keychain names `google_client_secret`, `google_refresh`. Scope `https://www.googleapis.com/auth/youtube.readonly`. `extra_auth_params`: `access_type=offline`, `prompt=consent`.

- [ ] **Step 1: Write failing tests with `mockito`**

`subscriptions_follow_pages`: `subscriptions?part=snippet&mine=true&maxResults=50` over two pages gives Affinities of kind `subscription`, score 60. `liked_videos_become_affinities`: `videos?part=snippet&myRating=like&maxResults=50`, up to 200, kind `liked`, score 15 summed per channel. `topics_become_interests`: `channels?part=topicDetails` with `topicCategories` URL `https://en.wikipedia.org/wiki/Hip_hop_music` gives `hip hop music`, origin `youtube_api`. `channels_requested_in_batches_of_50`. `quota_exceeded_is_readable`: 403 with reason `quotaExceeded`. `purge_removes_old_api_data_only`: a 31 day old row goes, a 29 day old row stays, and Takeout Plays stay. `disconnect_deletes_api_data_only`.

- [ ] **Step 2: Run them and see them fail, then implement**

- [ ] **Step 3: UI with `frontend-design`**

`ConnectYoutube.svelte` says first that YouTube's API has no watch history and Takeout is required for the Universe to be real, then Google Cloud setup steps (desktop OAuth client, enable the YouTube Data API v3, add yourself as test user), a note that tokens last 7 days while the Google app is in Testing, links to the YouTube Terms of Service and Google Privacy Policy, and fields for client ID and secret. e2e: `youtube card explains takeout`, `sync summary shows`.

- [ ] **Step 4: Run all checks and commit**

### Task 14: First run and privacy controls

Ticket 14. Blocked by Tasks 11 and 13.

**Files:**
- Create: `PRIVACY.md`, `web/app/screens/Welcome.svelte`
- Modify: `crates/core/src/store.rs`, `src-tauri/src/commands.rs`, `web/app/App.svelte`, `web/app/screens/{Settings,Sources}.svelte`, `web/shared/*`, `e2e/app.spec.ts`

**Interfaces:**
- Consumes: everything above.
- Produces: `Store::wipe()`, command `wipe_everything(confirmation: String)`, which requires the literal `DELETE`, then removes all rows, every keychain entry and the site folder. Commands `save_local_key(server, key)` and `forget_local_key(server)`.

- [ ] **Step 1: Write failing tests**

`wipe_removes_all_rows`; `wipe_needs_exact_confirmation` (command helper, tested without Tauri); `local_key_reaches_the_provider`: a key from `secrets` is set on the Provider that `generate_roast` and `provider_statuses` use. e2e: `first run shows welcome`, `welcome skips after data exists`, `demo path reaches a universe`, `delete everything needs typed confirmation`, `local api key field`.

- [ ] **Step 2: Implement with `frontend-design`**

Welcome flow: what Rostery is in two sentences, add data (Export, Connection or demo), pick a Provider with live status, then Roast me. Every screen says what stays on the machine. `PRIVACY.md` turns the "What leaves the machine" table into a privacy policy, and states how to disconnect and delete, which Spotify's policy requires. It says plainly that the tag step sends Creator names, and that Codex also sends a person's own `~/.codex/AGENTS.md` with each call, which nothing stops (ADR 0003). Settings gets "Delete everything".

A local Provider's row in Settings gets an API key field, which is the only way to use a server that wants one. `api_key` on `Provider::Local` exists and is sent when present, but Task 7 left nothing setting it, because the keychain only arrives with Task 10. The key goes in `secrets` as `ollama-api-key` or `lm-studio-api-key` and never in the settings JSON. `src-tauri/src/commands.rs` reads it and sets it on the Provider before `llm::complete` and `llm::statuses`. The status and the error that say Rostery cannot send a key yet change to say where to add one, and the `ponytail:` comment on `api_key` goes.

- [ ] **Step 3: Run all checks and commit**

### Task 15: Ship it

Ticket 15. Blocked by Task 14.

**Files:**
- Create: `README.md`, `LICENSE` (MIT, copyright 2026 Prashant Kumar Yadav), `CONTRIBUTING.md`, `SECURITY.md`, `.github/workflows/{ci,release}.yml`, `scripts/make-icon.mjs`, `docs/images/*`
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/icons/*`, `package.json`

**Interfaces:**
- Consumes: all commands, e2e screenshots.
- Produces: `.dmg`, `.deb` and `.AppImage` bundles from `pnpm tauri build`.

- [ ] **Step 1: Create the real icon**

`scripts/make-icon.mjs` draws an original ringed planet to a 1024x1024 PNG with no new dependency (Playwright can render a canvas to PNG). Run `pnpm tauri icon`.

- [ ] **Step 2: Bundle configuration**

Targets `dmg`, `app`, `deb`, `appimage`. Linux deps are declared. Confirm the app's CSP and capabilities are the minimum in use.

- [ ] **Step 3: CI**

`ci.yml` on push and PR, with a matrix of `macos-latest` and `ubuntu-latest`: install `libwebkit2gtk-4.1-dev`, `libsecret-1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev` and `patchelf` on Linux, run pnpm install, build, `cargo fmt --check`, clippy, `cargo test --workspace`, `pnpm test`, Playwright with Chromium. `release.yml` on `v*` tags uses `tauri-apps/tauri-action` to build macOS (arm64 and x64) and Linux, then attaches them to a draft release. Pin actions to major versions.

- [ ] **Step 4: Docs**

README: what it does, a Universe screenshot and a Roast screenshot, install, the two Export paths, Provider setup, a plain privacy section that does not oversell ("local" has limits when you use a hosted Provider), and the ADR 0002 warning about Spotify Connections, an architecture diagram, dev setup and testing. Also `CONTRIBUTING.md` and `SECURITY.md`. Follow the `unslop` skill.

- [ ] **Step 5: Build the real bundle and smoke test**

Run: `pnpm tauri build`. Expected: `.dmg` under `target/release/bundle/dmg`. Launch the built binary for 10 seconds and confirm it stays up, and that the first-run screen is reachable through e2e.

- [ ] **Step 6: Run all checks and commit**
