# Graph Report - F:/Code/archiplayer  (2026-07-25)

## Corpus Check
- 68 files · ~388,231 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 686 nodes · 1383 edges · 40 communities (36 shown, 4 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 53 edges (avg confidence: 0.87)
- Token cost: 290,119 input · 0 output

## Community Hubs (Navigation)
- [[_COMMUNITY_WFMU Scraper & Parsers|WFMU Scraper & Parsers]]
- [[_COMMUNITY_Tauri Command Layer|Tauri Command Layer]]
- [[_COMMUNITY_Player State Machine|Player State Machine]]
- [[_COMMUNITY_SQLite Library Store|SQLite Library Store]]
- [[_COMMUNITY_NPM Package Manifest|NPM Package Manifest]]
- [[_COMMUNITY_Docs & Release Process|Docs & Release Process]]
- [[_COMMUNITY_Episode Download Pipeline|Episode Download Pipeline]]
- [[_COMMUNITY_Theme System|Theme System]]
- [[_COMMUNITY_Tauri App Config|Tauri App Config]]
- [[_COMMUNITY_Frontend API Types|Frontend API Types]]
- [[_COMMUNITY_Svelte Routes & Widgets|Svelte Routes & Widgets]]
- [[_COMMUNITY_Shows Page Screenshot|Shows Page Screenshot]]
- [[_COMMUNITY_Rate-Limited HTTP Fetcher|Rate-Limited HTTP Fetcher]]
- [[_COMMUNITY_Release PowerShell Script|Release PowerShell Script]]
- [[_COMMUNITY_Show Page Screenshot|Show Page Screenshot]]
- [[_COMMUNITY_Mini Player Screenshot|Mini Player Screenshot]]
- [[_COMMUNITY_App Shell Layout|App Shell Layout]]
- [[_COMMUNITY_Profile Page Screenshot|Profile Page Screenshot]]
- [[_COMMUNITY_Random Show Picker|Random Show Picker]]
- [[_COMMUNITY_Share & Toast Helpers|Share & Toast Helpers]]
- [[_COMMUNITY_Collapsed Player Screenshot|Collapsed Player Screenshot]]
- [[_COMMUNITY_TypeScript Config|TypeScript Config]]
- [[_COMMUNITY_Mini Volume Screenshot|Mini Volume Screenshot]]
- [[_COMMUNITY_Theme Picker Screenshot|Theme Picker Screenshot]]
- [[_COMMUNITY_Tauri Capabilities|Tauri Capabilities]]
- [[_COMMUNITY_Episode Scroll Helper|Episode Scroll Helper]]
- [[_COMMUNITY_Track Playback Guards|Track Playback Guards]]
- [[_COMMUNITY_Styling Guide|Styling Guide]]
- [[_COMMUNITY_Version Gate Script|Version Gate Script]]
- [[_COMMUNITY_Agent Handoff Notes|Agent Handoff Notes]]
- [[_COMMUNITY_App Type Declarations|App Type Declarations]]
- [[_COMMUNITY_Svelte Config|Svelte Config]]

## God Nodes (most connected - your core abstractions)
1. `Db` - 36 edges
2. `$lib/api` - 36 edges
3. `AppState` - 33 edges
4. `Player` - 32 edges
5. `$lib/player.svelte` - 21 edges
6. `$lib/share` - 16 edges
7. `scripts` - 15 edges
8. `Fetcher` - 15 edges
9. `download_episode_inner()` - 13 edges
10. `clean_text()` - 13 edges

## Surprising Connections (you probably didn't know these)
- `Local deterministic checks` --semantically_similar_to--> `Verify CI workflow`  [INFERRED] [semantically similar]
  CONTRIBUTING.md → .github/workflows/verify.yml
- `Archiplayer landing page (site/index.html)` --semantically_similar_to--> `Local-first architecture`  [INFERRED] [semantically similar]
  site/index.html → README.md
- `Archiplayer landing page (site/index.html)` --semantically_similar_to--> `WFMU polite cache-first fetch policy`  [INFERRED] [semantically similar]
  site/index.html → README.md
- `Bug report issue template` --conceptually_related_to--> `Archiplayer`  [INFERRED]
  .github/ISSUE_TEMPLATE/bug_report.yml → README.md
- `Archiplayer landing page (site/index.html)` --conceptually_related_to--> `Unsigned release builds`  [INFERRED]
  site/index.html → README.md

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Tag-to-published-release pipeline** — release_tag_driven_process, scripts_release_script, scripts_verify_release_version_version_gate, _github_workflows_release_release_workflow, _github_workflows_release_publish_job, _github_workflows_release_checksums_job, release_draft_release_gate [EXTRACTED 1.00]
- **Deterministic verification gates (local and CI)** — contributing_local_checks, _github_workflows_verify_verify_workflow, _github_workflows_verify_frontend_job, _github_workflows_verify_native_job [INFERRED 0.95]
- **Unsigned build trust and verification chain** — readme_unsigned_builds, _github_workflows_release_checksums_job, release_virustotal_verification, site_index_landing_page [INFERRED 0.85]

## Communities (40 total, 4 thin omitted)

### Community 0 - "WFMU Scraper & Parsers"
Cohesion: 0.06
Nodes (92): ElementRef, Regex, archive_for(), archive_years_are_newest_first(), archive_years_deduped_across_dropdown_and_anchors(), archive_years_empty_when_no_dropdown(), archive_years_exclude_the_show_page_itself(), archive_years_extracted_from_anchor_links() (+84 more)

### Community 1 - "Tauri Command Layer"
Cohesion: 0.07
Nodes (77): CmdResult, Cow, Download, Episode, File, From, LiveStatusSource, MutexGuard (+69 more)

### Community 2 - "Player State Machine"
Cohesion: 0.07
Nodes (16): api, Episode, LiveSong, LiveStatus, LiveStream, Track, isAbortError(), LiveRequest (+8 more)

### Community 3 - "SQLite Library Store"
Cohesion: 0.12
Nodes (19): Connection, Db, Download, Episode, live_playlist_merge_preserves_favourite_track_ids(), live_playlist_refresh_detects_missing_and_stale_snapshots(), live_shows_are_hidden_from_catalog(), provider_history_upgrades_observations_and_is_idempotent() (+11 more)

### Community 4 - "NPM Package Manifest"
Cohesion: 0.06
Nodes (34): dependencies, @fontsource/inter, @tauri-apps/api, @tauri-apps/plugin-dialog, @tauri-apps/plugin-opener, description, devDependencies, svelte (+26 more)

### Community 5 - "Docs & Release Process"
Cohesion: 0.08
Nodes (34): Bug report issue template, Pages deploy workflow, Release checksums job (SHA256SUMS.txt), Release publish job (Tauri 2-platform matrix), Release workflow, Verify frontend job (check, test, build), Verify native job (fmt, clippy, cargo test, no-bundle build), Verify CI workflow (+26 more)

### Community 6 - "Episode Download Pipeline"
Cohesion: 0.16
Nodes (18): Drop, ActiveDownloadGuard, build_name(), download_episode(), download_episode_inner(), download_names_are_portable_and_distinct(), DownloadGuard, DownloadProgress (+10 more)

### Community 7 - "Theme System"
Cohesion: 0.12
Nodes (12): $lib/theme.svelte, cssVar, Palette, Preset, presetById(), PRESETS, theme, ThemeStore (+4 more)

### Community 8 - "Tauri App Config"
Cohesion: 0.10
Nodes (20): app, security, windows, enable, scope, build, beforeBuildCommand, beforeDevCommand (+12 more)

### Community 9 - "Frontend API Types"
Cohesion: 0.11
Nodes (17): $lib/api, AudioSource, Download, DownloadRow, EpisodeStat, FavouriteEpisode, Favourites, FavouriteShow (+9 more)

### Community 10 - "Svelte Routes & Widgets"
Cohesion: 0.22
Nodes (9): @tauri-apps/api/event, $app/navigation, @tauri-apps/plugin-dialog, @tauri-apps/plugin-opener, $lib/CatalogNav.svelte, $lib/Icon.svelte, $lib/TrackRow.svelte, on (+1 more)

### Community 11 - "Shows Page Screenshot"
Cohesion: 0.18
Nodes (16): Archiplayer Shows Page Screenshot (2026-07-23), Alphabetical Filter Row (All, A-Z, #, sort toggle) with Favourites (11), Archiplayer Desktop App (WFMU), Archive Show List (title, DJ, ARCHIVE badge, description, star favourites), Dark Theme UI with Orange Accent Colors, I'm Feeling Lucky Today (random show feature with dice icon), Give the Drummer Radio Live Stream (WFMU eclectic web-only), Listen Live Now Section (4 live stream cards) (+8 more)

### Community 12 - "Rate-Limited HTTP Fetcher"
Cohesion: 0.23
Nodes (8): Client, Default, Duration, Fetcher, Instant, Mutex, Result, Self

### Community 13 - "Release PowerShell Script"
Cohesion: 0.23
Nodes (12): Assert-DraftAssets(), Assert-Preconditions(), Get-ReleaseByTag(), Invoke-Native(), Invoke-VersionGate(), New-AndPushTag(), New-ReleaseCommit(), Publish-Draft() (+4 more)

### Community 14 - "Show Page Screenshot"
Cohesion: 0.19
Nodes (15): Archiplayer Show Page Screenshot, A-Z Alphabetical Filter Row, Archiplayer Desktop App, Episode List with Per-Episode Actions, Favourites Feature (star toggle, Favourites 11), Now Playing: TotT Volume 30 (July 16, 2010, 30/33), Play All (newest first) Action, Persistent Player Bar (+7 more)

### Community 15 - "Mini Player Screenshot"
Cohesion: 0.23
Nodes (15): Archiplayer Desktop App, Bookmark Indicator (yellow flag), Bottom Navigation (Shows, Profile), Compact Mini-Player Window Layout, Episode Subtitle (Mid-'90s Rough Mix Rebroadcast), Favorite Star Toggle, Long-Form Radio Archive Playback, Archiplayer Mini Player UI Screenshot (+7 more)

### Community 16 - "App Shell Layout"
Cohesion: 0.14
Nodes (5): @fontsource/inter/400.css, @fontsource/inter/600.css, @fontsource/inter/700.css, @fontsource/inter/800.css, @tauri-apps/api/window

### Community 17 - "Profile Page Screenshot"
Cohesion: 0.19
Nodes (13): User Profile Page Screenshot, CSV Export (Favourites, Listening History, Stats Ranking), Native Desktop App Window (Tauri), Downloads Saved for Offline, Favourites (Shows 11, Episodes 19, Songs 32), Listening Stats (Totals, Audition Ranking, Most-listened Episodes), Live Streaming (Wake with Clay Pigeon, WFMU), Midnight Amber Default Theme (The Original, Warm Amber on Ink) (+5 more)

### Community 18 - "Random Show Picker"
Cohesion: 0.22
Nodes (8): Show, ShowDetail, $lib/random-show, randomIndex(), RandomPlaybackSelection, RandomSource, selectRandomPlayback(), shuffled()

### Community 19 - "Share & Toast Helpers"
Cohesion: 0.24
Nodes (12): $lib/share, shareContent(), ShareData, shareEpisode(), shareShow(), shareTrack(), wfmuEpisodeUrl(), wfmuShowUrl() (+4 more)

### Community 20 - "Collapsed Player Screenshot"
Cohesion: 0.32
Nodes (12): Archiplayer Collapsed Player Screenshot, Archiplayer Application, Bottom Navigation Bar (Shows, Profile, WFMU), Collapsed Compact Window Mode, Dark Theme UI with Orange Accent, Favorite Star Toggle on Track, LIVE Badge Indicator, Now Playing Track: Cradle - Man Is A Man (+4 more)

### Community 21 - "TypeScript Config"
Cohesion: 0.17
Nodes (11): compilerOptions, allowJs, checkJs, esModuleInterop, forceConsistentCasingInFileNames, moduleResolution, resolveJsonModule, skipLibCheck (+3 more)

### Community 22 - "Mini Volume Screenshot"
Cohesion: 0.36
Nodes (11): UI Screenshot: Mini Player Volume Control, Archiplayer Application, Bottom Navigation (Logo, Shows, Profile), Favorite Star and Bookmark Indicators, Mini Player Compact Window, Seek Bar with Elapsed/Total Time (18:11 / 1:16:02), Teal Brand Theme (logo, play button, accents), Episode Metadata: A Rough Mix, August 17 2012, Mid-'90s Rough Mix Rebroadcast (+3 more)

### Community 23 - "Theme Picker Screenshot"
Cohesion: 0.29
Nodes (10): App Themes Screenshot (Customize panel), Archiplayer Theme (Brandbook: teal, cream & navy), Per-Theme Colour Tokens (Accent, Star/gold, Background, Panel, Hover, Border, Text, Muted text, On accent, Danger, Brand line), Cream Paper Theme (Light: ink on paper, teal accent), Customize Panel (colour scheme & theme), Forest Gold Theme (Deep green & antique gold), Local Theme Persistence (Saved locally), Midnight Amber Theme (The original: warm amber on ink) (+2 more)

### Community 24 - "Tauri Capabilities"
Cohesion: 0.33
Nodes (5): description, identifier, permissions, $schema, windows

### Community 26 - "Track Playback Guards"
Cohesion: 0.70
Nodes (3): $lib/track-playback, canPlayExactTrack(), hasExactTrackTimestamp()

### Community 27 - "Styling Guide"
Cohesion: 0.50
Nodes (4): CSS variable sizing presets, :global() icon piercing pattern, Icon.svelte component, +layout.svelte :root design tokens

### Community 28 - "Version Gate Script"
Cohesion: 0.50
Nodes (3): packageJson, tauriConfig, versions

## Ambiguous Edges - Review These
- `Vertical Volume Slider Popup` → `Transport Controls Row (prev, rewind, skip -15s, play, skip +15s, forward, next)`  [AMBIGUOUS]
  UiMiniPlayerVoiceControl.jpg · relation: conceptually_related_to

## Knowledge Gaps
- **119 isolated node(s):** `name`, `version`, `description`, `type`, `dev` (+114 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What is the exact relationship between `Vertical Volume Slider Popup` and `Transport Controls Row (prev, rewind, skip -15s, play, skip +15s, forward, next)`?**
  _Edge tagged AMBIGUOUS (relation: conceptually_related_to) - confidence is low._
- **Why does `AppState` connect `Tauri Command Layer` to `SQLite Library Store`, `Rate-Limited HTTP Fetcher`, `Episode Download Pipeline`?**
  _High betweenness centrality (0.046) - this node is a cross-community bridge._
- **Why does `Db` connect `SQLite Library Store` to `Tauri Command Layer`?**
  _High betweenness centrality (0.038) - this node is a cross-community bridge._
- **Why does `$lib/player.svelte` connect `Player State Machine` to `App Shell Layout`, `Frontend API Types`, `Svelte Routes & Widgets`?**
  _High betweenness centrality (0.027) - this node is a cross-community bridge._
- **What connects `name`, `version`, `description` to the rest of the system?**
  _122 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `WFMU Scraper & Parsers` be split into smaller, more focused modules?**
  _Cohesion score 0.05601194921583271 - nodes in this community are weakly interconnected._
- **Should `Tauri Command Layer` be split into smaller, more focused modules?**
  _Cohesion score 0.06670584778136938 - nodes in this community are weakly interconnected._